use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::OnceLock;

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex as TokioMutex;

use crate::error::{AppError, AppResult};

pub const OUTPUT_EVENT: &str = "shell-output";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellOutput {
    pub project_id: String,
    pub data: String,
}

/// Per-project PTY session. The reader runs on a blocking thread; the writer
/// and master are kept here so input + resize commands can act on them.
struct ShellSession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

fn sessions() -> &'static TokioMutex<HashMap<String, ShellSession>> {
    static SESSIONS: OnceLock<TokioMutex<HashMap<String, ShellSession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| TokioMutex::new(HashMap::new()))
}

pub async fn start(
    app: &AppHandle,
    project_id: &str,
    project_path: &Path,
    cols: u16,
    rows: u16,
) -> AppResult<()> {
    {
        // If a session for this project is already running, don't double-spawn.
        // The frontend toggles `active` on tab show/hide and is responsible for
        // calling stop on tear-down — but a stale entry shouldn't break things.
        let map = sessions().lock().await;
        if map.contains_key(project_id) {
            return Ok(());
        }
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| AppError::Other(format!("openpty failed: {e}")))?;

    // docker compose exec v2 allocates a TTY by default. The legacy `-it`
    // flags from `docker exec` aren't valid here and cause the shell inside
    // the container to land in a weird half-interactive state — manifests as
    // a runaway "ggggg…" echo when xterm's capability-query escape sequences
    // get echoed back as input.
    //
    // We also explicitly forward `TERM` and `LANG` so xterm-compatible escape
    // sequences (bracketed paste, 256-color, etc.) are interpreted by the
    // shell rather than echoed back as raw bytes.
    let mut cmd = CommandBuilder::new("docker");
    cmd.args([
        "compose",
        "exec",
        "-e",
        "TERM=xterm-256color",
        "-e",
        "LANG=C.UTF-8",
        "laravel.test",
        "bash",
    ]);
    cmd.cwd(project_path);
    // Set TERM on the host-side docker process too — portable-pty inherits a
    // sparse env by default. Some containers honor the inherited TERM if no
    // explicit -e is set.
    cmd.env("TERM", "xterm-256color");

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| AppError::Other(format!("failed to spawn docker compose exec: {e}")))?;

    // Wezterm convention: drop the slave handle after spawn so the master sees
    // EOF when the child exits. Without this, the reader loop hangs forever on
    // child death.
    drop(pair.slave);

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| AppError::Other(format!("clone pty reader failed: {e}")))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| AppError::Other(format!("take pty writer failed: {e}")))?;

    let app_out = app.clone();
    let pid_out = project_id.to_string();
    // portable-pty's reader is sync std::io::Read — we MUST use spawn_blocking
    // here. tokio::io adapters won't work and would deadlock the runtime.
    tokio::task::spawn_blocking(move || {
        let mut reader = reader;
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let _ = app_out.emit(
                        OUTPUT_EVENT,
                        ShellOutput {
                            project_id: pid_out.clone(),
                            data: chunk,
                        },
                    );
                }
                Err(_) => break,
            }
        }
    });

    let mut map = sessions().lock().await;
    map.insert(
        project_id.to_string(),
        ShellSession {
            master: pair.master,
            writer,
            child,
        },
    );

    Ok(())
}

pub async fn write_input(project_id: &str, data: &str) -> AppResult<()> {
    let mut map = sessions().lock().await;
    let session = map
        .get_mut(project_id)
        .ok_or_else(|| AppError::Other("shell session not found".into()))?;
    session
        .writer
        .write_all(data.as_bytes())
        .map_err(|e| AppError::Other(format!("shell write failed: {e}")))?;
    session
        .writer
        .flush()
        .map_err(|e| AppError::Other(format!("shell flush failed: {e}")))?;
    Ok(())
}

pub async fn resize(project_id: &str, cols: u16, rows: u16) -> AppResult<()> {
    let map = sessions().lock().await;
    let session = map
        .get(project_id)
        .ok_or_else(|| AppError::Other("shell session not found".into()))?;
    session
        .master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| AppError::Other(format!("pty resize failed: {e}")))?;
    Ok(())
}

pub async fn stop(project_id: &str) -> AppResult<()> {
    let mut map = sessions().lock().await;
    if let Some(mut session) = map.remove(project_id) {
        let _ = session.child.kill();
        // Dropping `master` and `writer` after this scope closes the PTY; the
        // reader thread will get a 0-byte read or error and exit cleanly.
    }
    Ok(())
}

#[allow(dead_code)]
pub async fn stop_all() {
    let mut map = sessions().lock().await;
    let ids: Vec<String> = map.keys().cloned().collect();
    for id in ids {
        if let Some(mut session) = map.remove(&id) {
            let _ = session.child.kill();
        }
    }
}
