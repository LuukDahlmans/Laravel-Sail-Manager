use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex as TokioMutex;

use crate::error::{AppError, AppResult};
use crate::models::{Project, ProjectStatus};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessOutput {
    pub project_id: String,
    pub stream: String,
    pub line: String,
}

pub const OUTPUT_EVENT: &str = "process-output";

/// Run a command to completion but give up after `secs`. Every docker call
/// that's polled from the UI must use this: when Docker Desktop is paused or
/// the daemon wedges, a bare `.output().await` never returns and the invoke
/// hangs the frontend forever. A timeout surfaces as an error the UI can show
/// instead.
pub async fn output_with_timeout(cmd: &mut Command, secs: u64) -> AppResult<std::process::Output> {
    match tokio::time::timeout(std::time::Duration::from_secs(secs), cmd.output()).await {
        Ok(Ok(out)) => Ok(out),
        Ok(Err(e)) => Err(AppError::Other(format!("could not run docker: {e}"))),
        Err(_) => Err(AppError::DockerUnavailable(
            "docker command timed out — engine likely paused or unresponsive".into(),
        )),
    }
}

pub async fn check_docker() -> AppResult<()> {
    // `docker ps -q` requires a working daemon connection — `docker info`
    // alone can succeed in odd states (e.g. Docker Desktop paused). Wrap in
    // a timeout so a hung command surfaces as "not running" instead of
    // hanging the UI poller.
    let cmd = Command::new("docker").args(["ps", "-q"]).output();
    let result = tokio::time::timeout(std::time::Duration::from_secs(3), cmd).await;
    match result {
        Err(_) => Err(AppError::DockerUnavailable(
            "docker command timed out — engine likely paused or unresponsive".into(),
        )),
        Ok(Err(e)) => Err(AppError::DockerUnavailable(format!(
            "could not run docker: {e}"
        ))),
        Ok(Ok(out)) if out.status.success() => Ok(()),
        Ok(Ok(out)) => {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            Err(AppError::DockerUnavailable(if stderr.is_empty() {
                "docker daemon is not responding".into()
            } else {
                stderr
            }))
        }
    }
}

pub async fn start(app: &AppHandle, project: &Project) -> AppResult<()> {
    // Bring up containers and wait for healthchecks so the next exec works.
    run_streaming(
        app,
        project,
        Command::new("docker")
            .args(["compose", "up", "-d", "--wait"])
            .current_dir(&project.path),
    )
    .await?;

    // Run migrations idempotently. Fresh Laravel 11+ uses SESSION_DRIVER=database
    // and every request hits the sessions table, so without migrations a fresh
    // project returns 500 on every page until someone runs migrate manually.
    // Best-effort: log failures but don't fail the start (e.g., projects with no
    // database service or no migrations).
    let _ = run_streaming(
        app,
        project,
        Command::new("docker")
            .args([
                "compose",
                "exec",
                "-T",
                "laravel.test",
                "php",
                "artisan",
                "migrate",
                "--force",
            ])
            .current_dir(&project.path),
    )
    .await;

    Ok(())
}

pub const AUTO_OUTPUT_EVENT: &str = "auto-command-output";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoCommandOutput {
    pub project_id: String,
    pub command_id: String,
    pub label: String,
    pub stream: String,
    pub line: String,
}

/// Holds running service-mode auto-command processes per project so we can
/// kill them when the project stops.
static AUTO_HANDLES: OnceLock<TokioMutex<HashMap<String, Vec<Child>>>> = OnceLock::new();

fn auto_handles() -> &'static TokioMutex<HashMap<String, Vec<Child>>> {
    AUTO_HANDLES.get_or_init(|| TokioMutex::new(HashMap::new()))
}

/// Translate a user-friendly command (e.g. `sail artisan horizon`) into the
/// raw command we exec inside the laravel.test container.
///
/// - Strips a leading `sail ` or `./vendor/bin/sail ` prefix.
/// - Rewrites `artisan ...` → `php artisan ...` so users can write either form.
pub(crate) fn transform_sail_command(input: &str) -> String {
    let trimmed = input.trim();
    let stripped = trimmed
        .strip_prefix("./vendor/bin/sail ")
        .or_else(|| trimmed.strip_prefix("vendor/bin/sail "))
        .or_else(|| trimmed.strip_prefix("sail "))
        .unwrap_or(trimmed);

    if let Some(rest) = stripped.strip_prefix("artisan ") {
        return format!("php artisan {rest}");
    }
    if stripped == "artisan" {
        return "php artisan".to_string();
    }
    stripped.to_string()
}

