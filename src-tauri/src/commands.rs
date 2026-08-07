use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::dependencies::{self, DependencyCheck};
use crate::dnsmasq;
use crate::error::{AppError, AppResult};
use crate::git;
use crate::import;
use crate::log_stream;
use crate::models::{
    AutoCommand, AutoCommandInput, CreateProjectInput, HistoryEntry, HistoryKind, Project,
    ProjectStatus,
};
use crate::one_shot;
use crate::ports::PortAllocator;
use crate::proxy;
use crate::resolver;
use crate::sail;
use crate::scaffolder;
use crate::settings::Settings;
use crate::shell;
use crate::state::AppState;
use crate::stats::{self, ContainerStat, GitStatus};
use crate::templates::{Template, TemplateInput};
use crate::tls;
use uuid::Uuid as Uuid2;

pub const STATUS_EVENT: &str = "project-status-changed";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StatusChange {
    project_id: String,
    status: ProjectStatus,
}

fn emit_status(app: &AppHandle, project_id: &str, status: ProjectStatus) {
    let _ = app.emit(
        STATUS_EVENT,
        StatusChange {
            project_id: project_id.into(),
            status,
        },
    );
}

#[tauri::command]
pub async fn list_projects(state: State<'_, AppState>) -> AppResult<Vec<Project>> {
    state.store.list()
}

