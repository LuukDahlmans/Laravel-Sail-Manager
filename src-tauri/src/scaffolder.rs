use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::error::{AppError, AppResult};
use crate::models::{Port, PortService, ServiceKind};
use crate::sail::{ProcessOutput, OUTPUT_EVENT};

pub async fn scaffold(
    app: &AppHandle,
    project_id: &str,
    name: &str,
    php_version: &str,
    services: &[ServiceKind],
    custom_services: &[String],
    projects_root: &Path,
) -> AppResult<PathBuf> {
    let project_path = projects_root.join(name);
    if project_path.exists() {
        return Err(AppError::Scaffold(format!(
            "path already exists: {}",
            project_path.display()
        )));
    }

    tokio::fs::create_dir_all(projects_root).await?;

    let (uid, gid) = current_uid_gid().await?;
    let image = php_image_for(php_version);
    let services_arg = combined_services_arg(services, custom_services);

    let with_arg = if services_arg.is_empty() {
        "mysql".to_string()
    } else {
        services_arg
    };
    let bash_cmd = format!(
        "set -e && \
         laravel new {name} --no-interaction && \
         cd {name} && \
         composer require laravel/sail --dev --no-interaction && \
         php artisan sail:install --no-interaction --with={with_arg}"
    );

    emit_line(app, project_id, "stdout", format!("Pulling image {image}…"));

    let mut cmd = Command::new("docker");
    cmd.args(["run", "--rm", "--pull=always", "-v"]);
    cmd.arg(format!("{}:/opt", projects_root.display()));
    cmd.args(["-w", "/opt"]);
    cmd.args(["-e", "WWWUSER", "-e", "WWWGROUP"]);
    cmd.env("WWWUSER", uid.to_string());
    cmd.env("WWWGROUP", gid.to_string());
    cmd.arg(&image);
    cmd.args(["bash", "-c", &bash_cmd]);

    let run_result = run_streaming(app, project_id, &mut cmd).await;
    if run_result.is_err() && project_path.exists() {
        let _ = tokio::fs::remove_dir_all(&project_path).await;
    }
    run_result?;

    if !project_path.exists() {
        return Err(AppError::Scaffold(
            "scaffold appeared to succeed but project folder does not exist".into(),
        ));
    }

    Ok(project_path)
}

