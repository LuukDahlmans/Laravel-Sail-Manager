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
    let yaml = build_yaml(projects, tld, https_enabled);
    tokio::fs::write(conf_dir.join("dynamic.yml"), yaml).await?;
    Ok(())
}

fn build_yaml(projects: &[Project], tld: &str, https_enabled: bool) -> String {
    let mut yaml =
        String::from("# Auto-generated by Sail Manager. Do not edit by hand.\nhttp:\n  routers:\n");
    let mut routable: Vec<&Project> = Vec::new();
    for p in projects {
        let has_app = p
            .ports
            .iter()
            .any(|x| matches!(x.service, PortService::App));
        if !has_app {
            continue;
        }
        routable.push(p);
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
    yaml
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
