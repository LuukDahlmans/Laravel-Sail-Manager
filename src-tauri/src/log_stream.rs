use std::collections::HashMap;
use std::process::Stdio;
use std::sync::OnceLock;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::models::Project;

pub const OUTPUT_EVENT: &str = "project-log";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub project_id: String,
    pub stream: String,
    pub line: String,
}

struct StreamHandle {
    child: Child,
}

fn streams() -> &'static Mutex<HashMap<String, StreamHandle>> {
    static STREAMS: OnceLock<Mutex<HashMap<String, StreamHandle>>> = OnceLock::new();
    STREAMS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub async fn start(app: &AppHandle, project: &Project, service: Option<String>) -> AppResult<()> {
    let mut map = streams().lock().await;
    if map.contains_key(&project.id) {
        return Ok(());
    }

    let mut cmd = Command::new("docker");
    cmd.args([
        "compose",
        "logs",
        "-f",
        "--no-color",
        "--timestamps",
        "--tail=200",
    ]);
    if let Some(svc) = service.as_ref() {
        let trimmed = svc.trim();
        if !trimmed.is_empty() {
            cmd.arg(trimmed);
        }
    }
    cmd.current_dir(&project.path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::Other(format!("failed to spawn docker compose logs: {e}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Other("missing stdout from docker compose logs".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Other("missing stderr from docker compose logs".into()))?;

    let app_out = app.clone();
    let pid_out = project.id.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app_out.emit(
                OUTPUT_EVENT,
                LogLine {
                    project_id: pid_out.clone(),
                    stream: "stdout".into(),
                    line,
                },
            );
        }
    });

    let app_err = app.clone();
    let pid_err = project.id.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app_err.emit(
                OUTPUT_EVENT,
                LogLine {
                    project_id: pid_err.clone(),
                    stream: "stderr".into(),
                    line,
                },
            );
        }
    });

    map.insert(project.id.clone(), StreamHandle { child });
    Ok(())
}

pub async fn stop(project_id: &str) -> AppResult<()> {
    let mut map = streams().lock().await;
    if let Some(mut handle) = map.remove(project_id) {
        // Best-effort kill — the child may already have exited.
        let _ = handle.child.kill().await;
    }
    Ok(())
}

#[allow(dead_code)]
pub async fn stop_all() {
    let mut map = streams().lock().await;
    let ids: Vec<String> = map.keys().cloned().collect();
    for id in ids {
        if let Some(mut handle) = map.remove(&id) {
            let _ = handle.child.kill().await;
        }
    }
}