/// Run a project's enabled auto-commands after start. `Once` commands run
/// blocking and sequentially with their output streamed live. `Service`
/// commands are spawned as tracked background processes so they keep running
/// in the foreground of the container, their output streams in real time, and
/// they can be killed when the project stops.
pub async fn run_auto_commands(
    app: &AppHandle,
    project: &Project,
    commands: &[crate::models::AutoCommand],
) {
    use crate::models::AutoCommandRunMode;

    // Dedup: if we're about to (re)spawn any service-mode command, first stop
    // whatever service processes are already running for this project.
    // Otherwise starting an already-running project again — or clicking "Run
    // auto-commands now" repeatedly — stacks N copies of `queue:work` /
    // `horizon` inside the container, so every job runs N times.
    let has_service = commands
        .iter()
        .any(|c| c.enabled && matches!(c.run_mode, AutoCommandRunMode::Service));
    if has_service {
        stop_auto_services(project).await;
    }

    for cmd in commands.iter().filter(|c| c.enabled) {
        let raw = cmd.command.trim();
        if raw.is_empty() {
            continue;
        }
        let actual = transform_sail_command(raw);

        match cmd.run_mode {
            AutoCommandRunMode::Once => {
                let _ = run_auto_once(app, project, &cmd.id, &cmd.label, &actual).await;
            }
            AutoCommandRunMode::Service => {
                let _ = spawn_auto_service(app, project, &cmd.id, &cmd.label, &actual).await;
            }
        }
    }
}

/// Stop and reap all background service-mode auto-commands for a project.
/// Called from commands::stop_project before `sail::stop` so we kill our
/// children before the container is brought down.
///
/// Killing the host-side `docker compose exec` client alone is NOT reliable:
/// for a non-TTY exec the daemon leaves the in-container process running as an
/// orphan. So we also sweep the PID files each service wrote (see
/// `spawn_auto_service`) and kill the real in-container processes. Best-effort
/// and time-boxed — a wedged container must not hang Stop.
pub async fn stop_auto_services(project: &Project) {
    // 1. Kill the host-side clients so output streaming stops.
    {
        let mut map = auto_handles().lock().await;
        if let Some(children) = map.remove(&project.id) {
            for mut child in children {
                let _ = child.kill().await;
            }
        }
    }

    // 2. Kill the actual in-container processes via their PID files.
    let sweep = format!(
        "for f in {dir}/sailmgr-{pid}-*.pid; do [ -e \"$f\" ] || continue; \
         kill \"$(cat \"$f\")\" 2>/dev/null; rm -f \"$f\"; done",
        dir = AUTO_PID_DIR,
        pid = project.id,
    );
    let fut = Command::new("docker")
        .args([
            "compose",
            "exec",
            "-T",
            "laravel.test",
            "bash",
            "-lc",
            &sweep,
        ])
        .current_dir(&project.path)
        .output();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), fut).await;
}

/// Where service commands drop their in-container PID file. `/tmp` inside the
/// container is fine — it dies with the container anyway.
const AUTO_PID_DIR: &str = "/tmp";

async fn run_auto_once(
    app: &AppHandle,
    project: &Project,
    command_id: &str,
    label: &str,
    cmd: &str,
) -> AppResult<()> {
    let mut child = Command::new("docker")
        .args(["compose", "exec", "-T", "laravel.test", "bash", "-lc", cmd])
        .current_dir(&project.path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Sail(format!("auto-cmd spawn failed: {e}")))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    spawn_auto_readers(
        app,
        project.id.clone(),
        command_id.to_string(),
        label.to_string(),
        stdout,
        stderr,
    );

    let status = child
        .wait()
        .await
        .map_err(|e| AppError::Sail(format!("auto-cmd wait failed: {e}")))?;
    if !status.success() {
        emit_auto_line(
            app,
            project.id.clone(),
            command_id.to_string(),
            label.to_string(),
            "stderr",
            format!("[exit {:?}]", status.code()),
        );
    }
    Ok(())
}

async fn spawn_auto_service(
    app: &AppHandle,
    project: &Project,
    command_id: &str,
    label: &str,
    cmd: &str,
) -> AppResult<()> {
    // Record the in-container PID before exec-ing into the real command, so
    // stop_auto_services can reliably kill it even though the host-side client
    // getting killed wouldn't. `exec` means `$$` (the shell PID that wrote the
    // file) becomes the command's PID. project.id/command_id are UUIDs, so the
    // path is shell-safe.
    let pid_file = format!("{AUTO_PID_DIR}/sailmgr-{}-{}.pid", project.id, command_id);
    let wrapped = format!("echo $$ > '{pid_file}'; exec {cmd}");
    let mut child = Command::new("docker")
        .args([
            "compose",
            "exec",
            "-T",
            "laravel.test",
            "bash",
            "-lc",
            &wrapped,
        ])
        .current_dir(&project.path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Sail(format!("auto-service spawn failed: {e}")))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    spawn_auto_readers(
        app,
        project.id.clone(),
        command_id.to_string(),
        label.to_string(),
        stdout,
        stderr,
    );

    let mut map = auto_handles().lock().await;
    map.entry(project.id.clone()).or_default().push(child);
    Ok(())
}

fn spawn_auto_readers(
    app: &AppHandle,
    project_id: String,
    command_id: String,
    label: String,
    stdout: Option<tokio::process::ChildStdout>,
    stderr: Option<tokio::process::ChildStderr>,
) {
    if let Some(s) = stdout {
        let app = app.clone();
        let pid = project_id.clone();
        let cid = command_id.clone();
        let lbl = label.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(s).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                emit_auto_line(&app, pid.clone(), cid.clone(), lbl.clone(), "stdout", line);
            }
        });
    }
    if let Some(s) = stderr {
        let app = app.clone();
        let pid = project_id.clone();
        let cid = command_id.clone();
        let lbl = label.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(s).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                emit_auto_line(&app, pid.clone(), cid.clone(), lbl.clone(), "stderr", line);
            }
        });
    }
}

