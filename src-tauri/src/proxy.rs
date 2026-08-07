use std::path::Path;

use tokio::process::Command;

use crate::error::{AppError, AppResult};
use crate::models::{PortService, Project};

const CONTAINER_NAME: &str = "sail-manager-proxy";
const IMAGE: &str = "traefik:v3.0";
const HTTPS_PORT: u16 = 443;

/// HTTPS configuration passed through to Traefik. When `Some`, the container
/// also binds `:443` and mounts the host cert directory at `/tls` so the file
/// provider can pick up the wildcard cert.
pub struct TlsRuntime<'a> {
    pub tls_dir: &'a Path,
}

pub async fn ensure_running(
    conf_dir: &Path,
    proxy_port: u16,
    tls: Option<&TlsRuntime<'_>>,
) -> AppResult<()> {
    tokio::fs::create_dir_all(conf_dir).await?;

    // If our proxy container already exists, check whether the requested host
    // port is actually bound to it. Docker on macOS occasionally creates the
    // container with empty PortBindings when the port is contested. Recreate
    // the container in that case rather than silently leaving a useless one.
    // We also recreate when the TLS mode has changed (added or removed :443).
    let inspect = Command::new("docker")
        .args([
            "inspect",
            CONTAINER_NAME,
            "--format",
            "{{.State.Running}}|{{json .NetworkSettings.Ports}}",
        ])
        .output()
        .await?;

    if inspect.status.success() {
        let raw = String::from_utf8_lossy(&inspect.stdout);
        let trimmed = raw.trim();
        let mut parts = trimmed.splitn(2, '|');
        let running = parts.next().unwrap_or("").trim() == "true";
        let ports = parts.next().unwrap_or("");
        let port_published = ports.contains(&format!("\"HostPort\":\"{proxy_port}\""));
        let https_published = ports.contains(&format!("\"HostPort\":\"{HTTPS_PORT}\""));
        let want_https = tls.is_some();

        if running && port_published && https_published == want_https {
            return Ok(());
        }

        // Either stopped, port-unbound, or TLS-mode mismatch: tear down and recreate.
        let _ = stop().await;
    }

    // Before docker run, check no other Docker container has the host ports
    // we're about to claim.
    if let Some(holder) = port_holder(proxy_port).await? {
        return Err(AppError::Other(format!(
            "Port {proxy_port} is already in use by container '{holder}'. \
             Stop that container (or change the proxy port in Settings) and try again."
        )));
    }
    if tls.is_some() {
        if let Some(holder) = port_holder(HTTPS_PORT).await? {
            return Err(AppError::Other(format!(
                "Port {HTTPS_PORT} is already in use by container '{holder}'. \
                 Stop that container or disable HTTPS in Settings."
            )));
        }
    }

    // A host process (not a Docker container) can also hold these ports —
    // Laravel Herd, Valet, Apache, or nginx all bind :80/:443. `docker ps`
    // won't see them, so without this probe `docker run` fails cryptically or
    // (per the macOS silent-bind bug) creates an unbound container. Name the
    // likely culprit so a new user isn't stuck. Our own container is stopped
    // at this point, so a failed bind means someone else holds the port.
    if !host_port_free(proxy_port) {
        return Err(AppError::Other(non_docker_holder_message(proxy_port)));
    }
    if tls.is_some() && !host_port_free(HTTPS_PORT) {
        return Err(AppError::Other(non_docker_holder_message(HTTPS_PORT)));
    }

    let port_arg = format!("{proxy_port}:80");
    let mount_arg = format!("{}:/conf", conf_dir.display());

    let mut args: Vec<String> = vec![
        "run".into(),
        "-d".into(),
        "--name".into(),
        CONTAINER_NAME.into(),
        "--restart".into(),
        "unless-stopped".into(),
        "-p".into(),
        port_arg,
        "-v".into(),
        mount_arg,
    ];

    let https_port_arg;
    let tls_mount_arg;
    if let Some(t) = tls {
        https_port_arg = format!("{HTTPS_PORT}:443");
        args.push("-p".into());
        args.push(https_port_arg);

        tls_mount_arg = format!("{}:/tls:ro", t.tls_dir.display());
        args.push("-v".into());
        args.push(tls_mount_arg);
    }

    args.push(IMAGE.into());
    args.push("--providers.file.directory=/conf".into());
    args.push("--providers.file.watch=true".into());
    args.push("--entrypoints.web.address=:80".into());
    if tls.is_some() {
        args.push("--entrypoints.websecure.address=:443".into());
    }

    let out = Command::new("docker").args(&args).output().await?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(AppError::Other(format!(
            "could not start proxy container on port {proxy_port}{}. Is something else bound to that port?\n{stderr}",
            if tls.is_some() { " / 443" } else { "" },
        )));
    }

    // Verify the port mapping actually took effect — Docker Desktop on macOS
    // sometimes creates the container with empty bindings when contested.
    let verify = Command::new("docker")
        .args([
            "inspect",
            CONTAINER_NAME,
            "--format",
            "{{json .NetworkSettings.Ports}}",
        ])
        .output()
        .await?;
    let bindings = String::from_utf8_lossy(&verify.stdout);
    if !bindings.contains(&format!("\"HostPort\":\"{proxy_port}\"")) {
        let _ = stop().await;
        return Err(AppError::Other(format!(
            "Proxy container started but port {proxy_port} did not bind to the host. Another process likely grabbed it after the conflict check."
        )));
    }
    if tls.is_some() && !bindings.contains(&format!("\"HostPort\":\"{HTTPS_PORT}\"")) {
        let _ = stop().await;
        return Err(AppError::Other(format!(
            "Proxy container started but port {HTTPS_PORT} did not bind to the host. Another process likely grabbed it after the conflict check."
        )));
    }
    Ok(())
}

