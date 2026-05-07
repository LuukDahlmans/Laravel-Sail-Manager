use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::{Port, PortService, Project, ProjectStatus, ServiceKind, StarterKit};
use crate::ports::PortAllocator;
use crate::store::ProjectStore;

const COMPOSE_CANDIDATES: &[&str] = &[
    "compose.yaml",
    "compose.yml",
    "docker-compose.yml",
    "docker-compose.yaml",
];

pub async fn import_existing(store: &ProjectStore, path: PathBuf) -> AppResult<Project> {
    if !path.exists() {
        return Err(AppError::Other(format!(
            "path does not exist: {}",
            path.display()
        )));
    }
    if !path.is_dir() {
        return Err(AppError::Other(format!(
            "path is not a directory: {}",
            path.display()
        )));
    }

    let compose_file = COMPOSE_CANDIDATES
        .iter()
        .map(|f| path.join(f))
        .find(|p| p.exists())
        .ok_or_else(|| {
            AppError::Other(format!(
                "no compose file found in {} (looked for compose.yaml, compose.yml, docker-compose.yml, docker-compose.yaml)",
                path.display()
            ))
        })?;
    let _ = compose_file; // Existence-only check.

    let env_path = path.join(".env");
    if !env_path.exists() {
        return Err(AppError::Other(format!(
            ".env not found in {}",
            path.display()
        )));
    }

    let env_contents = tokio::fs::read_to_string(&env_path)
        .await
        .map_err(|e| AppError::Other(format!("could not read .env: {e}")))?;
    let env = parse_env(&env_contents);

    let folder_basename = path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::Other(format!("invalid folder name: {}", path.display())))?;
    let sanitized_basename = sanitize_name(&folder_basename);
    if sanitized_basename.is_empty() {
        return Err(AppError::Other(format!(
            "could not derive a valid project name from folder: {folder_basename}"
        )));
    }

    let app_name = env
        .get("APP_NAME")
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| folder_basename.clone());
    let _ = app_name; // Parsed for completeness; project name uses folder basename.

    let compose_project_name = env
        .get("COMPOSE_PROJECT_NAME")
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| folder_basename.clone());

    let app_port: u16 = env
        .get("APP_PORT")
        .ok_or_else(|| AppError::Other("APP_PORT missing from .env".into()))?
        .parse()
        .map_err(|_| AppError::Other("APP_PORT in .env is not a valid u16 port".into()))?;

    let vite_port: u16 = env
        .get("VITE_PORT")
        .and_then(|v| v.parse().ok())
        .unwrap_or(5173);

    if store.name_exists(&sanitized_basename)? {
        return Err(AppError::NameTaken(sanitized_basename));
    }

    // Derive optional service ports.
    let parse_opt = |key: &str| -> Option<u16> { env.get(key).and_then(|v| v.parse().ok()) };

    let db_port = parse_opt("FORWARD_DB_PORT");
    let redis_port = parse_opt("FORWARD_REDIS_PORT");
    let mailpit_smtp_port = parse_opt("FORWARD_MAILPIT_PORT");
    let mailpit_ui_port = parse_opt("FORWARD_MAILPIT_DASHBOARD_PORT");
    let meilisearch_port = parse_opt("FORWARD_MEILISEARCH_PORT");
    let minio_port = parse_opt("FORWARD_MINIO_PORT");

    // Build services list from which FORWARD_* keys are present.
    let mut services: Vec<ServiceKind> = Vec::new();
    if env.contains_key("FORWARD_DB_PORT") {
        services.push(ServiceKind::Mysql);
    }
    if env.contains_key("FORWARD_REDIS_PORT") {
        services.push(ServiceKind::Redis);
    }
    if env.contains_key("FORWARD_MAILPIT_PORT")
        || env.contains_key("FORWARD_MAILPIT_DASHBOARD_PORT")
    {
        services.push(ServiceKind::Mailpit);
    }
    if env.contains_key("FORWARD_MEILISEARCH_PORT") {
        services.push(ServiceKind::Meilisearch);
    }
    if env.contains_key("FORWARD_MINIO_PORT") {
        services.push(ServiceKind::Minio);
    }

    // Build ports list. App + Vite are required; service ports added when present.
    let mut ports: Vec<Port> = Vec::new();
    ports.push(Port {
        service: PortService::App,
        label: PortService::App.label().to_string(),
        host: app_port,
    });
    ports.push(Port {
        service: PortService::Vite,
        label: PortService::Vite.label().to_string(),
        host: vite_port,
    });
    if let Some(p) = db_port {
        ports.push(Port {
            service: PortService::Mysql,
            label: PortService::Mysql.label().to_string(),
            host: p,
        });
    }
    if let Some(p) = redis_port {
        ports.push(Port {
            service: PortService::Redis,
            label: PortService::Redis.label().to_string(),
            host: p,
        });
    }
    if let Some(p) = mailpit_smtp_port {
        ports.push(Port {
            service: PortService::MailpitSmtp,
            label: PortService::MailpitSmtp.label().to_string(),
            host: p,
        });
    }
    if let Some(p) = mailpit_ui_port {
        ports.push(Port {
            service: PortService::MailpitUi,
            label: PortService::MailpitUi.label().to_string(),
            host: p,
        });
    }
    if let Some(p) = meilisearch_port {
        ports.push(Port {
            service: PortService::Meilisearch,
            label: PortService::Meilisearch.label().to_string(),
            host: p,
        });
    }
    if let Some(p) = minio_port {
        ports.push(Port {
            service: PortService::Minio,
            label: PortService::Minio.label().to_string(),
            host: p,
        });
    }

    // Conflict-resolve each port against the DB. If a port the orphan claims
    // in its .env is already used by another tracked project, reallocate to
    // a fresh free port and rewrite the orphan's .env so Sail picks it up.
    // This is the whole point of the app — import shouldn't fail just because
    // two stock Sail projects both start at APP_PORT=8000.
    let mut session_taken: Vec<u16> = Vec::new();
    let mut conflict_indices: Vec<usize> = Vec::new();
    for (i, port) in ports.iter().enumerate() {
        if store.host_port_in_use(port.host)? {
            conflict_indices.push(i);
        } else {
            session_taken.push(port.host);
        }
    }

    let mut env_updates: Vec<(&'static str, String)> = Vec::new();
    for idx in conflict_indices {
        let ps = ports[idx].service;
        let new_host = PortAllocator::allocate_single(store, ps, &session_taken)?;
        ports[idx].host = new_host;
        session_taken.push(new_host);
        if let Some(env_key) = env_key_for(ps) {
            env_updates.push((env_key, new_host.to_string()));
        }
    }

    if !env_updates.is_empty() {
        let updated_env = apply_env_updates(&env_contents, &env_updates);
        tokio::fs::write(&env_path, updated_env)
            .await
            .map_err(|e| {
                AppError::Other(format!(
                    "could not rewrite .env after port reallocation: {e}"
                ))
            })?;
    }

    let id = Uuid::new_v4().to_string();
    let project = Project {
        id: id.clone(),
        name: sanitized_basename.clone(),
        compose_project_name,
        path: path.display().to_string(),
        status: ProjectStatus::Stopped,
        starter_kit: StarterKit::None,
        php_version: "8.3".to_string(),
        services,
        ports,
        created_at: Utc::now(),
        last_started: None,
    };

    store.insert(&project)?;
    store.get(&id)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrphanCandidate {
    /// Suggested name (sanitized folder basename) — what import_existing
    /// would call the project.
    pub name: String,
    /// Absolute path on disk.
    pub path: String,
    /// The compose file we found (for the user's confidence in the toast).
    pub compose_file: String,
}

/// Scan `projects_root` for subdirectories that look like Sail projects but
/// aren't tracked in the DB yet. Used by the frontend on launch to surface a
/// "We found N projects you can import" prompt to new users.
///
/// A folder qualifies if it:
/// - is a directory directly under `projects_root` (not nested deeper),
/// - contains both a `.env` AND one of the compose-file candidates,
/// - is not already registered (compared by `path` AND by sanitized name).
pub async fn discover_orphans(
    store: &ProjectStore,
    projects_root: &Path,
) -> AppResult<Vec<OrphanCandidate>> {
    if !projects_root.exists() || !projects_root.is_dir() {
        return Ok(Vec::new());
    }

    // Snapshot what's already registered so we don't propose dups.
    let existing = store.list().unwrap_or_default();
    let known_paths: HashSet<String> = existing.iter().map(|p| p.path.clone()).collect();
    let known_names: HashSet<String> = existing.iter().map(|p| p.name.clone()).collect();

    let mut out = Vec::new();
    let mut entries = match tokio::fs::read_dir(projects_root).await {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // Skip dotfiles like .DS_Store.
        let basename = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) if !n.starts_with('.') => n.to_string(),
            _ => continue,
        };
        let path_str = path.display().to_string();
        if known_paths.contains(&path_str) {
            continue;
        }
        let name = sanitize_name(&basename);
        if name.is_empty() || known_names.contains(&name) {
            continue;
        }

        // Must have .env.
        if !path.join(".env").exists() {
            continue;
        }
        // Must have at least one compose file.
        let compose_file = match COMPOSE_CANDIDATES.iter().find(|c| path.join(c).exists()) {
            Some(c) => c.to_string(),
            None => continue,
        };

        out.push(OrphanCandidate {
            name,
            path: path_str,
            compose_file,
        });
    }
    // Sort by name so output is stable.
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn parse_env(contents: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for raw in contents.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Strip optional leading "export " for tolerance.
        let line = line.strip_prefix("export ").unwrap_or(line).trim_start();

        let Some(eq_idx) = line.find('=') else {
            continue;
        };
        let key = line[..eq_idx].trim();
        if key.is_empty() {
            continue;
        }
        let mut value = line[eq_idx + 1..].trim().to_string();

        // Strip a trailing inline comment (only when it's clearly outside quotes).
        if !(value.starts_with('"') || value.starts_with('\'')) {
            if let Some(hash_idx) = value.find(" #") {
                value.truncate(hash_idx);
                value = value.trim_end().to_string();
            }
        }

        // Strip surrounding matched quotes.
        if value.len() >= 2 {
            let first = value.as_bytes()[0];
            let last = value.as_bytes()[value.len() - 1];
            if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
                value = value[1..value.len() - 1].to_string();
            }
        }

        out.insert(key.to_string(), value);
    }
    out
}