pub async fn customize_env(project_path: &Path, name: &str, ports: &[Port]) -> AppResult<()> {
    let env_path = project_path.join(".env");
    let original = tokio::fs::read_to_string(&env_path)
        .await
        .unwrap_or_default();

    let (uid, gid) = current_uid_gid().await.unwrap_or((501, 20));
    let app_port = port_value(ports, PortService::App).unwrap_or(80);
    let mut overrides: Vec<(&str, String)> = vec![
        ("APP_NAME", name.to_string()),
        ("APP_PORT", app_port.to_string()),
        ("APP_URL", format!("http://localhost:{app_port}")),
        ("COMPOSE_PROJECT_NAME", name.to_string()),
        ("WWWUSER", uid.to_string()),
        ("WWWGROUP", gid.to_string()),
    ];
    if let Some(p) = port_value(ports, PortService::Vite) {
        overrides.push(("VITE_PORT", p.to_string()));
    }
    // Sail uses FORWARD_DB_PORT for any of mysql/mariadb/pgsql; pick whichever exists.
    let db_port = port_value(ports, PortService::Mysql)
        .or_else(|| port_value(ports, PortService::Mariadb))
        .or_else(|| port_value(ports, PortService::Pgsql));
    if let Some(p) = db_port {
        overrides.push(("FORWARD_DB_PORT", p.to_string()));
    }
    if let Some(p) = port_value(ports, PortService::Redis) {
        overrides.push(("FORWARD_REDIS_PORT", p.to_string()));
    }
    if let Some(p) = port_value(ports, PortService::Valkey) {
        overrides.push(("FORWARD_VALKEY_PORT", p.to_string()));
    }
    if let Some(p) = port_value(ports, PortService::Memcached) {
        overrides.push(("FORWARD_MEMCACHED_PORT", p.to_string()));
    }
    if let Some(p) = port_value(ports, PortService::MailpitSmtp) {
        overrides.push(("FORWARD_MAILPIT_PORT", p.to_string()));
    }
    if let Some(p) = port_value(ports, PortService::MailpitUi) {
        overrides.push(("FORWARD_MAILPIT_DASHBOARD_PORT", p.to_string()));
    }
    if let Some(p) = port_value(ports, PortService::Meilisearch) {
        overrides.push(("FORWARD_MEILISEARCH_PORT", p.to_string()));
    }
    if let Some(p) = port_value(ports, PortService::Typesense) {
        overrides.push(("FORWARD_TYPESENSE_PORT", p.to_string()));
    }
    if let Some(p) = port_value(ports, PortService::Mongodb) {
        overrides.push(("FORWARD_MONGODB_PORT", p.to_string()));
    }
    if let Some(p) = port_value(ports, PortService::Minio) {
        overrides.push(("FORWARD_MINIO_PORT", p.to_string()));
    }
    if let Some(p) = port_value(ports, PortService::MinioConsole) {
        overrides.push(("FORWARD_MINIO_CONSOLE_PORT", p.to_string()));
    }
    if let Some(p) = port_value(ports, PortService::Selenium) {
        overrides.push(("FORWARD_SELENIUM_PORT", p.to_string()));
    }
    if let Some(p) = port_value(ports, PortService::Soketi) {
        overrides.push(("FORWARD_SOKETI_PORT", p.to_string()));
    }

    let updated = apply_env_overrides(&original, &overrides);
    tokio::fs::write(&env_path, updated).await?;
    Ok(())
}

fn port_value(ports: &[Port], service: PortService) -> Option<u16> {
    ports.iter().find(|p| p.service == service).map(|p| p.host)
}

fn apply_env_overrides(original: &str, overrides: &[(&str, String)]) -> String {
    let mut applied = vec![false; overrides.len()];
    let mut out = String::with_capacity(original.len() + 256);

    for line in original.lines() {
        let mut replaced = false;
        let trimmed = line.trim_start();
        if !trimmed.starts_with('#') {
            if let Some(eq) = line.find('=') {
                let key = line[..eq].trim();
                if let Some(idx) = overrides.iter().position(|(k, _)| *k == key) {
                    out.push_str(overrides[idx].0);
                    out.push('=');
                    out.push_str(&overrides[idx].1);
                    out.push('\n');
                    applied[idx] = true;
                    replaced = true;
                }
            }
        }
        if !replaced {
            out.push_str(line);
            out.push('\n');
        }
    }

    let missing: Vec<_> = overrides
        .iter()
        .enumerate()
        .filter(|(i, _)| !applied[*i])
        .map(|(_, kv)| kv)
        .collect();
    if !missing.is_empty() {
        if !out.ends_with("\n\n") {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }
        out.push_str("# --- Sail Manager overrides ---\n");
        for (k, v) in missing {
            out.push_str(k);
            out.push('=');
            out.push_str(v);
            out.push('\n');
        }
    }

    out
}

fn php_image_for(php_version: &str) -> String {
    let compact = php_version.replace('.', "");
    format!("laravelsail/php{compact}-composer:latest")
}

fn combined_services_arg(services: &[ServiceKind], custom: &[String]) -> String {
    let mut parts: Vec<String> = services
        .iter()
        .map(|s| s.sail_install_arg().to_string())
        .collect();
    for c in custom {
        let trimmed = c.trim();
        if !trimmed.is_empty() {
            parts.push(trimmed.to_string());
        }
    }
    parts.join(",")
}

