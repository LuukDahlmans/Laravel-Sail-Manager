use std::sync::{Arc, Mutex};

use chrono::Utc;
use serde::Serialize;
use tauri::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Emitter, Listener, Manager,
};

use crate::models::{PortService, Project, ProjectStatus};
use crate::sail;
use crate::state::AppState;

/// Tauri event the frontend listens to in order to navigate when the user
/// picks "Show details" from a tray submenu.
const NAVIGATE_EVENT: &str = "tray-navigate";

#[derive(Serialize, Clone)]
struct NavigatePayload {
    path: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StatusChange {
    project_id: String,
    status: ProjectStatus,
}

/// Compute how many projects are currently in the `Running` state.
fn running_count(app: &AppHandle) -> usize {
    let state = app.state::<AppState>();
    match state.store.list() {
        Ok(projects) => projects
            .iter()
            .filter(|p| matches!(p.status, ProjectStatus::Running))
            .count(),
        Err(_) => 0,
    }
}

/// Format the tray title for a given running count.
/// Empty string when zero (so the menu bar stays minimal); otherwise
/// a leading space + count + " running".
fn format_title(count: usize) -> String {
    if count == 0 {
        String::new()
    } else {
        format!(" {} running", count)
    }
}

/// Sort key used to order projects in the tray menu.
/// Lower numbers come first.
fn status_sort_key(status: ProjectStatus) -> u8 {
    match status {
        ProjectStatus::Running => 0,
        ProjectStatus::Starting | ProjectStatus::Stopping => 1,
        ProjectStatus::Stopped => 2,
        ProjectStatus::Error => 3,
    }
}

/// One-character glyph that prefixes the project name in the submenu title.
fn status_symbol(status: ProjectStatus) -> &'static str {
    match status {
        ProjectStatus::Running => "●",
        ProjectStatus::Starting | ProjectStatus::Stopping => "◐",
        ProjectStatus::Stopped => "○",
        ProjectStatus::Error => "!",
    }
}

/// Resolve the URL the "Open <url>" item should point at, given the user's
/// settings and the project's allocated app port.
fn project_url(project: &Project, app: &AppHandle) -> Option<String> {
    let app_port = project
        .ports
        .iter()
        .find(|p| matches!(p.service, PortService::App))
        .map(|p| p.host);
    let settings = app.state::<AppState>().settings.snapshot();
    if settings.local_urls_enabled {
        Some(format!(
            "http://{}.{}",
            project.name, settings.local_url_tld
        ))
    } else {
        app_port.map(|p| format!("http://localhost:{}", p))
    }
}

fn emit_status(app: &AppHandle, project_id: &str, status: ProjectStatus) {
    let _ = app.emit(
        crate::commands::STATUS_EVENT,
        StatusChange {
            project_id: project_id.into(),
            status,
        },
    );
}

/// Mirror of `commands::start_project` for use from the tray. We can't easily
/// invoke the Tauri command directly from Rust without a `tauri::ipc` dispatch,
/// so we replicate the same status-event flow here.
async fn dispatch_start(app: AppHandle, id: String) {
    let state = app.state::<AppState>();
    let project = match state.store.get(&id) {
        Ok(p) => p,
        Err(_) => return,
    };
    let _ = state.store.update_status(&id, ProjectStatus::Starting);
    emit_status(&app, &id, ProjectStatus::Starting);

    match sail::start(&app, &project).await {
        Ok(()) => {
            let _ = state.store.update_status(&id, ProjectStatus::Running);
            let _ = state.store.touch_last_started(&id, Utc::now());
            emit_status(&app, &id, ProjectStatus::Running);
        }
        Err(_) => {
            let _ = state.store.update_status(&id, ProjectStatus::Error);
            emit_status(&app, &id, ProjectStatus::Error);
        }
    }
}

/// Sibling of `dispatch_start` for stop.
async fn dispatch_stop(app: AppHandle, id: String) {
    let state = app.state::<AppState>();
    let project = match state.store.get(&id) {
        Ok(p) => p,
        Err(_) => return,
    };
    let _ = state.store.update_status(&id, ProjectStatus::Stopping);
    emit_status(&app, &id, ProjectStatus::Stopping);

    match sail::stop(&app, &project).await {
        Ok(()) => {
            let _ = state.store.update_status(&id, ProjectStatus::Stopped);
            emit_status(&app, &id, ProjectStatus::Stopped);
        }
        Err(_) => {
            let _ = state.store.update_status(&id, ProjectStatus::Error);
            emit_status(&app, &id, ProjectStatus::Error);
        }
    }
}