/// Map a PortService back to the .env key Sail's stock compose reads.
/// Returns `None` for services the import path doesn't currently parse —
/// extend alongside the `parse_opt` block above if more get wired up.
fn env_key_for(ps: PortService) -> Option<&'static str> {
    match ps {
        PortService::App => Some("APP_PORT"),
        PortService::Vite => Some("VITE_PORT"),
        PortService::Mysql | PortService::Pgsql | PortService::Mariadb => Some("FORWARD_DB_PORT"),
        PortService::Redis | PortService::Valkey => Some("FORWARD_REDIS_PORT"),
        PortService::MailpitSmtp => Some("FORWARD_MAILPIT_PORT"),
        PortService::MailpitUi => Some("FORWARD_MAILPIT_DASHBOARD_PORT"),
        PortService::Meilisearch => Some("FORWARD_MEILISEARCH_PORT"),
        PortService::Minio => Some("FORWARD_MINIO_PORT"),
        _ => None,
    }
}

/// Replace `KEY=VALUE` lines in a .env contents string for the given updates,
/// preserving every other line (comments, blank lines, ordering) and the
/// trailing newline. Keys that don't already appear are appended at the end.
fn apply_env_updates(contents: &str, updates: &[(&'static str, String)]) -> String {
    let mut applied: HashSet<&'static str> = HashSet::new();
    let mut out_lines: Vec<String> = Vec::with_capacity(contents.lines().count() + updates.len());

    for raw in contents.lines() {
        let trimmed = raw.trim_start();
        let no_export = trimmed.strip_prefix("export ").unwrap_or(trimmed);
        let mut replaced = false;
        if let Some(eq_idx) = no_export.find('=') {
            let key = no_export[..eq_idx].trim();
            for (uk, uv) in updates {
                if !applied.contains(*uk) && key == *uk {
                    out_lines.push(format!("{}={}", *uk, uv));
                    applied.insert(*uk);
                    replaced = true;
                    break;
                }
            }
        }
        if !replaced {
            out_lines.push(raw.to_string());
        }
    }

    for (uk, uv) in updates {
        if !applied.contains(*uk) {
            out_lines.push(format!("{}={}", *uk, uv));
        }
    }

    let mut result = out_lines.join("\n");
    if contents.ends_with('\n') {
        result.push('\n');
    }
    result
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

// Allow path arg to be unused-suppressing for some edge tools.
#[allow(dead_code)]
fn _check_path(_: &Path) {}

#[cfg(test)]
mod tests {
    use super::{parse_env, sanitize_name};

    #[test]
    fn parses_basic_key_value_pairs() {
        let env = parse_env("APP_NAME=Sail\nAPP_PORT=8080\n");
        assert_eq!(env.get("APP_NAME").map(String::as_str), Some("Sail"));
        assert_eq!(env.get("APP_PORT").map(String::as_str), Some("8080"));
    }

    #[test]
    fn ignores_comment_lines() {
        let env =
            parse_env("# top-level comment\nAPP_NAME=Foo\n  # leading whitespace\nAPP_PORT=80\n");
        assert_eq!(env.get("APP_NAME").map(String::as_str), Some("Foo"));
        assert_eq!(env.get("APP_PORT").map(String::as_str), Some("80"));
        assert_eq!(env.len(), 2);
    }

    #[test]
    fn ignores_blank_lines() {
        let env = parse_env("\n\nAPP_NAME=Foo\n\n  \nAPP_PORT=80\n");
        assert_eq!(env.len(), 2);
    }

    #[test]
    fn strips_export_prefix() {
        let env = parse_env("export APP_NAME=Foo\nexport   APP_PORT=80\n");
        assert_eq!(env.get("APP_NAME").map(String::as_str), Some("Foo"));
        assert_eq!(env.get("APP_PORT").map(String::as_str), Some("80"));
    }

    #[test]
    fn strips_double_quotes_around_value() {
        let env = parse_env("APP_NAME=\"Sail Manager\"\n");
        assert_eq!(
            env.get("APP_NAME").map(String::as_str),
            Some("Sail Manager")
        );
    }

    #[test]
    fn strips_single_quotes_around_value() {
        let env = parse_env("APP_NAME='Sail Manager'\n");
        assert_eq!(
            env.get("APP_NAME").map(String::as_str),
            Some("Sail Manager")
        );
    }

    #[test]
    fn strips_inline_comment_outside_quotes() {
        let env = parse_env("APP_PORT=8080 # the http port\n");
        assert_eq!(env.get("APP_PORT").map(String::as_str), Some("8080"));
    }

    #[test]
    fn keeps_hash_inside_quoted_value() {
        let env = parse_env("APP_NAME=\"hash#in#value\"\n");
        assert_eq!(
            env.get("APP_NAME").map(String::as_str),
            Some("hash#in#value")
        );
    }

    #[test]
    fn handles_value_with_multiple_equals() {
        // Only the FIRST '=' is treated as a separator. Anything past it is
        // part of the value (after quote/comment processing).
        let env = parse_env("DATABASE_URL=postgres://u:p=word@h:5432/db\n");
        assert_eq!(
            env.get("DATABASE_URL").map(String::as_str),
            Some("postgres://u:p=word@h:5432/db")
        );
    }

    #[test]
    fn drops_lines_with_no_equals() {
        let env = parse_env("APP_NAME=Foo\njustakey\nAPP_PORT=80\n");
        assert_eq!(env.len(), 2);
        assert!(!env.contains_key("justakey"));
    }

    #[test]
    fn drops_lines_with_empty_key() {
        let env = parse_env("=value\nAPP_PORT=80\n");
        assert!(!env.contains_key(""));
        assert_eq!(env.len(), 1);
    }

    #[test]
    fn empty_value_yields_empty_string() {
        let env = parse_env("APP_NAME=\n");
        assert_eq!(env.get("APP_NAME").map(String::as_str), Some(""));
    }

    #[test]
    fn returns_none_for_missing_key() {
        let env = parse_env("APP_NAME=Foo\n");
        assert!(!env.contains_key("DOES_NOT_EXIST"));
    }

    #[test]
    fn sanitize_lowercases_and_keeps_alphanumeric() {
        assert_eq!(sanitize_name("AcmeShop"), "acmeshop");
    }

    #[test]
    fn sanitize_collapses_punctuation_to_dashes() {
        assert_eq!(sanitize_name("Acme  Shop  v2"), "acme-shop-v2");
    }

    #[test]
    fn sanitize_strips_trailing_dashes() {
        assert_eq!(sanitize_name("acme--shop--"), "acme-shop");
    }

    #[test]
    fn sanitize_drops_leading_punctuation() {
        // Leading non-alphanumeric chars don't insert a leading dash.
        assert_eq!(sanitize_name("   acme"), "acme");
    }

    #[test]
    fn sanitize_returns_empty_for_pure_punctuation() {
        assert_eq!(sanitize_name("---"), "");
        assert_eq!(sanitize_name(""), "");
    }
}
