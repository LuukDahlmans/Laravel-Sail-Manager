use std::collections::HashMap;
use std::process::Stdio;
use std::sync::OnceLock;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex as TokioMutex;

use crate::error::{AppError, AppResult};
use crate::models::Project;
use crate::sail::transform_sail_command;

pub const OUTPUT_EVENT: &str = "one-shot-output";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OneShotLine {
    pub project_id: String,
    pub stream: String,
    pub line: String,
}

/// Tracks a running one-shot via just its OS PID, so `stop()` can kill it via
/// SIGTERM without contending with the waiter task that owns the `Child`.
struct Running {
    pid: u32,
}

fn handles() -> &'static TokioMutex<HashMap<String, Running>> {
    static HANDLES: OnceLock<TokioMutex<HashMap<String, Running>>> = OnceLock::new();
    HANDLES.get_or_init(|| TokioMutex::new(HashMap::new()))
}

/// Spawn a one-shot command inside the project's `laravel.test` container.
/// Streams stdout/stderr line-by-line on `OUTPUT_EVENT`. Emits a final
/// synthetic line `[exit <code>]` so the frontend can show completion.
/// Replaces any prior running command for this project.
pub async fn run(app: &AppHandle, project: &Project, command: &str) -> AppResult<()> {
    let raw = command.trim();
    if raw.is_empty() {
        return Err(AppError::Other("command is empty".into()));
    }
    let actual = transform_sail_command(raw);

    // Replace any in-flight one-shot for this project before starting a new one.
    if let Some(prev) = handles().lock().await.remove(&project.id) {
        kill_pid(prev.pid).await;
    }

    let mut child = Command::new("docker")
        .args([
            "compose",
            "exec",
            "-T",
            "laravel.test",
            "bash",
            "-lc",
            &actual,
        ])
        .current_dir(&project.path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Other(format!("one-shot spawn failed: {e}")))?;

    let pid = child
        .id()
        .ok_or_else(|| AppError::Other("one-shot child has no pid".into()))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Other("missing stdout from one-shot".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Other("missing stderr from one-shot".into()))?;

    emit_line(app, &project.id, "stdout", format!("$ {actual}"));

    let app_out = app.clone();
    let pid_out = project.id.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            emit_line(&app_out, &pid_out, "stdout", line);
        }
    });

    let app_err = app.clone();
    let pid_err = project.id.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            emit_line(&app_err, &pid_err, "stderr", line);
        }
    });

    {
        let mut map = handles().lock().await;
        map.insert(project.id.clone(), Running { pid });
    }

    // Drive completion in the background so the command returns immediately.
    let app_done = app.clone();
    let pid_done = project.id.clone();
    tokio::spawn(async move {
        let status = child.wait().await;
        let line = match status {
            Ok(s) if s.success() => "[exit 0]".to_string(),
            Ok(s) => format!(
                "[exit {}]",
                s.code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "?".into())
            ),
            Err(e) => format!("[wait failed: {e}]"),
        };
        emit_line(&app_done, &pid_done, "stdout", line);
        // Only remove the entry if it's still ours (a fresh run() may have
        // already replaced it with a new pid).
        let mut map = handles().lock().await;
        if let Some(existing) = map.get(&pid_done) {
            if existing.pid == pid {
                map.remove(&pid_done);
            }
        }
    });

    Ok(())
}

pub async fn stop(project_id: &str) -> AppResult<()> {
    let entry = handles().lock().await.remove(project_id);
    if let Some(running) = entry {
        kill_pid(running.pid).await;
    }
    Ok(())
}

/// Send SIGTERM via the `kill` binary — avoids pulling in a libc dep just for
/// one syscall and matches the macOS-only target. The wait task reaps the
/// child and emits `[exit ...]`.
async fn kill_pid(pid: u32) {
    let _ = Command::new("kill").arg(pid.to_string()).output().await;
}

fn emit_line(app: &AppHandle, project_id: &str, stream: &str, line: String) {
    let _ = app.emit(
        OUTPUT_EVENT,
        OneShotLine {
            project_id: project_id.to_string(),
            stream: stream.into(),
            line,
        },
    );
}
