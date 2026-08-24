//! Adopt Laravel Sail projects Docker already knows about.
//!
//! A user who installs Sail Manager usually has a pile of Sail projects they
//! have run from the terminal with `./vendor/bin/sail up` — some up right now,
//! most stopped. All of them are invisible to the app: they aren't in the DB,
//! so their ports aren't reserved and the very next project we scaffold can be
//! handed a port one of them already claims.
//!
//! This module finds them by reading Compose's own labels off every container
//! (`docker ps -a`, so stopped stacks count), and converts one into a tracked
//! project. The conversion step is the interesting part: a stock Sail `.env`
//! has no `APP_PORT` and no `FORWARD_*` keys at all — Compose falls back to
//! the `${APP_PORT:-80}` defaults baked into `compose.yaml` — which is exactly
//! the input `import::import_existing` rejects. So before importing we write
//! concrete ports into `.env` under the keys that project's own compose file
//! reads:
//!
//! - a **running** stack keeps precisely the ports its containers are bound to,
//!   so adopting it doesn't disturb anything;
//! - a **stopped** stack keeps the ports it was last bound to when those are
//!   still free, and otherwise gets fresh ones from the allocator — which is
//!   the whole reason this app exists.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio::process::Command;

use crate::error::{AppError, AppResult};
use crate::import::{sanitize_name, COMPOSE_CANDIDATES};
use crate::models::PortService;
use crate::ports::PortAllocator;
use crate::sail::output_with_timeout;
use crate::store::ProjectStore;

/// Field separator for the `docker ps --format` template. Compose project
/// names, service names and host paths can all contain `|`, `\t` and spaces
/// in principle, so use a sequence that cannot occur in any of them.
const SEP: &str = "\u{1}";

/// The compose service name Sail gives the PHP container. Together with the
/// `sail-<php>/app` image tag this is what identifies a Compose project as a
/// Sail project rather than any other docker-compose stack the user runs.
const SAIL_APP_SERVICE: &str = "laravel.test";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UntrackedSailProject {
    /// `com.docker.compose.project` — the identity Docker knows it by, and
    /// what the adopted project's `composeProjectName` becomes.
    pub compose_project: String,
    /// Suggested Sail Manager project name (sanitized folder basename).
    pub name: String,
    /// Host directory the compose file lives in.
    pub path: String,
    /// Compose services in the stack: laravel.test, pgsql, redis, …
    pub services: Vec<String>,
    /// True when at least one of its containers is up. Drives the banner's
    /// wording, and decides whether adoption preserves the bound ports
    /// verbatim or re-checks them against what's free.
    pub running: bool,
    /// Host port serving the app, from the container's port bindings. Docker
    /// keeps reporting these for a stopped container when they were
    /// explicitly published; `None` when it doesn't.
    pub app_port: Option<u16>,
    /// The PHP version from the `sail-<version>/app` image tag, when present.
    pub php_version: Option<String>,
    /// Whether `adopt` can run. False when the folder moved, or has no `.env`
    /// or compose file we can read.
    pub importable: bool,
    /// Why `importable` is false, phrased for the banner.
    pub blocked_reason: Option<String>,
}

/// One running container, reduced to the fields we care about.
#[derive(Debug, Clone)]
struct ContainerRow {
    compose_project: String,
    service: String,
    working_dir: String,
    image: String,
    /// `running`, `exited`, `created`, …
    state: String,
    /// container port -> host port, from the `PORTS` column.
    published: BTreeMap<u16, u16>,
}