/// True if `port` can be bound on the host right now (i.e. nothing is
/// listening). Probes both IPv4 and IPv6 wildcards — Docker/Herd may hold
/// either — mirroring the port allocator's free-check.
fn host_port_free(port: u16) -> bool {
    use std::net::{Ipv4Addr, Ipv6Addr, TcpListener};
    let v4 = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).is_ok();
    let v6 = TcpListener::bind((Ipv6Addr::UNSPECIFIED, port)).is_ok();
    v4 && v6
}

fn non_docker_holder_message(port: u16) -> String {
    let hint = if port == 80 || port == HTTPS_PORT {
        " If you run Laravel Herd or Valet, quit it — they hold ports 80 and 443. \
         Otherwise it may be Apache or nginx."
    } else {
        ""
    };
    format!(
        "Port {port} is already in use by another program on your Mac (not a Docker container).{hint} \
         You can also turn off Local URLs (or HTTPS) in Settings and open projects at localhost:<port> instead."
    )
}

async fn port_holder(port: u16) -> AppResult<Option<String>> {
    let out = Command::new("docker")
        .args([
            "ps",
            "--filter",
            &format!("publish={port}"),
            "--format",
            "{{.Names}}",
        ])
        .output()
        .await?;
    let name = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .find(|s| !s.is_empty() && s != CONTAINER_NAME);
    Ok(name)
}

pub async fn stop() -> AppResult<()> {
    let _ = Command::new("docker")
        .args(["stop", CONTAINER_NAME])
        .output()
        .await;
    let _ = Command::new("docker")
        .args(["rm", CONTAINER_NAME])
        .output()
        .await;
    Ok(())
}