fn emit_auto_line(
    app: &AppHandle,
    project_id: String,
    command_id: String,
    label: String,
    stream: &str,
    line: String,
) {
    let _ = app.emit(
        AUTO_OUTPUT_EVENT,
        AutoCommandOutput {
            project_id,
            command_id,
            label,
            stream: stream.into(),
            line,
        },
    );
}

pub async fn stop(app: &AppHandle, project: &Project) -> AppResult<()> {
    run_streaming(
        app,
        project,
        Command::new("docker")
            .args(["compose", "down"])
            .current_dir(&project.path),
    )
    .await
}

pub async fn current_status(project: &Project) -> ProjectStatus {
    // Polled from the UI (refresh_status), so it must be time-boxed: a paused
    // daemon would otherwise hang the status refresh indefinitely.
    let mut cmd = Command::new("docker");
    cmd.args(["compose", "ps", "-q"]).current_dir(&project.path);
    let output = output_with_timeout(&mut cmd, 5).await;

    match output {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            if s.lines().any(|l| !l.trim().is_empty()) {
                ProjectStatus::Running
            } else {
                ProjectStatus::Stopped
            }
        }
        _ => ProjectStatus::Stopped,
    }
}

async fn run_streaming(app: &AppHandle, project: &Project, cmd: &mut Command) -> AppResult<()> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::Sail(format!("spawn failed: {e}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Sail("missing stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Sail("missing stderr".into()))?;

    let buf = Arc::new(Mutex::new(Vec::<String>::new()));

    let app_out = app.clone();
    let pid_out = project.id.clone();
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
    let pid_err = project.id.clone();
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
        .map_err(|e| AppError::Sail(format!("wait failed: {e}")))?;
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
        return Err(AppError::Sail(format!(
            "exit {:?}\n---\n{}",
            status.code(),
            preview
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_dot_slash_vendor_bin_sail_prefix() {
        assert_eq!(
            transform_sail_command("./vendor/bin/sail npm run dev"),
            "npm run dev"
        );
    }

    #[test]
    fn strips_vendor_bin_sail_prefix() {
        assert_eq!(
            transform_sail_command("vendor/bin/sail npm run dev"),
            "npm run dev"
        );
    }

    #[test]
    fn strips_bare_sail_prefix() {
        assert_eq!(transform_sail_command("sail npm run dev"), "npm run dev");
    }

    #[test]
    fn rewrites_sail_artisan_to_php_artisan() {
        assert_eq!(
            transform_sail_command("sail artisan horizon"),
            "php artisan horizon"
        );
    }

    #[test]
    fn rewrites_vendor_bin_sail_artisan() {
        assert_eq!(
            transform_sail_command("./vendor/bin/sail artisan queue:work"),
            "php artisan queue:work"
        );
    }

    #[test]
    fn rewrites_bare_artisan_with_args() {
        assert_eq!(
            transform_sail_command("artisan migrate --force"),
            "php artisan migrate --force"
        );
    }

    #[test]
    fn rewrites_lonely_artisan() {
        assert_eq!(transform_sail_command("artisan"), "php artisan");
    }

    #[test]
    fn lonely_sail_yields_empty_string() {
        // After stripping "sail " (the only prefix that matches), the input
        // "sail" itself does not have a trailing space — so the prefix doesn't
        // strip. We get the literal "sail" back. This documents the current
        // behaviour: a bare "sail" with no further args is an unhandled edge
        // case that returns the literal token.
        assert_eq!(transform_sail_command("sail"), "sail");
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(transform_sail_command(""), "");
    }

    #[test]
    fn whitespace_only_input_trims_to_empty() {
        assert_eq!(transform_sail_command("   "), "");
    }

    #[test]
    fn trims_outer_whitespace_before_processing() {
        assert_eq!(
            transform_sail_command("   sail artisan tinker   "),
            "php artisan tinker"
        );
    }

    #[test]
    fn preserves_inner_whitespace() {
        assert_eq!(
            transform_sail_command("sail npm  run   dev"),
            "npm  run   dev"
        );
    }

    #[test]
    fn passes_through_arbitrary_command_unchanged() {
        assert_eq!(
            transform_sail_command("php artisan tinker"),
            "php artisan tinker"
        );
    }

    #[test]
    fn does_not_strip_sail_inside_word() {
        // Only a leading "sail " (with a trailing space) is stripped, not a
        // word starting with "sail".
        assert_eq!(transform_sail_command("sailboat ahoy"), "sailboat ahoy");
    }

    #[test]
    fn artisan_only_rewrite_after_prefix_strip() {
        // Confirm the rewrite path doesn't double-prepend `php` when the
        // command was already "php artisan ...".
        assert_eq!(
            transform_sail_command("php artisan migrate"),
            "php artisan migrate"
        );
    }
}