/// List every Compose container — `-a`, so stopped stacks are included.
async fn list_compose_containers() -> AppResult<Vec<ContainerRow>> {
    let format = format!(
        "{{{{.Label \"com.docker.compose.project\"}}}}{SEP}\
         {{{{.Label \"com.docker.compose.service\"}}}}{SEP}\
         {{{{.Label \"com.docker.compose.project.working_dir\"}}}}{SEP}\
         {{{{.Image}}}}{SEP}\
         {{{{.State}}}}{SEP}\
         {{{{.Ports}}}}"
    );
    let mut cmd = Command::new("docker");
    cmd.args([
        "ps",
        "-a",
        "--filter",
        "label=com.docker.compose.project",
        "--format",
        &format,
    ]);
    let out = output_with_timeout(&mut cmd, 6).await?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(AppError::Other(if stderr.is_empty() {
            "docker ps failed".into()
        } else {
            stderr
        }));
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut rows = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split(SEP).collect();
        if parts.len() < 6 {
            continue;
        }
        let compose_project = parts[0].trim();
        if compose_project.is_empty() {
            continue;
        }
        rows.push(ContainerRow {
            compose_project: compose_project.to_string(),
            service: parts[1].trim().to_string(),
            working_dir: parts[2].trim().to_string(),
            image: parts[3].trim().to_string(),
            state: parts[4].trim().to_string(),
            published: parse_published(parts[5]),
        });
    }
    Ok(rows)
}

/// Parse docker's `PORTS` column into `container port -> host port`.
///
/// The column looks like `0.0.0.0:80->80/tcp, [::]:80->80/tcp, 8025/tcp`.
/// Unpublished entries (no `->`) are skipped; the IPv4 and IPv6 bindings of
/// one mapping name the same host port, so first-wins is fine.
fn parse_published(field: &str) -> BTreeMap<u16, u16> {
    let mut out = BTreeMap::new();
    for entry in field.split(',') {
        let Some((left, right)) = entry.trim().split_once("->") else {
            continue;
        };
        // Host side is `0.0.0.0:8080` or `[::]:8080` — the port is after the
        // last colon either way.
        let Some(host) = left.rsplit(':').next().and_then(|s| s.trim().parse().ok()) else {
            continue;
        };
        let Some(container) = right
            .split('/')
            .next()
            .and_then(|s| s.trim().parse::<u16>().ok())
        else {
            continue;
        };
        out.entry(container).or_insert(host);
    }
    out
}

/// Pull the PHP version out of Sail's `sail-8.4/app` image tag.
fn php_version_from_image(image: &str) -> Option<String> {
    let rest = image.strip_prefix("sail-")?;
    let version = rest.strip_suffix("/app")?;
    if version.is_empty() || !version.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    Some(version.to_string())
}

/// True if this Compose project is a Laravel Sail stack.
fn is_sail_stack(rows: &[ContainerRow]) -> bool {
    rows.iter()
        .any(|r| r.service == SAIL_APP_SERVICE || php_version_from_image(&r.image).is_some())
}

/// Best-effort canonicalization so `/Users/x/p` and `/Users/x/./p` (or a path
/// behind a symlink) compare equal. Falls back to the raw path.
fn canonical(path: &str) -> String {
    std::fs::canonicalize(path)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.to_string())
}