/// Build the full tray menu from the current project list and settings.
fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    // 1. Header (disabled).
    let header = MenuItem::with_id(app, "header", "Sail Manager", false, None::<&str>)?;
    let sep_top = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(app, &[&header, &sep_top])?;

    // 2. Projects, sorted by status group then name.
    let state = app.state::<AppState>();
    let mut projects = state.store.list().unwrap_or_default();
    projects.sort_by(|a, b| {
        status_sort_key(a.status)
            .cmp(&status_sort_key(b.status))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    for project in &projects {
        let title = format!("{} {}", status_symbol(project.status), project.name);
        let id_prefix = format!("project:{}", project.id);

        let mut sub_items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = Vec::new();

        let running = matches!(project.status, ProjectStatus::Running);
        let in_transition = matches!(
            project.status,
            ProjectStatus::Starting | ProjectStatus::Stopping
        );

        // Open <url> — only when running.
        if running {
            if let Some(url) = project_url(project, app) {
                let label = format!("Open {}", url);
                let item = MenuItem::with_id(
                    app,
                    format!("{}:open", id_prefix),
                    label,
                    true,
                    None::<&str>,
                )?;
                sub_items.push(Box::new(item));
            }
        }

        // Start — when not running and not transitioning.
        if !running && !in_transition {
            let item = MenuItem::with_id(
                app,
                format!("{}:start", id_prefix),
                "Start",
                true,
                None::<&str>,
            )?;
            sub_items.push(Box::new(item));
        }

        // Stop — when running.
        if running {
            let item = MenuItem::with_id(
                app,
                format!("{}:stop", id_prefix),
                "Stop",
                true,
                None::<&str>,
            )?;
            sub_items.push(Box::new(item));
        }

        // Reveal in Finder.
        let reveal = MenuItem::with_id(
            app,
            format!("{}:reveal", id_prefix),
            "Reveal in Finder",
            true,
            None::<&str>,
        )?;
        sub_items.push(Box::new(reveal));

        // Show details — emit tray-navigate.
        let show = MenuItem::with_id(
            app,
            format!("{}:show", id_prefix),
            "Show details",
            true,
            None::<&str>,
        )?;
        sub_items.push(Box::new(show));

        // Build the submenu from collected items.
        let item_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
            sub_items.iter().map(|b| b.as_ref()).collect();
        let submenu = Submenu::with_id_and_items(app, id_prefix.clone(), title, true, &item_refs)?;
        menu.append(&submenu)?;
    }

    // 3. Footer.
    let sep_bot = PredefinedMenuItem::separator(app)?;
    let show_item = MenuItem::with_id(app, "show", "Show Sail Manager", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit Sail Manager", true, None::<&str>)?;
    menu.append(&sep_bot)?;
    menu.append(&show_item)?;
    menu.append(&quit_item)?;

    Ok(menu)
}

/// Handle a click on any tray menu item.
fn handle_menu_event(app: &AppHandle, event: &MenuEvent) {
    let id = event.id.as_ref();

    match id {
        "show" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            return;
        }
        "quit" => {
            app.exit(0);
            return;
        }
        "header" => return,
        _ => {}
    }

    // Per-project items: `project:<id>:<action>`
    let Some(rest) = id.strip_prefix("project:") else {
        return;
    };
    let Some((project_id, action)) = rest.rsplit_once(':') else {
        return;
    };
    let project_id = project_id.to_string();

    match action {
        "open" => {
            // Re-resolve the URL fresh in case settings changed since the
            // menu was built.
            let state = app.state::<AppState>();
            let Ok(project) = state.store.get(&project_id) else {
                return;
            };
            if let Some(url) = project_url(&project, app) {
                let _ = std::process::Command::new("open").arg(url).spawn();
            }
        }
        "start" => {
            let app_clone = app.clone();
            let id = project_id.clone();
            tokio::spawn(async move {
                dispatch_start(app_clone, id).await;
            });
        }
        "stop" => {
            let app_clone = app.clone();
            let id = project_id.clone();
            tokio::spawn(async move {
                dispatch_stop(app_clone, id).await;
            });
        }
        "reveal" => {
            let state = app.state::<AppState>();
            if let Ok(project) = state.store.get(&project_id) {
                let _ = std::process::Command::new("open")
                    .arg(&project.path)
                    .spawn();
            }
        }
        "show" => {
            // Bring the main window forward then ask the frontend to navigate.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            let _ = app.emit(
                NAVIGATE_EVENT,
                NavigatePayload {
                    path: format!("/projects/{}", project_id),
                },
            );
        }
        _ => {}
    }
}

/// Set up the macOS menu-bar tray icon.
///
/// Builds an initial `TrayIcon`, then keeps its menu and title in sync with
/// the project list by listening for `project-status-changed` events.
pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let handle = app.handle().clone();

    let initial_menu = build_menu(&handle)?;

    let mut builder = TrayIconBuilder::new()
        .tooltip("Sail Manager")
        .menu(&initial_menu)
        .on_menu_event(move |app, event: MenuEvent| handle_menu_event(app, &event));

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    let tray = builder.build(app)?;
    let _ = tray.set_title(Some(format_title(running_count(&handle))));

    let tray: Arc<Mutex<Option<TrayIcon<tauri::Wry>>>> = Arc::new(Mutex::new(Some(tray)));
    let tray_for_listener = Arc::clone(&tray);
    let handle_for_listener = handle.clone();

    handle.listen(crate::commands::STATUS_EVENT, move |_event| {
        let title = format_title(running_count(&handle_for_listener));
        let new_menu = build_menu(&handle_for_listener);
        if let Ok(guard) = tray_for_listener.lock() {
            if let Some(t) = guard.as_ref() {
                let _ = t.set_title(Some(title));
                if let Ok(menu) = new_menu {
                    let _ = t.set_menu(Some(menu));
                }
            }
        }
    });

    // Keep the tray alive forever — `TrayIconBuilder::build` already registers
    // it with the app's tray manager, but the listener also holds a clone.
    std::mem::forget(tray);

    Ok(())
}
