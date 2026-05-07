mod commands;
mod dependencies;
mod dnsmasq;
mod error;
mod git;
mod import;
mod log_stream;
mod models;
mod one_shot;
mod ports;
mod proxy;
mod resolver;
mod sail;
mod scaffolder;
mod settings;
mod shell;
mod state;
mod stats;
mod store;
mod templates;
mod tls;
mod tray;

use tauri::Manager;

use crate::settings::{settings_path, SettingsStore};
use crate::state::AppState;
use crate::store::ProjectStore;
use crate::templates::{templates_path, TemplateStore};

/// Augment PATH so we can find docker, git, npm, etc. when the app is launched
/// from Finder/Spotlight (which gives the process a minimal PATH that doesn't
/// include Homebrew or the Docker Desktop symlink). Without this, every shell-
/// out from the Rust backend fails with "command not found" once the app is
/// installed and run as a packaged .app.
///
/// Only meaningful on macOS — the bundled paths are macOS-specific. On Linux
/// the user's distro PATH already includes /usr/local/bin etc., and on Windows
/// these paths don't exist.
#[cfg(target_os = "macos")]
fn augment_path() {
    use std::collections::HashSet;
    let extras = [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/local/sbin",
        "/Applications/Docker.app/Contents/Resources/bin",
    ];
    let current = std::env::var("PATH").unwrap_or_default();
    let mut seen: HashSet<String> = HashSet::new();
    let mut parts: Vec<String> = Vec::new();
    for p in extras.iter().map(|s| s.to_string()).chain(
        current
            .split(':')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
    ) {
        if seen.insert(p.clone()) {
            parts.push(p);
        }
    }
    std::env::set_var("PATH", parts.join(":"));
}

#[cfg(not(target_os = "macos"))]
fn augment_path() {
    // The user's shell-managed PATH is already correct on Linux/Windows.
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    augment_path();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // Closing the main window hides it instead of quitting the process, so
        // running auto-services + tray icon stay alive in the background. Full
        // exit happens only via the tray's "Quit Sail Manager" item, which
        // calls `app.exit(0)` and bypasses this hook.
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&app_data_dir)?;
            let store = ProjectStore::open(app_data_dir.join("state.db"))?;
            let settings = SettingsStore::open(settings_path(&app_data_dir))?;
            let template_store = TemplateStore::open(templates_path(&app_data_dir))?;

            // Honor a persisted projects_root if the user picked one in the
            // welcome wizard or settings; otherwise default to ~/SailProjects.
            let stored_root = settings.snapshot().projects_root;
            let projects_root = if !stored_root.is_empty() {
                std::path::PathBuf::from(stored_root)
            } else {
                app.path()
                    .home_dir()
                    .map(|h| h.join("SailProjects"))
                    .unwrap_or_else(|_| app_data_dir.join("projects"))
            };
            std::fs::create_dir_all(&projects_root)?;

            let proxy_conf_dir = app_data_dir.join("proxy");
            std::fs::create_dir_all(&proxy_conf_dir)?;
            let dns_conf_dir = app_data_dir.join("dns");
            std::fs::create_dir_all(&dns_conf_dir)?;
            let tls_dir = tls::tls_dir(&app_data_dir);
            std::fs::create_dir_all(&tls_dir)?;

            app.manage(AppState::new(
                store,
                settings,
                template_store,
                projects_root,
                proxy_conf_dir,
                dns_conf_dir,
                tls_dir,
            ));
            tray::setup(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_projects,
            commands::get_project,
            commands::create_project,
            commands::start_project,
            commands::stop_project,
            commands::delete_project,
            commands::refresh_status,
            commands::check_environment,
            commands::check_dependencies,
            commands::set_projects_root,
            commands::get_settings,
            commands::set_local_urls_enabled,
            commands::set_local_url_tld,
            commands::set_local_urls_https,
            commands::resync_local_urls,
            commands::check_local_urls,
            commands::repair_local_urls_quiet,
            commands::open_in_terminal,
            commands::open_in_editor,
            commands::set_editor,
            commands::get_project_logs,
            commands::start_log_stream,
            commands::stop_log_stream,
            commands::import_project,
            commands::clone_project,
            commands::list_templates,
            commands::get_template,
            commands::create_template,
            commands::update_template,
            commands::delete_template,
            commands::complete_first_run,
            commands::set_theme,
            commands::start_docker_desktop,
            commands::reset_application,
            commands::list_history,
            commands::list_auto_commands,
            commands::upsert_auto_command,
            commands::delete_auto_command,
            commands::run_auto_commands_now,
            commands::get_project_stats,
            commands::get_git_status,
            commands::list_compose_services,
            commands::run_one_shot,
            commands::stop_one_shot,
            commands::start_shell,
            commands::send_shell_input,
            commands::shell_resize,
            commands::stop_shell,
            commands::discover_orphans,
            commands::get_all_running_stats,
            commands::get_docker_system_info,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app_handle, event| match event {
            // Graceful shutdown: kill any docker compose log streams + service-mode
            // auto-commands we spawned. They'd be cleaned up by the OS anyway, but
            // doing it here means tidy output and no zombie host processes.
            tauri::RunEvent::ExitRequested { .. } => {
                tauri::async_runtime::block_on(async {
                    log_stream::stop_all().await;
                    shell::stop_all().await;
                    if let Some(state) = app_handle.try_state::<AppState>() {
                        if let Ok(projects) = state.store.list() {
                            for p in projects {
                                sail::stop_auto_services(&p.id).await;
                            }
                        }
                    }
                });
            }
            // Clicking the Dock icon (macOS) when the window is hidden should
            // bring it back. Without this, the only way back in is the tray.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => {
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            _ => {}
        });
}