/// Find Sail projects Docker knows about — running or stopped — that Sail
/// Manager doesn't track yet.
///
/// Already-tracked projects are matched on either their Compose project name
/// or their folder: a project imported under a different name still counts as
/// tracked, and we must not offer to import it twice.
pub async fn discover_untracked(store: &ProjectStore) -> AppResult<Vec<UntrackedSailProject>> {
    let rows = list_compose_containers().await?;

    let tracked = store.list().unwrap_or_default();
    let tracked_compose: HashSet<String> = tracked
        .iter()
        .map(|p| p.compose_project_name.clone())
        .collect();
    let tracked_paths: HashSet<String> = tracked.iter().map(|p| canonical(&p.path)).collect();
    let tracked_names: HashSet<String> = tracked.iter().map(|p| p.name.clone()).collect();

    // Group by Compose project, preserving first-seen order per project.
    let mut grouped: BTreeMap<String, Vec<ContainerRow>> = BTreeMap::new();
    for row in rows {
        grouped
            .entry(row.compose_project.clone())
            .or_default()
            .push(row);
    }

    let mut out = Vec::new();
    for (compose_project, rows) in grouped {
        if !is_sail_stack(&rows) {
            continue;
        }
        if tracked_compose.contains(&compose_project) {
            continue;
        }
        // Every container in a Compose project shares the working dir; take
        // the first non-empty one.
        let Some(working_dir) = rows
            .iter()
            .map(|r| r.working_dir.as_str())
            .find(|d| !d.is_empty())
        else {
            continue;
        };
        if tracked_paths.contains(&canonical(working_dir)) {
            continue;
        }

        // Containers whose folder is gone can never be adopted — they're
        // leftovers from a project the user deleted. Listing them as blocked
        // would be pure noise, so drop them here rather than in the UI.
        let path = PathBuf::from(working_dir);
        if !path.is_dir() {
            continue;
        }

        // Sail runs several containers off the same app image (laravel.test,
        // worker, scheduler, reverb, …); prefer the one publishing the web
        // port, and fall back to any app-image container for the PHP version.
        let app_row = rows
            .iter()
            .filter(|r| r.service == SAIL_APP_SERVICE || php_version_from_image(&r.image).is_some())
            .max_by_key(|r| r.published.contains_key(&80) as u8);
        let app_port = app_row.and_then(|r| r.published.get(&80).copied());
        let php_version = app_row.and_then(|r| php_version_from_image(&r.image));

        let mut services: Vec<String> = rows.iter().map(|r| r.service.clone()).collect();
        services.sort();
        services.dedup();

        let (importable, blocked_reason) = importability(&path, &tracked_names);

        out.push(UntrackedSailProject {
            compose_project,
            name: suggested_name(&path),
            path: working_dir.to_string(),
            services,
            running: rows.iter().any(|r| r.state == "running"),
            app_port,
            php_version,
            importable,
            blocked_reason,
        });
    }

    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// The project name adoption would use — same rule as `import_existing`, so
/// what the banner shows is what ends up in the list.
fn suggested_name(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .map(sanitize_name)
        .unwrap_or_default()
}

/// Decide up front whether adoption can succeed, so the banner can grey out a
/// candidate with a reason instead of failing on click.
fn importability(path: &Path, tracked_names: &HashSet<String>) -> (bool, Option<String>) {
    if !path.is_dir() {
        return (
            false,
            Some("its folder is no longer on disk (moved or deleted).".into()),
        );
    }
    if !path.join(".env").exists() {
        return (false, Some("it has no .env file.".into()));
    }
    if !COMPOSE_CANDIDATES.iter().any(|c| path.join(c).exists()) {
        return (false, Some("its compose file is missing.".into()));
    }
    let name = suggested_name(path);
    if name.is_empty() {
        return (
            false,
            Some("no usable project name can be derived from its folder.".into()),
        );
    }
    if tracked_names.contains(&name) {
        return (
            false,
            Some(format!("a project named \"{name}\" already exists.")),
        );
    }
    (true, None)
}

/// Per compose service, the `(env key, container port)` pairs its `ports:`
/// list is driven by.
///
/// Sail writes every published port as `'${SOME_KEY:-default}:<container>'`,
/// so this reads the mapping straight out of the project's own compose file
/// instead of hardcoding a service→key table that drifts every time upstream
/// Sail adds a service. Text scan rather than a YAML parse, matching how
/// `compose_audit` reads the same file.
pub fn parse_port_env_keys(compose_text: &str) -> HashMap<String, Vec<(String, u16)>> {
    let mut out: HashMap<String, Vec<(String, u16)>> = HashMap::new();
    let mut in_services = false;
    let mut service_indent: Option<usize> = None;
    let mut current: Option<String> = None;
    let mut in_ports = false;
    let mut ports_indent = 0usize;

    for raw in compose_text.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();

        if indent == 0 {
            in_services = trimmed == "services:";
            service_indent = None;
            current = None;
            in_ports = false;
            continue;
        }
        if !in_services {
            continue;
        }

        // The first indented mapping key under `services:` fixes the depth
        // service names live at; everything deeper belongs to a service.
        if service_indent.is_none() && !trimmed.starts_with('-') {
            service_indent = Some(indent);
        }

        if in_ports {
            if trimmed.starts_with('-') && indent >= ports_indent {
                if let Some(svc) = current.as_ref() {
                    let entry = trimmed.trim_start_matches('-').trim();
                    if let Some(pair) = parse_port_mapping(entry) {
                        out.entry(svc.clone()).or_default().push(pair);
                    }
                }
                continue;
            }
            in_ports = false;
        }

        if Some(indent) == service_indent && !trimmed.starts_with('-') {
            current = trimmed
                .strip_suffix(':')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            continue;
        }

        if current.is_some() && trimmed == "ports:" {
            in_ports = true;
            ports_indent = indent;
        }
    }

    out
}