async fn current_uid_gid() -> AppResult<(u32, u32)> {
    let uid_out = Command::new("id").arg("-u").output().await?;
    let gid_out = Command::new("id").arg("-g").output().await?;
    let parse = |bytes: &[u8]| -> AppResult<u32> {
        let s = String::from_utf8_lossy(bytes);
        s.trim()
            .parse::<u32>()
            .map_err(|e| AppError::Other(format!("parse uid/gid: {e}")))
    };
    Ok((parse(&uid_out.stdout)?, parse(&gid_out.stdout)?))
}

fn emit_line(app: &AppHandle, project_id: &str, stream: &str, line: String) {
    let _ = app.emit(
        OUTPUT_EVENT,
        ProcessOutput {
            project_id: project_id.into(),
            stream: stream.into(),
            line,
        },
    );
}

async fn run_streaming(app: &AppHandle, project_id: &str, cmd: &mut Command) -> AppResult<()> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::Scaffold(format!("spawn failed: {e}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Scaffold("missing stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Scaffold("missing stderr".into()))?;

    let buf = Arc::new(Mutex::new(Vec::<String>::new()));

    let app_out = app.clone();
    let pid_out = project_id.to_string();
    let buf_out = buf.clone();
    let stdout_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            buf_out.lock().unwrap().push(line.clone());
            let _ = app_out.emit(
                OUTPUT_EVENT,
                ProcessOutput {
                    project_id: pid_out.clone(),
                    stream: "stdout".into(),
                    line,
                },
            );
        }
    });

    let app_err = app.clone();
    let pid_err = project_id.to_string();
    let buf_err = buf.clone();
    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            buf_err.lock().unwrap().push(line.clone());
            let _ = app_err.emit(
                OUTPUT_EVENT,
                ProcessOutput {
                    project_id: pid_err.clone(),
                    stream: "stderr".into(),
                    line,
                },
            );
        }
    });

    let status = child
        .wait()
        .await
        .map_err(|e| AppError::Scaffold(format!("wait failed: {e}")))?;
    stdout_task.await.ok();
    stderr_task.await.ok();

    if !status.success() {
        let lines = buf.lock().unwrap();
        let tail: Vec<&str> = lines
            .iter()
            .rev()
            .take(15)
            .rev()
            .map(|s| s.as_str())
            .collect();
        let preview = tail.join("\n");
        return Err(AppError::Scaffold(format!(
            "exit {:?}\n---\n{}",
            status.code(),
            preview
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{apply_env_overrides, combined_services_arg, php_image_for};
    use crate::models::ServiceKind;

    #[test]
    fn replaces_existing_key_in_place() {
        let original = "APP_NAME=Old\nAPP_PORT=80\nDB_HOST=mysql\n";
        let updated = apply_env_overrides(
            original,
            &[
                ("APP_NAME", "Sail".to_string()),
                ("APP_PORT", "8123".to_string()),
            ],
        );
        assert!(updated.contains("APP_NAME=Sail\n"));
        assert!(updated.contains("APP_PORT=8123\n"));
        assert!(updated.contains("DB_HOST=mysql\n"));
        assert!(!updated.contains("APP_NAME=Old"));
        assert!(!updated.contains("APP_PORT=80\n"));
    }

    #[test]
    fn appends_missing_keys_in_managed_block() {
        let original = "APP_NAME=Sail\n";
        let updated = apply_env_overrides(
            original,
            &[
                ("APP_NAME", "Sail".to_string()),
                ("APP_PORT", "8123".to_string()),
            ],
        );
        assert!(updated.contains("# --- Sail Manager overrides ---"));
        assert!(updated.contains("APP_PORT=8123\n"));
        // Non-missing key is NOT appended again.
        assert_eq!(updated.matches("APP_NAME=").count(), 1);
    }

    #[test]
    fn preserves_comments_unchanged() {
        let original = "# database settings\nDB_HOST=mysql\n# end\n";
        let updated = apply_env_overrides(original, &[("DB_HOST", "localhost".to_string())]);
        assert!(updated.contains("# database settings\n"));
        assert!(updated.contains("# end\n"));
        assert!(updated.contains("DB_HOST=localhost\n"));
    }

    #[test]
    fn does_not_replace_inside_commented_lines() {
        // A commented "# APP_NAME=foo" should not be treated as a key we can
        // replace — apply_env_overrides should leave it alone and append the
        // override fresh.
        let original = "# APP_NAME=commented-out\n";
        let updated = apply_env_overrides(original, &[("APP_NAME", "Sail".to_string())]);
        assert!(updated.contains("# APP_NAME=commented-out\n"));
        assert!(updated.contains("APP_NAME=Sail\n"));
        assert!(updated.contains("# --- Sail Manager overrides ---"));
    }

    #[test]
    fn separates_managed_block_with_blank_line() {
        let original = "APP_NAME=Sail\n";
        let updated = apply_env_overrides(original, &[("APP_PORT", "8000".to_string())]);
        // There should be a blank line before the managed block header.
        let header = "# --- Sail Manager overrides ---";
        let idx = updated.find(header).expect("managed block header present");
        let before = &updated[..idx];
        assert!(before.ends_with("\n\n"), "got: {before:?}");
    }

    #[test]
    fn handles_empty_original_file() {
        let updated = apply_env_overrides("", &[("APP_PORT", "8000".to_string())]);
        assert!(updated.contains("# --- Sail Manager overrides ---"));
        assert!(updated.contains("APP_PORT=8000\n"));
    }

    #[test]
    fn handles_original_without_trailing_newline() {
        let original = "APP_NAME=Sail";
        let updated = apply_env_overrides(original, &[("APP_PORT", "8000".to_string())]);
        assert!(updated.contains("APP_NAME=Sail\n"));
        assert!(updated.contains("APP_PORT=8000\n"));
    }

    #[test]
    fn preserves_value_with_equals_signs_in_other_lines() {
        // We only split on the FIRST '=' to identify the key; values like
        // a=b=c on other lines should not be touched.
        let original = "DATABASE_URL=postgres://u:p=word@h/db\nAPP_NAME=Old\n";
        let updated = apply_env_overrides(original, &[("APP_NAME", "Sail".to_string())]);
        assert!(updated.contains("DATABASE_URL=postgres://u:p=word@h/db\n"));
        assert!(updated.contains("APP_NAME=Sail\n"));
    }

    #[test]
    fn replaces_only_keys_listed_even_if_others_match() {
        let original = "APP_NAME=Old\nAPP_PORT=80\n";
        let updated = apply_env_overrides(original, &[("APP_NAME", "Sail".to_string())]);
        // APP_PORT should still read 80 — we did not pass an override for it.
        assert!(updated.contains("APP_NAME=Sail\n"));
        assert!(updated.contains("APP_PORT=80\n"));
    }

    #[test]
    fn php_image_strips_dot_from_version() {
        assert_eq!(php_image_for("8.3"), "laravelsail/php83-composer:latest");
        assert_eq!(php_image_for("8.4"), "laravelsail/php84-composer:latest");
        assert_eq!(php_image_for("7.4"), "laravelsail/php74-composer:latest");
    }

    #[test]
    fn combined_services_arg_joins_with_commas() {
        let arg = combined_services_arg(&[ServiceKind::Mysql, ServiceKind::Redis], &[]);
        assert_eq!(arg, "mysql,redis");
    }

    #[test]
    fn combined_services_arg_appends_custom_services() {
        let arg = combined_services_arg(
            &[ServiceKind::Mysql],
            &[
                "clickhouse".to_string(),
                "  ".to_string(),
                "kafka".to_string(),
            ],
        );
        assert_eq!(arg, "mysql,clickhouse,kafka");
    }

    #[test]
    fn combined_services_arg_empty_when_nothing_provided() {
        assert_eq!(combined_services_arg(&[], &[]), "");
    }
}
