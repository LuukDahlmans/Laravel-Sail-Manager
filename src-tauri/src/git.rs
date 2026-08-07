use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::{Port, Project, ProjectStatus, ServiceKind, StarterKit};
use crate::ports::PortAllocator;
use crate::sail::{ProcessOutput, OUTPUT_EVENT};
use crate::scaffolder;
use crate::store::ProjectStore;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneInput {
    pub url: String,
    pub name: Option<String>,
    pub branch: Option<String>,
    pub php_version: Option<String>,
}

pub async fn clone_and_register(
    app: &AppHandle,
    store: &ProjectStore,
    projects_root: &Path,
    input: CloneInput,
) -> AppResult<Project> {
    let url = input.url.trim().to_string();
    if !is_valid_git_url(&url) {
        return Err(AppError::Other(format!(
            "invalid git URL (expected https://… or git@host:owner/repo.git): {url}"
        )));
    }

    let raw_name = match input
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(n) => n.to_string(),
        None => derive_name_from_url(&url).ok_or_else(|| {
            AppError::Other(format!("could not derive a project name from URL: {url}"))
        })?,
    };
    let name = sanitize_name(&raw_name);
    if name.is_empty() {
        return Err(AppError::InvalidName(raw_name));
    }

    if store.name_exists(&name)? {
        return Err(AppError::NameTaken(name));
    }

    let project_path = projects_root.join(&name);
    if project_path.exists() {
        return Err(AppError::Other(format!(
            "destination already exists: {}",
            project_path.display()
        )));
    }

    tokio::fs::create_dir_all(projects_root).await?;

    // Use a temporary correlation id for streaming events while the project is
    // not yet in the DB. Reused as the final project id for continuity.
    let id = Uuid::new_v4().to_string();
    let php_version = input
        .php_version
        .clone()
        .unwrap_or_else(|| "8.3".to_string());

    emit_line(app, &id, "stdout", format!("Cloning {url} into {name}…"));

    let clone_result = run_git_clone(
        app,
        &id,
        projects_root,
        &url,
        &name,
        input.branch.as_deref(),
    )
    .await;
    if let Err(e) = clone_result {
        let _ = tokio::fs::remove_dir_all(&project_path).await;
        return Err(e);
    }

    if !project_path.exists() {
        return Err(AppError::Other(
            "git clone reported success but project folder is missing".into(),
        ));
    }

    // Wrap the post-clone work so we can clean up on any failure.
    let post = post_clone(
        app,
        store,
        projects_root,
        &project_path,
        &id,
        &name,
        &php_version,
    )
    .await;
    match post {
        Ok(project) => Ok(project),
        Err(e) => {
            let _ = tokio::fs::remove_dir_all(&project_path).await;
            Err(e)
        }
    }
}