/// Parse one compose port entry into the env key that drives it and the
/// container port it maps to.
///
/// Handles `${APP_PORT:-80}:80`, the same-variable-both-sides form Sail uses
/// for Vite (`${VITE_PORT:-5173}:${VITE_PORT:-5173}`), and an optional host-IP
/// prefix (`127.0.0.1:${APP_PORT:-80}:80`).
fn parse_port_mapping(entry: &str) -> Option<(String, u16)> {
    let mut value = entry.trim().trim_matches(|c| c == '"' || c == '\'').trim();

    // Drop a leading host-IP segment so `127.0.0.1:${X:-1}:1` still parses.
    if !value.starts_with("${") {
        value = value.split_once(':').map(|(_, rest)| rest)?.trim();
        if !value.starts_with("${") {
            return None;
        }
    }

    let (key, after) = split_interpolation(value)?;
    let container = after.strip_prefix(':')?.split('/').next()?.trim();
    let container_port = if container.starts_with("${") {
        // `${VITE_PORT:-5173}` on the container side: the default is the
        // container port whenever the variable is unset, which is exactly the
        // case we are filling in.
        default_of(container)?
    } else {
        container.parse().ok()?
    };
    Some((key, container_port))
}

/// Split `${KEY:-default}rest` into the key and whatever follows the brace.
fn split_interpolation(value: &str) -> Option<(String, &str)> {
    let rest = value.strip_prefix("${")?;
    let close = rest.find('}')?;
    let key = rest[..close].split(":-").next()?.trim();
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    Some((key.to_string(), &rest[close + 1..]))
}

/// The `default` out of `${KEY:-default}`, parsed as a port.
fn default_of(value: &str) -> Option<u16> {
    let rest = value.strip_prefix("${")?;
    let close = rest.find('}')?;
    rest[..close].split_once(":-")?.1.trim().parse().ok()
}

/// Result of adopting one running project.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptOutcome {
    pub project: crate::models::Project,
    /// `.env` keys we added because the project relied on compose defaults.
    pub pinned_keys: Vec<String>,
    /// True when port-conflict resolution moved the app off the port its
    /// containers are bound to — the project needs a restart to match.
    pub needs_restart: bool,
}

/// Convert an untracked Sail project into a tracked one.
///
/// Rediscovers the candidate from Docker rather than trusting a path from the
/// frontend, resolves its ports into `.env`, then hands off to the normal
/// import path so an adopted project is indistinguishable from an imported one.
pub async fn adopt(store: &ProjectStore, compose_project: &str) -> AppResult<AdoptOutcome> {
    let candidates = discover_untracked(store).await?;
    let candidate = candidates
        .into_iter()
        .find(|c| c.compose_project == compose_project)
        .ok_or_else(|| {
            AppError::Other(format!(
                "{compose_project} is no longer in Docker, or is already tracked"
            ))
        })?;

    if !candidate.importable {
        return Err(AppError::Other(format!(
            "cannot import {}: {}",
            candidate.name,
            candidate
                .blocked_reason
                .unwrap_or_else(|| "unknown reason".into())
        )));
    }

    let path = PathBuf::from(&candidate.path);
    let pinned_keys = pin_ports(store, &path, compose_project, candidate.running).await?;

    let project = crate::import::import_existing(store, path).await?;

    // import_existing runs its own conflict pass on top of ours. For a running
    // project that can move the app off the port its containers are bound to,
    // and then the new port isn't real until a restart.
    let tracked_app_port = project
        .ports
        .iter()
        .find(|p| p.service == crate::models::PortService::App)
        .map(|p| p.host);
    let needs_restart = candidate.running
        && match (candidate.app_port, tracked_app_port) {
            (Some(live), Some(tracked)) => live != tracked,
            _ => false,
        };

    Ok(AdoptOutcome {
        project,
        pinned_keys,
        needs_restart,
    })
}