pub async fn write_config(
    conf_dir: &Path,
    projects: &[Project],
    tld: &str,
    https_enabled: bool,
) -> AppResult<()> {
    tokio::fs::create_dir_all(conf_dir).await?;
    let path = conf_dir.join("dynamic.yml");
    // WHY: Traefik rejects a config whose `http:` block has no routers/services
    // ("http cannot be a standalone element") and spams the proxy logs with
    // errors on a fresh install with zero projects. With nothing to emit we
    // remove the file entirely — an empty watched directory is valid.
    match build_yaml(projects, tld, https_enabled) {
        Some(yaml) => tokio::fs::write(path, yaml).await?,
        None => {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
    Ok(())
}

fn build_yaml(projects: &[Project], tld: &str, https_enabled: bool) -> Option<String> {
    let routable: Vec<&Project> = projects
        .iter()
        .filter(|p| {
            p.ports
                .iter()
                .any(|x| matches!(x.service, PortService::App))
        })
        .collect();
    if routable.is_empty() && !https_enabled {
        return None;
    }

    let mut yaml = String::from("# Auto-generated by Sail Manager. Do not edit by hand.\n");
    if !routable.is_empty() {
        yaml.push_str("http:\n  routers:\n");
        for p in &routable {
            let key = sanitize(&p.compose_project_name);
            yaml.push_str(&format!("    {key}:\n"));
            yaml.push_str(&format!(
                "      rule: \"Host(`{name}.{tld}`)\"\n",
                name = p.name,
                tld = tld
            ));
            yaml.push_str(&format!("      service: {key}\n"));
            if https_enabled {
                yaml.push_str("      entryPoints:\n        - web\n        - websecure\n");
                yaml.push_str("      tls: {}\n");
            } else {
                yaml.push_str("      entryPoints:\n        - web\n");
            }
        }
        yaml.push_str("  services:\n");
        for p in &routable {
            let key = sanitize(&p.compose_project_name);
            let app_port = p
                .ports
                .iter()
                .find(|x| matches!(x.service, PortService::App))
                .map(|x| x.host)
                .unwrap_or(80);
            yaml.push_str(&format!("    {key}:\n"));
            yaml.push_str("      loadBalancer:\n        servers:\n");
            yaml.push_str(&format!(
                "          - url: \"http://host.docker.internal:{app_port}\"\n"
            ));
        }
    }
    if https_enabled {
        // Tell Traefik where the wildcard cert lives + use it as the default
        // certificate so a SNI miss (or a fresh container that hasn't seen
        // the cert added to its SNI map yet) doesn't fall through to
        // Traefik's built-in self-signed "TRAEFIK DEFAULT CERT". Filenames
        // must match what tls::ensure_wildcard_cert writes for this TLD.
        yaml.push_str("tls:\n");
        yaml.push_str("  certificates:\n");
        yaml.push_str(&format!(
            "    - certFile: /tls/{tld}-wildcard.pem\n      keyFile: /tls/{tld}-wildcard-key.pem\n"
        ));
        yaml.push_str("  stores:\n");
        yaml.push_str("    default:\n");
        yaml.push_str("      defaultCertificate:\n");
        yaml.push_str(&format!(
            "        certFile: /tls/{tld}-wildcard.pem\n        keyFile: /tls/{tld}-wildcard-key.pem\n"
        ));
    }
    Some(yaml)
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Port, ProjectStatus, StarterKit};
    use chrono::Utc;

    fn project(name: &str, app_port: u16) -> Project {
        Project {
            id: name.to_string(),
            name: name.to_string(),
            compose_project_name: name.to_string(),
            path: format!("/tmp/{name}"),
            status: ProjectStatus::Stopped,
            starter_kit: StarterKit::None,
            php_version: "8.3".into(),
            services: vec![],
            ports: vec![Port {
                service: PortService::App,
                label: "App".into(),
                host: app_port,
            }],
            created_at: Utc::now(),
            last_started: None,
        }
    }

    #[test]
    fn no_projects_no_https_yields_no_config() {
        // Traefik rejects `http:` with empty routers/services — must emit nothing.
        assert_eq!(build_yaml(&[], "test", false), None);
    }

    #[test]
    fn no_projects_with_https_yields_tls_only() {
        let yaml = build_yaml(&[], "test", true).unwrap();
        assert!(!yaml.contains("http:"), "must not emit an empty http block");
        assert!(yaml.contains("tls:"));
        assert!(yaml.contains("certFile: /tls/test-wildcard.pem"));
    }

    #[test]
    fn project_without_app_port_is_not_routable() {
        let mut p = project("noapp", 8080);
        p.ports.clear();
        assert_eq!(build_yaml(&[p], "test", false), None);
    }

    #[test]
    fn projects_yield_router_and_service() {
        let yaml = build_yaml(&[project("shop", 8081)], "test", false).unwrap();
        assert!(yaml.contains("rule: \"Host(`shop.test`)\""));
        assert!(yaml.contains("http://host.docker.internal:8081"));
        assert!(yaml.contains("  routers:\n    shop:"));
        assert!(yaml.contains("  services:\n    shop:"));
    }
}