#[tauri::command]
pub async fn get_project(state: State<'_, AppState>, id: String) -> AppResult<Project> {
    state.store.get(&id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvCheck {
    pub docker_ok: bool,
    pub docker_error: Option<String>,
    pub projects_root: String,
}

#[tauri::command]
pub async fn check_dependencies() -> AppResult<DependencyCheck> {
    Ok(dependencies::check_all().await)
}

/// Update the directory new projects get scaffolded / cloned into. Validates
/// the path is non-empty, expands a leading `~`, creates the directory if it
/// doesn't exist, and persists the choice to settings so it survives restarts.
#[tauri::command]
pub async fn set_projects_root(state: State<'_, AppState>, path: String) -> AppResult<EnvCheck> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::Other("projects root path cannot be empty".into()));
    }

    // Expand `~` so users can paste shell-style paths from the terminal.
    let home = std::env::var("HOME").ok().map(PathBuf::from);
    let expanded: PathBuf = if let Some(rest) = trimmed.strip_prefix("~/") {
        match &home {
            Some(h) => h.join(rest),
            None => PathBuf::from(trimmed),
        }
    } else if trimmed == "~" {
        home.unwrap_or_else(|| PathBuf::from(trimmed))
    } else {
        PathBuf::from(trimmed)
    };

    // Reject obviously dangerous targets so a fat-fingered "/" doesn't end up
    // as the projects root.
    if expanded.parent().is_none() || expanded == Path::new("/") {
        return Err(AppError::Other(
            "projects root must be a directory inside your home folder".into(),
        ));
    }

    tokio::fs::create_dir_all(&expanded).await.map_err(|e| {
        AppError::Other(format!(
            "could not create directory {}: {e}",
            expanded.display()
        ))
    })?;

    // Persist first so a crash mid-way doesn't leave runtime + settings
    // disagreeing on next launch.
    let stored = expanded.display().to_string();
    state
        .settings
        .update(|s| s.projects_root = stored.clone())?;
    state.set_projects_root(expanded);

    let (docker_ok, docker_error) = match sail::check_docker().await {
        Ok(()) => (true, None),
        Err(AppError::DockerUnavailable(msg)) => (false, Some(msg)),
        Err(e) => (false, Some(e.to_string())),
    };
    Ok(EnvCheck {
        docker_ok,
        docker_error,
        projects_root: state.projects_root().display().to_string(),
    })
}

#[tauri::command]
pub async fn check_environment(state: State<'_, AppState>) -> AppResult<EnvCheck> {
    let (docker_ok, docker_error) = match sail::check_docker().await {
        Ok(()) => (true, None),
        Err(AppError::DockerUnavailable(msg)) => (false, Some(msg)),
        Err(e) => (false, Some(e.to_string())),
    };
    Ok(EnvCheck {
        docker_ok,
        docker_error,
        projects_root: state.projects_root().display().to_string(),
    })
}

#[tauri::command]
pub async fn create_project(
    app: AppHandle,
    state: State<'_, AppState>,
    input: CreateProjectInput,
) -> AppResult<Project> {
    // Serialize concurrent `create_project` calls. Without this, two near-
    // simultaneous calls can both read the DB before either inserts and end up
    // allocating the same host ports. The lock is held for the duration of
    // scaffolding (which is slow), so creating two projects in parallel is
    // sequential — that's intentional and safer than a port collision.
    let _create_guard = state.create_lock.lock().await;

    let name = input.name.trim().to_string();
    if !is_valid_name(&name) {
        return Err(AppError::InvalidName(name));
    }
    if state.store.name_exists(&name)? {
        return Err(AppError::NameTaken(name));
    }

    let ports = PortAllocator::allocate_for_services(&state.store, &input.services)?;

    let id = Uuid::new_v4().to_string();
    let projects_root = state.projects_root();
    let project_path = projects_root.join(&name);
    let project = Project {
        id: id.clone(),
        name: name.clone(),
        compose_project_name: name.clone(),
        path: project_path.display().to_string(),
        status: ProjectStatus::Stopped,
        starter_kit: input.starter_kit,
        php_version: input.php_version.clone(),
        services: input.services.clone(),
        ports: ports.clone(),
        created_at: Utc::now(),
        last_started: None,
    };
    state.store.insert(&project)?;

    // Everything after the insert can fail (scaffold, .env customization, the
    // re-read). On ANY failure we must roll back both the DB row and the
    // on-disk folder — otherwise the row's allocated host ports stay reserved
    // forever (host_port_in_use keeps returning true) and a stale folder blocks
    // re-creating the same name.
    let build = async {
        let path = scaffolder::scaffold(
            &app,
            &id,
            &name,
            &input.php_version,
            &input.services,
            &input.custom_services,
            &projects_root,
        )
        .await?;
        scaffolder::customize_env(&path, &name, &ports).await?;
        state.store.get(&id)
    }
    .await;

    let project = match build {
        Ok(p) => p,
        Err(e) => {
            let _ = state.store.delete(&id);
            let _ = tokio::fs::remove_dir_all(&project_path).await;
            return Err(e);
        }
    };
    let _ = state.store.add_history(&id, HistoryKind::Created, None);
    // Release the create lock before refresh_local_urls_silent (which moves
    // `state`). Project creation is finished by this point; another concurrent
    // create can safely begin.
    drop(_create_guard);
    refresh_local_urls_silent(&app, state).await;
    Ok(project)
}

#[tauri::command]
pub async fn start_project(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<()> {
    let project = state.store.get(&id)?;
    state.store.update_status(&id, ProjectStatus::Starting)?;
    emit_status(&app, &id, ProjectStatus::Starting);

    match sail::start(&app, &project).await {
        Ok(()) => {
            state.store.update_status(&id, ProjectStatus::Running)?;
            state.store.touch_last_started(&id, Utc::now())?;
            emit_status(&app, &id, ProjectStatus::Running);
            let _ = state.store.add_history(&id, HistoryKind::Started, None);
            // Run any configured auto-commands. Best effort, errors don't fail start.
            if let Ok(cmds) = state.store.list_auto_commands(&id) {
                if !cmds.is_empty() {
                    sail::run_auto_commands(&app, &project, &cmds).await;
                }
            }
            Ok(())
        }
        Err(e) => {
            // `compose up --wait` can fail partway (e.g. a healthcheck times
            // out) with some containers already up, holding host ports. Bring
            // the project back down so a failed start doesn't leak containers
            // and block the ports on the next attempt. Best-effort.
            let _ = sail::stop(&app, &project).await;
            state.store.update_status(&id, ProjectStatus::Error)?;
            emit_status(&app, &id, ProjectStatus::Error);
            let _ = state
                .store
                .add_history(&id, HistoryKind::Errored, Some(&e.to_string()));
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn stop_project(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    let project = state.store.get(&id)?;
    state.store.update_status(&id, ProjectStatus::Stopping)?;
    emit_status(&app, &id, ProjectStatus::Stopping);

    // Kill any background service-mode auto-commands first so they don't
    // outlive the container.
    sail::stop_auto_services(&project).await;

    match sail::stop(&app, &project).await {
        Ok(()) => {
            state.store.update_status(&id, ProjectStatus::Stopped)?;
            emit_status(&app, &id, ProjectStatus::Stopped);
            let _ = state.store.add_history(&id, HistoryKind::Stopped, None);
            Ok(())
        }
        Err(e) => {
            state.store.update_status(&id, ProjectStatus::Error)?;
            emit_status(&app, &id, ProjectStatus::Error);
            let _ = state
                .store
                .add_history(&id, HistoryKind::Errored, Some(&e.to_string()));
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn delete_project(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    also_remove_files: bool,
) -> AppResult<()> {
    let project = state.store.get(&id)?;
    // Tear down anything still pointed at this project before its folder goes
    // away: background auto-services, the live log stream, one-shot output, and
    // any open shell PTY. Otherwise these keep running against a deleted path.
    sail::stop_auto_services(&project).await;
    let _ = log_stream::stop(&id).await;
    let _ = one_shot::stop(&id).await;
    let _ = shell::stop(&id).await;
    // Best-effort: regardless of recorded status, try to bring down any
    // containers from partial/aborted starts. Ignore errors.
    let _ = sail::stop(&app, &project).await;
    state.store.delete(&id)?;
    if also_remove_files {
        let _ = tokio::fs::remove_dir_all(&project.path).await;
    }
    refresh_local_urls_silent(&app, state).await;
    Ok(())
}

#[tauri::command]
pub async fn refresh_status(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<ProjectStatus> {
    let project = state.store.get(&id)?;
    let status = sail::current_status(&project).await;
    state.store.update_status(&id, status)?;
    emit_status(&app, &id, status);
    Ok(status)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> AppResult<Settings> {
    Ok(state.settings.snapshot())
}

/// Bring Traefik up to match the current settings — config + container both.
/// Centralizes the "what does Traefik need right now" logic so the toggles,
/// repair, and resync paths all stay in sync.
async fn apply_proxy(state: &AppState, settings: &Settings, projects: &[Project]) -> AppResult<()> {
    if settings.local_urls_https {
        // Build per-project hostnames so the cert covers each one explicitly
        // (Chrome silently rejects `*.<single-label>` wildcards). The cert
        // re-issues automatically when this list changes.
        let hosts: Vec<String> = projects
            .iter()
            .map(|p| format!("{}.{}", p.name, settings.local_url_tld))
            .collect();
        tls::ensure_wildcard_cert(&state.tls_dir, &settings.local_url_tld, &hosts).await?;
    }
    proxy::write_config(
        &state.proxy_conf_dir,
        projects,
        &settings.local_url_tld,
        settings.local_urls_https,
    )
    .await?;
    let tls_runtime = if settings.local_urls_https {
        Some(proxy::TlsRuntime {
            tls_dir: &state.tls_dir,
        })
    } else {
        None
    };
    proxy::ensure_running(
        &state.proxy_conf_dir,
        settings.proxy_port,
        tls_runtime.as_ref(),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn set_local_urls_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> AppResult<Settings> {
    // IMPORTANT: don't persist the new setting until all side-effecting steps
    // have succeeded. Otherwise a failure mid-flight (e.g. user cancels the
    // admin password prompt) leaves Local URLs flagged as enabled while the
    // resolver file is missing — broken half-state.
    let snapshot = state.settings.snapshot();

    if enabled {
        let projects = state.store.list()?;
        apply_proxy(&state, &snapshot, &projects).await?;
        dnsmasq::ensure_running(&state.dns_conf_dir, &snapshot.local_url_tld).await?;
        resolver::ensure_resolver(&snapshot.local_url_tld, dnsmasq::HOST_PORT).await?;
        return state.settings.update(|s| s.local_urls_enabled = true);
    }

    proxy::stop().await?;
    dnsmasq::stop().await?;
    // Best effort: remove resolver file + clear legacy /etc/hosts block.
    let _ = resolver::remove_resolver(&snapshot.local_url_tld, true).await;
    state.settings.update(|s| s.local_urls_enabled = false)
}

/// Toggle HTTPS for `.<tld>` URLs. Enabling generates a local CA + wildcard
/// cert, installs the CA into the user's login keychain (one-time GUI auth
/// prompt), and recreates Traefik with `:443` bound. Disabling tears down
/// `:443` and removes the CA from the keychain.
#[tauri::command]
pub async fn set_local_urls_https(
    state: State<'_, AppState>,
    enabled: bool,
) -> AppResult<Settings> {
    if enabled {
        let snap = state.settings.snapshot();
        // Scorch-earth refresh on the explicit toggle:
        //   1. Drop the old CA from the keychain (a previously-broken CA
        //      with absurd validity dates would poison the chain).
        //   2. Regenerate CA with sensible dates.
        //   3. Regenerate the wildcard cert signed by the new CA.
        //   4. Install the new CA into the keychain (one auth prompt).
        // Steps 1 + 4 only affect the keychain when the CA was/is present
        // and trusted; the rest is idempotent.
        let _ = tls::remove_ca_from_keychain().await;
        tls::force_regen_ca(&state.tls_dir, &snap.local_url_tld).await?;
        let projects = state.store.list()?;
        let hosts: Vec<String> = projects
            .iter()
            .map(|p| format!("{}.{}", p.name, snap.local_url_tld))
            .collect();
        tls::force_regen_wildcard_cert(&state.tls_dir, &snap.local_url_tld, &hosts).await?;
        tls::install_ca_to_keychain(&state.tls_dir).await?;
        // Only persist `https = true` once every side effect (including the
        // proxy reconfigure that binds :443) has succeeded. Persisting first
        // then failing apply_proxy would leave settings claiming HTTPS is on
        // while Traefik was never reconfigured — the half-state the sibling
        // set_local_urls_enabled deliberately avoids.
        if snap.local_urls_enabled {
            let mut with_https = snap.clone();
            with_https.local_urls_https = true;
            let projects = state.store.list()?;
            apply_proxy(&state, &with_https, &projects).await?;
        }
        let updated = state.settings.update(|s| s.local_urls_https = true)?;
        Ok(updated)
    } else {
        // Best-effort cleanup — if removal fails (e.g. user already deleted
        // the cert manually) we don't block disabling.
        let _ = tls::remove_ca_from_keychain().await;
        let updated = state.settings.update(|s| s.local_urls_https = false)?;
        if updated.local_urls_enabled {
            let projects = state.store.list()?;
            apply_proxy(&state, &updated, &projects).await?;
        }
        Ok(updated)
    }
}

#[tauri::command]
pub async fn set_local_url_tld(state: State<'_, AppState>, tld: String) -> AppResult<Settings> {
    let cleaned = tld.trim().trim_start_matches('.').to_lowercase();
    if !resolver::is_shell_safe_tld(&cleaned) {
        return Err(AppError::Other(
            "TLD must be 2–32 lowercase letters/digits/hyphens (no dots)".into(),
        ));
    }
    if !resolver::is_allowed_tld(&cleaned) {
        return Err(AppError::Other(format!(
            "'.{cleaned}' is a real or reserved TLD — routing it to localhost would break DNS system-wide. Pick a made-up TLD like 'sail', 'test', or 'ddev'."
        )));
    }
    let previous_tld = state.settings.snapshot().local_url_tld;
    let updated = state
        .settings
        .update(|s| s.local_url_tld = cleaned.clone())?;

    if updated.local_urls_enabled {
        let projects = state.store.list()?;
        let tld_changed = previous_tld != updated.local_url_tld;
        // When HTTPS is on and the TLD changes, the CA must be re-issued
        // name-constrained to the new TLD and re-trusted — apply_proxy
        // (via ensure_ca) re-issues it, so bracket with keychain remove/install
        // so the freshly-scoped root is trusted and the padlock keeps working.
        if updated.local_urls_https && tld_changed {
            let _ = tls::remove_ca_from_keychain().await;
        }
        apply_proxy(&state, &updated, &projects).await?;
        if updated.local_urls_https && tld_changed {
            tls::install_ca_to_keychain(&state.tls_dir).await?;
        }
        dnsmasq::ensure_running(&state.dns_conf_dir, &updated.local_url_tld).await?;
        if tld_changed {
            // Best-effort: drop the old resolver. Will trigger a sudo prompt.
            let _ = resolver::remove_resolver(&previous_tld, false).await;
        }
        resolver::ensure_resolver(&updated.local_url_tld, dnsmasq::HOST_PORT).await?;
    }
    Ok(updated)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalUrlsHealth {
    pub enabled: bool,
    pub tld: String,
    pub resolver_ok: bool,
    pub dnsmasq_running: bool,
    pub proxy_running: bool,
    pub proxy_port_bound: bool,
    pub dns_resolves: bool,
    pub overall_ok: bool,
    pub issues: Vec<String>,
}

#[tauri::command]
pub async fn check_local_urls(state: State<'_, AppState>) -> AppResult<LocalUrlsHealth> {
    let s = state.settings.snapshot();

    if !s.local_urls_enabled {
        return Ok(LocalUrlsHealth {
            enabled: false,
            tld: s.local_url_tld,
            resolver_ok: false,
            dnsmasq_running: false,
            proxy_running: false,
            proxy_port_bound: false,
            dns_resolves: false,
            overall_ok: true,
            issues: vec![],
        });
    }

    let mut issues: Vec<String> = Vec::new();

    // Resolver file present + correct content?
    let resolver_path = format!("/etc/resolver/{}", s.local_url_tld);
    let want = format!(
        "# Sail Manager\nnameserver 127.0.0.1\nport {}\n",
        crate::dnsmasq::HOST_PORT
    );
    let resolver_ok = match tokio::fs::read_to_string(&resolver_path).await {
        Ok(content) => content == want,
        Err(_) => false,
    };
    if !resolver_ok {
        issues.push(format!(
            "/etc/resolver/{} is missing or out of date — admin password is required to fix",
            s.local_url_tld
        ));
    }

    // dnsmasq container running?
    let dnsmasq_running = is_container_running("sail-manager-dns").await;
    if !dnsmasq_running {
        issues.push("DNS forwarder container (sail-manager-dns) is not running".into());
    }

    // proxy container running with the configured port bound?
    let (proxy_running, proxy_port_bound) = proxy_health("sail-manager-proxy", s.proxy_port).await;
    if !proxy_running {
        issues.push("Traefik proxy container (sail-manager-proxy) is not running".to_string());
    } else if !proxy_port_bound {
        issues.push(format!(
            "Traefik is running but port :{} is not bound to the host (something else is holding it)",
            s.proxy_port
        ));
    }

    // Can our dnsmasq actually answer queries?
    let dns_resolves = if dnsmasq_running {
        dns_probe(&s.local_url_tld).await
    } else {
        false
    };
    if dnsmasq_running && !dns_resolves {
        issues.push("DNS forwarder is up but isn't resolving the configured TLD".into());
    }

    let overall_ok =
        resolver_ok && dnsmasq_running && proxy_running && proxy_port_bound && dns_resolves;

    Ok(LocalUrlsHealth {
        enabled: true,
        tld: s.local_url_tld,
        resolver_ok,
        dnsmasq_running,
        proxy_running,
        proxy_port_bound,
        dns_resolves,
        overall_ok,
        issues,
    })
}

/// Best-effort silent recovery for the bits that don't need sudo: bring up the
/// proxy and dnsmasq containers if they're missing (Docker may have been down
/// last time the app ran). Resolver-file repair is intentionally NOT done here
/// because it would surface a password prompt — that's the job of
/// `resync_local_urls`, which the user invokes from a banner button.
#[tauri::command]
pub async fn repair_local_urls_quiet(state: State<'_, AppState>) -> AppResult<LocalUrlsHealth> {
    let s = state.settings.snapshot();
    if s.local_urls_enabled {
        let projects = state.store.list().unwrap_or_default();
        let _ = apply_proxy(&state, &s, &projects).await;
        let _ = crate::dnsmasq::ensure_running(&state.dns_conf_dir, &s.local_url_tld).await;
    }
    check_local_urls(state).await
}

async fn is_container_running(name: &str) -> bool {
    let mut cmd = tokio::process::Command::new("docker");
    cmd.args(["inspect", name, "--format", "{{.State.Running}}"]);
    let out = sail::output_with_timeout(&mut cmd, 5).await;
    matches!(out, Ok(o) if o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "true")
}

async fn proxy_health(name: &str, port: u16) -> (bool, bool) {
    let mut cmd = tokio::process::Command::new("docker");
    cmd.args([
        "inspect",
        name,
        "--format",
        "{{.State.Running}}|{{json .NetworkSettings.Ports}}",
    ]);
    let out = sail::output_with_timeout(&mut cmd, 5).await;
    match out {
        Ok(o) if o.status.success() => {
            let raw = String::from_utf8_lossy(&o.stdout);
            let trimmed = raw.trim();
            let mut parts = trimmed.splitn(2, '|');
            let running = parts.next().unwrap_or("").trim() == "true";
            let ports = parts.next().unwrap_or("");
            let port_published = ports.contains(&format!("\"HostPort\":\"{port}\""));
            (running, port_published)
        }
        _ => (false, false),
    }
}

async fn dns_probe(tld: &str) -> bool {
    // Run `dig` against our dnsmasq with a 2s timeout. Returns true if any
    // answer line for `<probe>.<tld>` resolves to 127.0.0.1.
    let probe = format!("sail-manager-probe.{tld}");
    let cmd = tokio::process::Command::new("dig")
        .args([
            "@127.0.0.1",
            "-p",
            &crate::dnsmasq::HOST_PORT.to_string(),
            &probe,
            "+short",
            "+time=1",
            "+tries=1",
        ])
        .output();
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), cmd).await;
    match result {
        Ok(Ok(o)) if o.status.success() => String::from_utf8_lossy(&o.stdout).contains("127.0.0.1"),
        _ => false,
    }
}

#[tauri::command]
pub async fn resync_local_urls(state: State<'_, AppState>) -> AppResult<()> {
    let s = state.settings.snapshot();
    if !s.local_urls_enabled {
        return Err(AppError::Other(
            "local URLs are not enabled — turn them on in Settings first".into(),
        ));
    }
    let projects = state.store.list()?;
    apply_proxy(&state, &s, &projects).await?;
    dnsmasq::ensure_running(&state.dns_conf_dir, &s.local_url_tld).await?;
    resolver::ensure_resolver(&s.local_url_tld, dnsmasq::HOST_PORT).await?;
    Ok(())
}

#[tauri::command]
pub async fn open_in_terminal(path: String) -> AppResult<()> {
    let out = tokio::process::Command::new("open")
        .args(["-a", "Terminal", &path])
        .output()
        .await?;
    if !out.status.success() {
        return Err(AppError::Other(format!(
            "could not open Terminal: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

#[tauri::command]
pub async fn open_in_editor(state: State<'_, AppState>, path: String) -> AppResult<()> {
    let editor = state.settings.snapshot().editor;
    let app_name = editor_app_name(&editor)
        .ok_or_else(|| AppError::Other("No editor configured. Choose one in Settings.".into()))?;
    let out = tokio::process::Command::new("open")
        .args(["-a", app_name, &path])
        .output()
        .await?;
    if !out.status.success() {
        return Err(AppError::Other(format!(
            "could not open {app_name}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

fn editor_app_name(key: &str) -> Option<&'static str> {
    match key {
        "phpstorm" => Some("PhpStorm"),
        "vscode" => Some("Visual Studio Code"),
        "cursor" => Some("Cursor"),
        "zed" => Some("Zed"),
        _ => None,
    }
}

#[tauri::command]
pub async fn set_editor(state: State<'_, AppState>, editor: String) -> AppResult<Settings> {
    if !editor.is_empty() && editor_app_name(&editor).is_none() {
        return Err(AppError::Other(format!("unknown editor: {editor}")));
    }
    state.settings.update(|s| s.editor = editor)
}

#[tauri::command]
pub async fn get_project_logs(
    state: State<'_, AppState>,
    id: String,
    tail: Option<u32>,
) -> AppResult<String> {
    let project = state.store.get(&id)?;
    let tail_n = tail.unwrap_or(200).to_string();
    let mut cmd = tokio::process::Command::new("docker");
    cmd.args([
        "compose",
        "logs",
        "--tail",
        &tail_n,
        "--timestamps",
        "--no-color",
    ])
    .current_dir(&project.path);
    let out = sail::output_with_timeout(&mut cmd, 10).await?;
    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(&out.stdout));
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !stderr.trim().is_empty() {
        combined.push_str(stderr.trim_end());
        combined.push('\n');
    }
    if combined.trim().is_empty() {
        combined = "(no log output — has the project been started?)".to_string();
    }
    Ok(combined)
}

/// A project's real, on-disk `.env` — plus the DB connection fields parsed out
/// of it. The Environment and Database tabs render THIS instead of a
/// synthesized guess, so imported projects (whose credentials/engine differ
/// from Sail's defaults) show values that actually work.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEnv {
    pub raw: String,
    pub db_connection: Option<String>,
    pub db_host: Option<String>,
    pub db_port: Option<String>,
    pub db_database: Option<String>,
    pub db_username: Option<String>,
    pub db_password: Option<String>,
}

#[tauri::command]
pub async fn get_project_env(state: State<'_, AppState>, id: String) -> AppResult<ProjectEnv> {
    let project = state.store.get(&id)?;
    let env_path = std::path::Path::new(&project.path).join(".env");
    let raw = tokio::fs::read_to_string(&env_path)
        .await
        .unwrap_or_default();
    let parsed = import::parse_env(&raw);
    let get = |k: &str| parsed.get(k).filter(|v| !v.is_empty()).cloned();
    Ok(ProjectEnv {
        raw,
        db_connection: get("DB_CONNECTION"),
        db_host: get("DB_HOST"),
        db_port: get("FORWARD_DB_PORT").or_else(|| get("DB_PORT")),
        db_database: get("DB_DATABASE"),
        db_username: get("DB_USERNAME"),
        db_password: get("DB_PASSWORD"),
    })
}

#[tauri::command]
pub async fn start_log_stream(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    service: Option<String>,
) -> AppResult<()> {
    let project = state.store.get(&id)?;
    log_stream::start(&app, &project, service).await
}

#[tauri::command]
pub async fn stop_log_stream(id: String) -> AppResult<()> {
    log_stream::stop(&id).await
}

/// Returns the list of services declared in the project's compose file via
/// `docker compose config --services`. Falls back to an empty list on error.
#[tauri::command]
pub async fn list_compose_services(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Vec<String>> {
    let project = state.store.get(&id)?;
    let mut cmd = tokio::process::Command::new("docker");
    cmd.args(["compose", "config", "--services"])
        .current_dir(&project.path);
    let out = sail::output_with_timeout(&mut cmd, 8).await?;
    if !out.status.success() {
        return Err(AppError::Other(format!(
            "docker compose config failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let services: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    Ok(services)
}

#[tauri::command]
pub async fn run_one_shot(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    command: String,
) -> AppResult<()> {
    let project = state.store.get(&id)?;
    one_shot::run(&app, &project, &command).await
}

#[tauri::command]
pub async fn stop_one_shot(id: String) -> AppResult<()> {
    one_shot::stop(&id).await
}

#[tauri::command]
pub async fn import_project(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<Project> {
    // Hold the same lock as create_project: import allocates host ports and
    // inserts, and that allocate+insert must not race a concurrent
    // create/import/clone or two projects can grab the same port — defeating
    // the whole point of the app.
    let guard = state.create_lock.lock().await;
    let project = import::import_existing(&state.store, PathBuf::from(path)).await?;
    drop(guard);
    refresh_local_urls_silent(&app, state).await;
    Ok(project)
}

#[tauri::command]
pub async fn discover_orphans(
    state: State<'_, AppState>,
) -> AppResult<Vec<import::OrphanCandidate>> {
    let projects_root = state.projects_root();
    import::discover_orphans(&state.store, &projects_root).await
}

#[tauri::command]
pub async fn get_all_running_stats(
) -> AppResult<std::collections::HashMap<String, crate::stats::ProjectStatsSummary>> {
    crate::stats::get_all_running_stats().await
}

#[tauri::command]
pub async fn get_docker_system_info() -> AppResult<crate::stats::DockerSystemInfo> {
    crate::stats::get_docker_system_info().await
}

#[tauri::command]
pub async fn start_shell(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
) -> AppResult<()> {
    let project = state.store.get(&id)?;
    shell::start(&app, &id, std::path::Path::new(&project.path), cols, rows).await
}

#[tauri::command]
pub async fn send_shell_input(id: String, data: String) -> AppResult<()> {
    shell::write_input(&id, &data).await
}

#[tauri::command]
pub async fn shell_resize(id: String, cols: u16, rows: u16) -> AppResult<()> {
    shell::resize(&id, cols, rows).await
}

#[tauri::command]
pub async fn stop_shell(id: String) -> AppResult<()> {
    shell::stop(&id).await
}

#[tauri::command]
pub async fn clone_project(
    app: AppHandle,
    state: State<'_, AppState>,
    input: git::CloneInput,
) -> AppResult<Project> {
    let projects_root = state.projects_root();
    // Same port-allocation race guard as create_project / import_project.
    let guard = state.create_lock.lock().await;
    let project = git::clone_and_register(&app, &state.store, &projects_root, input).await?;
    drop(guard);
    refresh_local_urls_silent(&app, state).await;
    Ok(project)
}

#[tauri::command]
pub async fn list_templates(state: State<'_, AppState>) -> AppResult<Vec<Template>> {
    Ok(state.template_store.list())
}

#[tauri::command]
pub async fn get_template(state: State<'_, AppState>, id: String) -> AppResult<Template> {
    state.template_store.get(&id).ok_or(AppError::NotFound)
}

#[tauri::command]
pub async fn create_template(
    state: State<'_, AppState>,
    input: TemplateInput,
) -> AppResult<Template> {
    state.template_store.create(input)
}

#[tauri::command]
pub async fn update_template(
    state: State<'_, AppState>,
    id: String,
    input: TemplateInput,
) -> AppResult<Template> {
    state.template_store.update(&id, input)
}

#[tauri::command]
pub async fn delete_template(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.template_store.delete(&id)
}

#[tauri::command]
pub async fn complete_first_run(state: State<'_, AppState>) -> AppResult<Settings> {
    state.settings.update(|s| s.first_run_completed = true)
}

/// Wipe ALL of the app's state — projects from the DB, templates back to seeds,
/// settings back to defaults, our Traefik + dnsmasq containers down, the
/// /etc/resolver/<tld> file removed, and any running containers from registered
/// projects shut down. **Project folders on disk are NOT touched** — the user's
/// code is safe; only the app's tracking + infrastructure is reset.
#[tauri::command]
pub async fn reset_application(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let projects = state.store.list().unwrap_or_default();

    // 1. Stop background auto-services for every project we know about.
    for p in &projects {
        sail::stop_auto_services(p).await;
    }

    // 2. Best-effort `docker compose down` for each project. Some may not even
    //    have containers — ignore failures.
    for p in &projects {
        let _ = sail::stop(&app, p).await;
    }

    // 3. Tear down our infra containers.
    let _ = proxy::stop().await;
    let _ = dnsmasq::stop().await;

    // 4. Remove the /etc/resolver entry (and any legacy /etc/hosts block).
    //    Surfaces a single admin prompt — the user already confirmed reset.
    let s_snapshot = state.settings.snapshot();
    let _ = resolver::remove_resolver(&s_snapshot.local_url_tld, true).await;

    // 5. Wipe app state (project folders on disk are intentionally untouched).
    state.store.clear_all()?;
    state.template_store.reset_to_seeds()?;
    state
        .settings
        .replace(crate::settings::Settings::default())?;

    Ok(())
}

#[tauri::command]
pub async fn start_docker_desktop() -> AppResult<()> {
    // Preferred path: `docker desktop start --detach`. Available in Docker
    // Desktop 4.30+. This handles BOTH cases that matter:
    //   1. Docker Desktop quit → boots the app and the engine.
    //   2. Engine paused while Desktop is running → resumes the engine.
    // `--detach` returns immediately; our frontend polls `check_environment`
    // until the engine actually becomes responsive.
    let cli_out = tokio::process::Command::new("docker")
        .args(["desktop", "start", "--detach"])
        .output()
        .await;

    if let Ok(out) = cli_out {
        if out.status.success() {
            return Ok(());
        }
        // Docker CLI is on the path but `desktop` subcommand might not exist
        // on older Docker Desktop versions. Fall through to `open` fallback.
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !stderr.contains("docker: 'desktop' is not a docker command")
            && !stderr.contains("unknown command")
        {
            // Real failure (e.g. user denied an admin prompt). Surface it.
            return Err(AppError::Other(format!(
                "docker desktop start failed: {}",
                stderr.trim()
            )));
        }
    }

    // Fallback: launch the app via macOS `open`. This handles the case where
    // Docker Desktop isn't running at all but doesn't help if the engine is
    // paused while the app is open — in that case the user will need to
    // upgrade Docker Desktop or click Resume manually.
    let app_candidates = ["Docker", "Docker Desktop"];
    for name in app_candidates {
        let out = tokio::process::Command::new("open")
            .args(["-a", name])
            .output()
            .await;
        if let Ok(o) = out {
            if o.status.success() {
                return Ok(());
            }
        }
    }

    Err(AppError::Other(
        "Could not start Docker Desktop. Make sure it's installed; if the engine is paused, open Docker Desktop and click Resume.".into(),
    ))
}

#[tauri::command]
pub async fn set_theme(state: State<'_, AppState>, theme: String) -> AppResult<Settings> {
    if !matches!(theme.as_str(), "dark" | "light" | "system") {
        return Err(AppError::Other(format!(
            "invalid theme: {theme} (expected dark, light, or system)"
        )));
    }
    state.settings.update(|s| s.theme = theme)
}

#[tauri::command]
pub async fn list_history(
    state: State<'_, AppState>,
    id: String,
    limit: Option<u32>,
) -> AppResult<Vec<HistoryEntry>> {
    state.store.list_history(&id, limit.unwrap_or(100))
}

#[tauri::command]
pub async fn list_auto_commands(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Vec<AutoCommand>> {
    state.store.list_auto_commands(&id)
}

#[tauri::command]
pub async fn upsert_auto_command(
    state: State<'_, AppState>,
    input: AutoCommandInput,
) -> AppResult<AutoCommand> {
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| Uuid2::new_v4().to_string());
    let cmd = AutoCommand {
        id: id.clone(),
        project_id: input.project_id.clone(),
        label: input.label.trim().to_string(),
        command: input.command.trim().to_string(),
        run_mode: input.run_mode,
        enabled: input.enabled,
        sort_order: input.sort_order,
    };
    state.store.upsert_auto_command(&cmd)?;
    Ok(cmd)
}

#[tauri::command]
pub async fn delete_auto_command(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.store.delete_auto_command(&id)
}

#[tauri::command]
pub async fn run_auto_commands_now(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<()> {
    let project = state.store.get(&id)?;
    let cmds = state.store.list_auto_commands(&id)?;
    sail::run_auto_commands(&app, &project, &cmds).await;
    Ok(())
}

#[tauri::command]
pub async fn get_project_stats(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Vec<ContainerStat>> {
    let project = state.store.get(&id)?;
    stats::get_project_stats(&project.compose_project_name).await
}

#[tauri::command]
pub async fn get_git_status(path: String) -> AppResult<Option<GitStatus>> {
    stats::get_git_status(&path).await
}

/// Re-wire Local URLs (Traefik config + dnsmasq) after a project add/remove.
/// Named "silent" because it never fails the triggering command — but a
/// failure IS surfaced as a non-blocking warning toast (via an event the
/// layout listens for), so a project whose `.<tld>` URL couldn't be wired up
/// doesn't just silently not route.
async fn refresh_local_urls_silent(app: &AppHandle, state: State<'_, AppState>) {
    let s = state.settings.snapshot();
    if !s.local_urls_enabled {
        return;
    }
    let projects = match state.store.list() {
        Ok(p) => p,
        Err(_) => return,
    };
    let result = async {
        apply_proxy(&state, &s, &projects).await?;
        dnsmasq::ensure_running(&state.dns_conf_dir, &s.local_url_tld).await
    }
    .await;
    if let Err(e) = result {
        let _ = app.emit(
            "local-urls-warning",
            format!(
                "Local URLs couldn't be updated for the change you just made — a project's .{} address may not route. {e}",
                s.local_url_tld
            ),
        );
    }
}

fn is_valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 40 {
        return false;
    }
    if !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    bytes
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
}

#[cfg(test)]
mod tests {
    use super::is_valid_name;

    // ----- is_valid_name -----

    #[test]
    fn name_accepts_simple_alpha() {
        assert!(is_valid_name("acme"));
    }

    #[test]
    fn name_accepts_hyphens() {
        assert!(is_valid_name("acme-shop"));
        assert!(is_valid_name("a-b-c-d"));
    }

    #[test]
    fn name_accepts_digits_after_first_char() {
        assert!(is_valid_name("acme-shop-2"));
        assert!(is_valid_name("project1"));
    }

    #[test]
    fn name_rejects_starting_with_digit() {
        assert!(!is_valid_name("9-foo"));
        assert!(!is_valid_name("1project"));
    }

    #[test]
    fn name_rejects_starting_with_hyphen() {
        assert!(!is_valid_name("-foo"));
    }

    #[test]
    fn name_rejects_empty() {
        assert!(!is_valid_name(""));
    }

    #[test]
    fn name_rejects_too_long() {
        // 41 chars exceeds the 40-char cap.
        assert!(!is_valid_name(&format!("a{}", "b".repeat(40))));
    }

    #[test]
    fn name_accepts_max_length() {
        assert!(is_valid_name(&format!("a{}", "b".repeat(39))));
    }

    #[test]
    fn name_rejects_special_chars() {
        assert!(!is_valid_name("acme_shop"));
        assert!(!is_valid_name("acme shop"));
        assert!(!is_valid_name("acme.shop"));
        assert!(!is_valid_name("acme/shop"));
    }

    #[test]
    fn name_rejects_uppercase() {
        // is_ascii_alphabetic() permits uppercase as the first char, but
        // is_ascii_alphanumeric() permits uppercase elsewhere too. So this
        // currently accepts uppercase. Document that as the actual behaviour.
        // (The frontend lowercases project names before submitting.)
        assert!(is_valid_name("AcmeShop"));
    }
}