/// Give every port this project's compose file publishes a concrete value in
/// `.env`, under the keys that compose file reads them from.
///
/// Only *missing* keys are written. A key already present in `.env` is what
/// Compose used to bind the port in the first place, so rewriting it would at
/// best be a no-op and at worst clobber a deliberate choice.
///
/// For a **running** project the value is whatever its container is bound to
/// right now, taken verbatim — adopting must not disturb a stack that works.
/// For a **stopped** one the port Docker last recorded is only a starting
/// guess: it's reused when still free, and replaced by a freshly allocated
/// port when something else has taken it in the meantime. Same for the
/// compose default we fall back to when Docker reports no binding at all,
/// which is the common case — that default is `80` / `3306` / `6379` for
/// every stock Sail project on the machine, so handing them all out unchecked
/// would recreate exactly the collision this app exists to prevent.
async fn pin_ports(
    store: &ProjectStore,
    path: &Path,
    compose_project: &str,
    running: bool,
) -> AppResult<Vec<String>> {
    let compose_file = COMPOSE_CANDIDATES
        .iter()
        .map(|f| path.join(f))
        .find(|p| p.exists())
        .ok_or_else(|| AppError::Other(format!("no compose file in {}", path.display())))?;
    let compose_text = tokio::fs::read_to_string(&compose_file)
        .await
        .map_err(|e| AppError::Other(format!("could not read compose file: {e}")))?;
    let port_keys = parse_port_env_keys(&compose_text);

    let env_path = path.join(".env");
    let env_text = tokio::fs::read_to_string(&env_path)
        .await
        .map_err(|e| AppError::Other(format!("could not read .env: {e}")))?;
    let env = crate::import::parse_env(&env_text);

    // Bindings Docker has on record for this project, per compose service.
    let rows = list_compose_containers().await?;
    let bound: HashMap<String, BTreeMap<u16, u16>> = rows
        .into_iter()
        .filter(|r| r.compose_project == compose_project)
        .map(|r| (r.service, r.published))
        .collect();

    let db_connection = env.get("DB_CONNECTION").map(String::as_str);

    // Ports already in .env are off-limits for the allocator: Compose will use
    // them, they're just not ours to change.
    let mut taken: Vec<u16> = env
        .iter()
        .filter(|(k, _)| k.ends_with("_PORT"))
        .filter_map(|(_, v)| v.parse::<u16>().ok())
        .collect();

    // Deterministic order: HashMap iteration isn't stable, and two runs over
    // the same project should hand out the same ports.
    let mut services: Vec<&String> = port_keys.keys().collect();
    services.sort();

    let mut overrides: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for service in services {
        let published = bound.get(service);
        for (key, container_port) in &port_keys[service] {
            if env.contains_key(key) || !seen.insert(key.as_str()) {
                continue;
            }
            let recorded = published.and_then(|p| p.get(container_port).copied());
            let host_port = match recorded {
                // Bound by this project's own container — the freeness probe
                // would (correctly) say it's taken, so don't run it.
                Some(live) if running => live,
                other => {
                    let preferred = other.unwrap_or(*container_port);
                    if PortAllocator::is_free(store, preferred, &taken)? {
                        preferred
                    } else {
                        match port_service_for_env_key(key, db_connection) {
                            Some(ps) => PortAllocator::allocate_single(store, ps, &taken)?,
                            None => PortAllocator::allocate_from_base(store, preferred, &taken)?,
                        }
                    }
                }
            };
            taken.push(host_port);
            overrides.push((key.clone(), host_port.to_string()));
        }
    }

    // Pin the Compose project name too, so the identity we record in the DB is
    // the one Docker already uses for these containers.
    if !env.contains_key("COMPOSE_PROJECT_NAME") {
        overrides.push(("COMPOSE_PROJECT_NAME".into(), compose_project.to_string()));
    }

    if overrides.is_empty() {
        return Ok(Vec::new());
    }

    // Stable order so the block we append to .env reads the same every time.
    overrides.sort_by(|a, b| a.0.cmp(&b.0));
    let borrowed: Vec<(&str, String)> = overrides
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();
    let updated = crate::scaffolder::apply_env_overrides(&env_text, &borrowed);
    tokio::fs::write(&env_path, updated)
        .await
        .map_err(|e| AppError::Other(format!("could not write .env: {e}")))?;

    Ok(overrides.into_iter().map(|(k, _)| k).collect())
}