async fn post_clone(
    app: &AppHandle,
    store: &ProjectStore,
    projects_root: &Path,
    project_path: &Path,
    id: &str,
    name: &str,
    php_version: &str,
) -> AppResult<Project> {
    // Validate Laravel project.
    let composer_path = project_path.join("composer.json");
    if !composer_path.exists() {
        return Err(AppError::Other(
            "this repository does not look like a Laravel project (no composer.json)".into(),
        ));
    }
    let composer_contents = tokio::fs::read_to_string(&composer_path)
        .await
        .map_err(|e| AppError::Other(format!("could not read composer.json: {e}")))?;
    if !composer_contents.contains("laravel/framework") {
        return Err(AppError::Other(
            "this repository does not look like a Laravel project (laravel/framework not in composer.json)".into(),
        ));
    }
    let has_sail = composer_contents.contains("laravel/sail");

    let services = vec![ServiceKind::Mysql, ServiceKind::Redis, ServiceKind::Mailpit];

    let bash_cmd = if has_sail {
        emit_line(
            app,
            id,
            "stdout",
            "laravel/sail already present — running composer install…".into(),
        );
        format!(
            "set -e && \
             cd {name} && \
             composer install --no-interaction && \
             php artisan key:generate --force"
        )
    } else {
        emit_line(
            app,
            id,
            "stdout",
            "laravel/sail not found — installing Sail and default services…".into(),
        );
        format!(
            "set -e && \
             cd {name} && \
             composer install --no-interaction && \
             composer require laravel/sail --dev --no-interaction && \
             php artisan sail:install --no-interaction --with=mysql,redis,mailpit && \
             php artisan key:generate --force"
        )
    };

    let (uid, gid) = current_uid_gid().await.unwrap_or((501, 20));
    let image = php_image_for(php_version);
    emit_line(app, id, "stdout", format!("Pulling image {image}…"));

    let mut cmd = Command::new("docker");
    cmd.args(["run", "--rm", "--pull=always", "-v"]);
    cmd.arg(format!("{}:/opt", projects_root.display()));
    cmd.args(["-w", "/opt"]);
    cmd.args(["-e", "WWWUSER", "-e", "WWWGROUP"]);
    cmd.env("WWWUSER", uid.to_string());
    cmd.env("WWWGROUP", gid.to_string());
    cmd.arg(&image);
    cmd.args(["bash", "-c", &bash_cmd]);

    run_streaming(app, id, &mut cmd).await?;

    // A cloned repo's compose file is untrusted. Now that install/sail:install
    // has produced (or the repo shipped) a compose file, audit it for
    // host-escape directives before the project can be Started.
    for candidate in [
        "compose.yaml",
        "compose.yml",
        "docker-compose.yml",
        "docker-compose.yaml",
    ] {
        let compose_path = project_path.join(candidate);
        if compose_path.exists() {
            let text = tokio::fs::read_to_string(&compose_path)
                .await
                .map_err(|e| AppError::Other(format!("could not read {candidate}: {e}")))?;
            let risks = crate::compose_audit::audit(&text);
            if !risks.is_empty() {
                return Err(AppError::Other(crate::compose_audit::describe(&risks)));
            }
            break;
        }
    }

    // Allocate ports for default services and customise .env.
    let ports: Vec<Port> = PortAllocator::allocate_for_services(store, &services)?;
    scaffolder::customize_env(project_path, name, &ports).await?;

    // Build and persist the project record.
    let project = Project {
        id: id.to_string(),
        name: name.to_string(),
        compose_project_name: name.to_string(),
        path: project_path.display().to_string(),
        status: ProjectStatus::Stopped,
        starter_kit: StarterKit::None,
        php_version: php_version.to_string(),
        services,
        ports,
        created_at: Utc::now(),
        last_started: None,
    };
    store.insert(&project)?;

    emit_line(app, id, "stdout", "Done.".into());
    store.get(id)
}

async fn run_git_clone(
    app: &AppHandle,
    project_id: &str,
    projects_root: &Path,
    url: &str,
    name: &str,
    branch: Option<&str>,
) -> AppResult<()> {
    let mut cmd = Command::new("git");
    cmd.current_dir(projects_root);
    cmd.arg("clone");
    cmd.arg("--progress");
    if let Some(b) = branch.map(str::trim).filter(|s| !s.is_empty()) {
        cmd.arg("--branch").arg(b);
    }
    cmd.arg(url).arg(name);
    run_streaming(app, project_id, &mut cmd).await
}

fn is_valid_git_url(url: &str) -> bool {
    if url.is_empty() {
        return false;
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        // Require something past the scheme.
        let rest = url.split_once("://").map(|(_, r)| r).unwrap_or("");
        return !rest.is_empty() && !rest.contains(' ');
    }
    // SSH form: git@host:user/repo(.git)?
    if let Some((user_host, path)) = url.split_once(':') {
        if user_host.contains('@')
            && !path.is_empty()
            && !path.contains(' ')
            && !path.starts_with('/')
        {
            return true;
        }
    }
    // Also allow the explicit ssh:// scheme.
    if url.starts_with("ssh://") {
        let rest = url.trim_start_matches("ssh://");
        return !rest.is_empty() && !rest.contains(' ');
    }
    false
}

fn derive_name_from_url(url: &str) -> Option<String> {
    // Take the last path segment, regardless of separator.
    let trimmed = url.trim().trim_end_matches('/');
    let last = trimmed.rsplit(['/', ':']).next().unwrap_or("");
    let last = last.strip_suffix(".git").unwrap_or(last);
    if last.is_empty() {
        None
    } else {
        Some(last.to_string())
    }
}

fn sanitize_name(input: &str) -> String {
    let lower = input.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut prev_dash = false;
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn php_image_for(php_version: &str) -> String {
    let compact = php_version.replace('.', "");
    format!("laravelsail/php{compact}-composer:latest")
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

// Mirrors scaffolder::run_streaming (which is private). Kept in sync by hand.
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