/// Which `PortService` an .env key belongs to, so a port we have to reallocate
/// lands in that service's curated range instead of next to the compose
/// default. The inverse of `import::env_key_for`; keys with no equivalent
/// (Reverb, anything custom) return `None` and get scanned from their own
/// default instead.
fn port_service_for_env_key(key: &str, db_connection: Option<&str>) -> Option<PortService> {
    Some(match key {
        "APP_PORT" => PortService::App,
        "VITE_PORT" => PortService::Vite,
        "FORWARD_DB_PORT" => match db_connection {
            Some("pgsql") | Some("postgres") | Some("postgresql") => PortService::Pgsql,
            Some("mariadb") => PortService::Mariadb,
            Some("mongodb") => PortService::Mongodb,
            _ => PortService::Mysql,
        },
        "FORWARD_REDIS_PORT" => PortService::Redis,
        "FORWARD_VALKEY_PORT" => PortService::Valkey,
        "FORWARD_MEMCACHED_PORT" => PortService::Memcached,
        "FORWARD_MAILPIT_PORT" => PortService::MailpitSmtp,
        "FORWARD_MAILPIT_DASHBOARD_PORT" => PortService::MailpitUi,
        "FORWARD_MEILISEARCH_PORT" => PortService::Meilisearch,
        "FORWARD_TYPESENSE_PORT" => PortService::Typesense,
        "FORWARD_MONGODB_PORT" => PortService::Mongodb,
        "FORWARD_MINIO_PORT" => PortService::Minio,
        "FORWARD_MINIO_CONSOLE_PORT" => PortService::MinioConsole,
        "FORWARD_SELENIUM_PORT" => PortService::Selenium,
        "FORWARD_SOKETI_PORT" => PortService::Soketi,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ipv4_and_ipv6_bindings_to_one_mapping() {
        let m = parse_published("0.0.0.0:80->80/tcp, [::]:80->80/tcp");
        assert_eq!(m.get(&80), Some(&80));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn parses_remapped_host_port() {
        let m = parse_published("0.0.0.0:8081->80/tcp, [::]:8081->80/tcp");
        assert_eq!(m.get(&80), Some(&8081));
    }

    #[test]
    fn parses_multiple_distinct_ports() {
        let m =
            parse_published("0.0.0.0:1025->1025/tcp, [::]:1025->1025/tcp, 0.0.0.0:8025->8025/tcp");
        assert_eq!(m.get(&1025), Some(&1025));
        assert_eq!(m.get(&8025), Some(&8025));
    }

    #[test]
    fn skips_unpublished_ports() {
        let m = parse_published("8025/tcp");
        assert!(m.is_empty());
    }

    #[test]
    fn empty_ports_column_yields_nothing() {
        assert!(parse_published("").is_empty());
    }

    #[test]
    fn reads_php_version_from_sail_image_tag() {
        assert_eq!(
            php_version_from_image("sail-8.4/app").as_deref(),
            Some("8.4")
        );
        assert_eq!(
            php_version_from_image("sail-8.5/app").as_deref(),
            Some("8.5")
        );
    }

    #[test]
    fn non_sail_images_have_no_php_version() {
        assert!(php_version_from_image("redis:alpine").is_none());
        assert!(php_version_from_image("pgvector/pgvector:pg18").is_none());
        assert!(php_version_from_image("sail-nope/app").is_none());
    }

    #[test]
    fn recognizes_sail_stack_by_service_name() {
        let rows = vec![
            row("redis", "redis:alpine"),
            row("laravel.test", "custom/app"),
        ];
        assert!(is_sail_stack(&rows));
    }

    #[test]
    fn recognizes_sail_stack_by_image_tag() {
        let rows = vec![row("app", "sail-8.3/app")];
        assert!(is_sail_stack(&rows));
    }

    #[test]
    fn plain_compose_stack_is_not_a_sail_stack() {
        let rows = vec![row("web", "nginx:latest"), row("db", "postgres:16")];
        assert!(!is_sail_stack(&rows));
    }

    fn row(service: &str, image: &str) -> ContainerRow {
        ContainerRow {
            compose_project: "p".into(),
            service: service.into(),
            working_dir: "/tmp/p".into(),
            image: image.into(),
            state: "exited".into(),
            published: BTreeMap::new(),
        }
    }

    #[test]
    fn a_stopped_sail_stack_is_still_a_sail_stack() {
        // `docker ps -a` is what feeds this, so every row can be exited — the
        // signature is the service name and image, never the state.
        let rows = vec![
            row("laravel.test", "sail-8.4/app"),
            row("mysql", "mysql:8.4"),
        ];
        assert!(rows.iter().all(|r| r.state == "exited"));
        assert!(is_sail_stack(&rows));
    }

    #[test]
    fn sail_worker_containers_alone_identify_the_stack() {
        // Queue workers and schedulers run the same app image under different
        // service names; a stack whose web container was removed is still Sail.
        let rows = vec![
            row("worker", "sail-8.5/app"),
            row("scheduler", "sail-8.5/app"),
        ];
        assert!(is_sail_stack(&rows));
    }

    #[test]
    fn maps_env_keys_to_their_port_service() {
        assert_eq!(
            port_service_for_env_key("APP_PORT", None),
            Some(PortService::App)
        );
        assert_eq!(
            port_service_for_env_key("FORWARD_MAILPIT_DASHBOARD_PORT", None),
            Some(PortService::MailpitUi)
        );
        assert_eq!(
            port_service_for_env_key("FORWARD_SOKETI_PORT", None),
            Some(PortService::Soketi)
        );
    }

    #[test]
    fn db_port_key_follows_the_configured_connection() {
        assert_eq!(
            port_service_for_env_key("FORWARD_DB_PORT", Some("pgsql")),
            Some(PortService::Pgsql)
        );
        assert_eq!(
            port_service_for_env_key("FORWARD_DB_PORT", Some("mariadb")),
            Some(PortService::Mariadb)
        );
        // Unset or unrecognized falls back to MySQL, matching import_existing.
        assert_eq!(
            port_service_for_env_key("FORWARD_DB_PORT", None),
            Some(PortService::Mysql)
        );
    }

    #[test]
    fn unknown_env_keys_have_no_port_service() {
        // Reverb and anything custom get scanned from their own compose
        // default instead of a curated base.
        assert_eq!(port_service_for_env_key("FORWARD_REVERB_PORT", None), None);
        assert_eq!(port_service_for_env_key("DB_PASSWORD", None), None);
    }

    #[test]
    fn parses_simple_interpolated_mapping() {
        assert_eq!(
            parse_port_mapping("'${APP_PORT:-80}:80'"),
            Some(("APP_PORT".into(), 80))
        );
    }

    #[test]
    fn parses_same_variable_on_both_sides() {
        assert_eq!(
            parse_port_mapping("'${VITE_PORT:-5173}:${VITE_PORT:-5173}'"),
            Some(("VITE_PORT".into(), 5173))
        );
    }

    #[test]
    fn parses_mapping_behind_a_host_ip() {
        assert_eq!(
            parse_port_mapping("'127.0.0.1:${FORWARD_DB_PORT:-3306}:3306'"),
            Some(("FORWARD_DB_PORT".into(), 3306))
        );
    }

    #[test]
    fn parses_mapping_with_protocol_suffix() {
        assert_eq!(
            parse_port_mapping("'${FORWARD_DNS_PORT:-53}:53/udp'"),
            Some(("FORWARD_DNS_PORT".into(), 53))
        );
    }

    #[test]
    fn rejects_literal_mapping_with_no_variable() {
        assert_eq!(parse_port_mapping("'8080:80'"), None);
    }

    #[test]
    fn rejects_variable_without_container_port() {
        assert_eq!(parse_port_mapping("'${APP_PORT:-80}'"), None);
    }

    const SAIL_COMPOSE: &str = r#"
services:
    laravel.test:
        build:
            context: './vendor/laravel/sail/runtimes/8.4'
        image: 'sail-8.4/app'
        ports:
            - '${APP_PORT:-80}:80'
            - '${VITE_PORT:-5173}:${VITE_PORT:-5173}'
        environment:
            WWWUSER: '${WWWUSER}'
        volumes:
            - '.:/var/www/html'
    pgsql:
        image: 'pgvector/pgvector:pg17'
        ports:
            - '${FORWARD_DB_PORT:-5432}:5432'
        environment:
            PGPASSWORD: '${DB_PASSWORD:-secret}'
    mailpit:
        image: 'axllent/mailpit:latest'
        ports:
            - '${FORWARD_MAILPIT_PORT:-1025}:1025'
            - '${FORWARD_MAILPIT_DASHBOARD_PORT:-8025}:8025'
networks:
    sail:
        driver: bridge
volumes:
    sail-pgsql:
        driver: local
"#;

    #[test]
    fn maps_every_service_to_its_port_env_keys() {
        let keys = parse_port_env_keys(SAIL_COMPOSE);
        assert_eq!(
            keys.get("laravel.test").unwrap(),
            &vec![
                ("APP_PORT".to_string(), 80),
                ("VITE_PORT".to_string(), 5173)
            ]
        );
        assert_eq!(
            keys.get("pgsql").unwrap(),
            &vec![("FORWARD_DB_PORT".to_string(), 5432)]
        );
        assert_eq!(
            keys.get("mailpit").unwrap(),
            &vec![
                ("FORWARD_MAILPIT_PORT".to_string(), 1025),
                ("FORWARD_MAILPIT_DASHBOARD_PORT".to_string(), 8025),
            ]
        );
    }

    #[test]
    fn ignores_top_level_sections_that_are_not_services() {
        let keys = parse_port_env_keys(SAIL_COMPOSE);
        assert!(!keys.contains_key("sail"));
        assert!(!keys.contains_key("sail-pgsql"));
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn environment_block_does_not_leak_into_ports() {
        // `PGPASSWORD: '${DB_PASSWORD:-secret}'` sits right after pgsql's
        // ports list; the scan must close the block at the `environment:` key.
        let keys = parse_port_env_keys(SAIL_COMPOSE);
        assert_eq!(keys.get("pgsql").unwrap().len(), 1);
    }

    #[test]
    fn compose_with_no_services_yields_nothing() {
        assert!(
            parse_port_env_keys("volumes:\n    sail-mysql:\n        driver: local\n").is_empty()
        );
    }

    #[test]
    fn service_without_ports_is_absent() {
        let keys = parse_port_env_keys(
            "services:\n    redis:\n        image: 'redis:alpine'\n        volumes:\n            - 'sail-redis:/data'\n",
        );
        assert!(keys.is_empty());
    }
}
