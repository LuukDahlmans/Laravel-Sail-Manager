use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerStat {
    pub name: String,
    pub cpu_percent: String,
    pub mem_usage: String,
    pub mem_percent: String,
    pub net_io: String,
    pub block_io: String,
    pub pids: u32,
}

/// Raw shape returned by `docker stats --format '{{json .}}'`. Field names
/// match docker's output exactly so we can deserialize directly. Numeric
/// values (CPUPerc, MemPerc) come through as strings like "0.18%" — we
/// keep them stringified and let the frontend parse the percent.
#[derive(Debug, Deserialize)]
struct DockerStatRow {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "CPUPerc", default)]
    cpu_perc: String,
    #[serde(rename = "MemUsage", default)]
    mem_usage: String,
    #[serde(rename = "MemPerc", default)]
    mem_perc: String,
    #[serde(rename = "NetIO", default)]
    net_io: String,
    #[serde(rename = "BlockIO", default)]
    block_io: String,
    #[serde(rename = "PIDs", default)]
    pids: String,
}

/// `docker stats --no-stream` for the running containers of a single Compose
/// project. `docker stats` itself does NOT accept `--filter`, so we resolve
/// container IDs first via `docker ps --filter` and then pass them as
/// positional args to `stats`. If the project isn't running we get an empty
/// list. Errors bubble up as AppError::Other so callers can surface a toast.
pub async fn get_project_stats(compose_project_name: &str) -> AppResult<Vec<ContainerStat>> {
    let label_filter = format!("label=com.docker.compose.project={compose_project_name}");

    // 1. Resolve the container IDs that belong to this project.
    let ps_out = Command::new("docker")
        .args(["ps", "--filter", &label_filter, "--format", "{{.ID}}"])
        .output()
        .await
        .map_err(|e| AppError::Other(format!("could not run docker ps: {e}")))?;
    if !ps_out.status.success() {
        let stderr = String::from_utf8_lossy(&ps_out.stderr).trim().to_string();
        return Err(AppError::Other(if stderr.is_empty() {
            "docker ps failed".into()
        } else {
            stderr
        }));
    }
    let ids: Vec<String> = String::from_utf8_lossy(&ps_out.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    // 2. Get one snapshot of stats for those IDs. `--no-stream` returns once
    //    instead of streaming, `--no-trunc` keeps full names.
    let mut args: Vec<&str> = vec![
        "stats",
        "--no-stream",
        "--no-trunc",
        "--format",
        "{{json .}}",
    ];
    for id in &ids {
        args.push(id);
    }
    let out = Command::new("docker")
        .args(&args)
        .output()
        .await
        .map_err(|e| AppError::Other(format!("could not run docker stats: {e}")))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(AppError::Other(if stderr.is_empty() {
            "docker stats failed".into()
        } else {
            stderr
        }));
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut rows = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<DockerStatRow>(trimmed) {
            Ok(r) => {
                let pids = r.pids.trim().parse::<u32>().unwrap_or(0);
                rows.push(ContainerStat {
                    name: r.name,
                    cpu_percent: r.cpu_perc,
                    mem_usage: r.mem_usage,
                    mem_percent: r.mem_perc,
                    net_io: r.net_io,
                    block_io: r.block_io,
                    pids,
                });
            }
            // Tolerate odd lines (warnings on stdout, etc.) instead of failing
            // the whole call.
            Err(_) => continue,
        }
    }
    Ok(rows)
}

/// Aggregated stats for a single Compose project. Sum of all of its running
/// container metrics. Used for the per-row CPU/RAM badges on the project list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStatsSummary {
    pub compose_project_name: String,
    pub container_count: u32,
    pub cpu_percent: f64,
    pub mem_used_bytes: u64,
    pub mem_limit_bytes: u64,
}

/// Docker daemon-wide stats — what's shown in the panel at the top of the
/// project list. Combines `docker info`, `docker system df`, and an
/// aggregation of `docker stats`.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DockerSystemInfo {
    pub containers_running: u32,
    pub containers_stopped: u32,
    pub images: u32,
    pub total_cpu_percent: f64,
    pub mem_used_bytes: u64,
    pub mem_total_bytes: u64,
    pub disk_images_bytes: u64,
    pub disk_containers_bytes: u64,
    pub disk_volumes_bytes: u64,
    pub disk_cache_bytes: u64,
}

#[derive(Debug, Deserialize)]
struct DockerInfo {
    #[serde(rename = "ContainersRunning", default)]
    containers_running: u32,
    #[serde(rename = "ContainersStopped", default)]
    containers_stopped: u32,
    #[serde(rename = "Images", default)]
    images: u32,
    #[serde(rename = "MemTotal", default)]
    mem_total: u64,
}

/// `docker system df --format '{{json .}}'` returns one JSON object per line,
/// each with a Type field and a Size string.
#[derive(Debug, Deserialize)]
struct SystemDfRow {
    #[serde(rename = "Type")]
    kind: String,
    #[serde(rename = "Size", default)]
    size: String,
}

#[derive(Debug, Deserialize)]
struct StatsRowWithId {
    #[serde(rename = "ID", default)]
    id: String,
    #[serde(rename = "CPUPerc", default)]
    cpu_perc: String,
    #[serde(rename = "MemUsage", default)]
    mem_usage: String,
}

/// Parse a docker-formatted size string like `"340.2MiB"`, `"2GB"`,
/// `"512KiB"` into bytes. Returns None for unparseable input.
fn parse_size(raw: &str) -> Option<u64> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let split = s.find(|c: char| c.is_ascii_alphabetic())?;
    let (num_str, unit) = s.split_at(split);
    let num: f64 = num_str.trim().parse().ok()?;
    let multiplier: f64 = match unit.trim() {
        "B" => 1.0,
        "kB" | "KB" => 1_000.0,
        "KiB" => 1_024.0,
        "MB" => 1_000_000.0,
        "MiB" => 1_048_576.0,
        "GB" => 1_000_000_000.0,
        "GiB" => 1_073_741_824.0,
        "TB" => 1e12,
        "TiB" => 1_099_511_627_776.0,
        _ => 1.0,
    };
    Some((num * multiplier).max(0.0) as u64)
}

fn parse_percent(raw: &str) -> f64 {
    raw.trim()
        .trim_end_matches('%')
        .parse::<f64>()
        .unwrap_or(0.0)
}

/// One snapshot of `docker stats --no-stream` aggregated by Compose project.
/// Returns a map keyed by `compose_project_name`, suitable for the
/// `loadAllStats` poller on the frontend.
pub async fn get_all_running_stats() -> AppResult<HashMap<String, ProjectStatsSummary>> {
    // 1. List running containers with their compose-project label so we can
    //    map container IDs to projects.
    let ps_out = Command::new("docker")
        .args([
            "ps",
            "--no-trunc",
            "--format",
            "{{.ID}}\t{{.Label \"com.docker.compose.project\"}}",
        ])
        .output()
        .await
        .map_err(|e| AppError::Other(format!("docker ps failed: {e}")))?;
    if !ps_out.status.success() {
        return Ok(HashMap::new());
    }
    let mut id_to_project: HashMap<String, String> = HashMap::new();
    for line in String::from_utf8_lossy(&ps_out.stdout).lines() {
        let mut parts = line.splitn(2, '\t');
        let id = parts.next().unwrap_or("").trim();
        let project = parts.next().unwrap_or("").trim();
        if !id.is_empty() && !project.is_empty() {
            id_to_project.insert(id.to_string(), project.to_string());
        }
    }
    if id_to_project.is_empty() {
        return Ok(HashMap::new());
    }

    // 2. One snapshot of `docker stats` for everything running. Without an
    //    explicit list of IDs this returns all containers; we filter against
    //    the map above so a stray non-Sail container doesn't sneak in.
    let stats_out = Command::new("docker")
        .args([
            "stats",
            "--no-stream",
            "--no-trunc",
            "--format",
            "{{json .}}",
        ])
        .output()
        .await
        .map_err(|e| AppError::Other(format!("docker stats failed: {e}")))?;
    if !stats_out.status.success() {
        return Ok(HashMap::new());
    }

    let mut summary: HashMap<String, ProjectStatsSummary> = HashMap::new();
    for line in String::from_utf8_lossy(&stats_out.stdout).lines() {
        let row: StatsRowWithId = match serde_json::from_str(line.trim()) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let project = match id_to_project.get(&row.id) {
            Some(p) => p.clone(),
            None => continue,
        };

        let cpu = parse_percent(&row.cpu_perc);
        let (used, limit) = match row.mem_usage.split_once('/') {
            Some((u, l)) => (parse_size(u).unwrap_or(0), parse_size(l).unwrap_or(0)),
            None => (0, 0),
        };

        let entry = summary
            .entry(project.clone())
            .or_insert(ProjectStatsSummary {
                compose_project_name: project,
                container_count: 0,
                cpu_percent: 0.0,
                mem_used_bytes: 0,
                mem_limit_bytes: 0,
            });
        entry.container_count += 1;
        entry.cpu_percent += cpu;
        entry.mem_used_bytes += used;
        // Mem limit is a per-container daemon limit; keep the largest as the
        // headline limit (memory_limit isn't really additive in any useful way).
        if limit > entry.mem_limit_bytes {
            entry.mem_limit_bytes = limit;
        }
    }
    Ok(summary)
}

/// Combine `docker info`, `docker system df`, and aggregated `docker stats`
/// into one snapshot for the system panel.
pub async fn get_docker_system_info() -> AppResult<DockerSystemInfo> {
    let mut info = DockerSystemInfo::default();

    // docker info --format '{{json .}}'
    if let Ok(out) = Command::new("docker")
        .args(["info", "--format", "{{json .}}"])
        .output()
        .await
    {
        if out.status.success() {
            if let Ok(d) = serde_json::from_slice::<DockerInfo>(&out.stdout) {
                info.containers_running = d.containers_running;
                info.containers_stopped = d.containers_stopped;
                info.images = d.images;
                info.mem_total_bytes = d.mem_total;
            }
        }
    }

    // docker system df --format '{{json .}}' returns one row per resource type
    if let Ok(out) = Command::new("docker")
        .args(["system", "df", "--format", "{{json .}}"])
        .output()
        .await
    {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let row: SystemDfRow = match serde_json::from_str(line.trim()) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let bytes = parse_size(&row.size).unwrap_or(0);
                match row.kind.as_str() {
                    "Images" => info.disk_images_bytes = bytes,
                    "Containers" => info.disk_containers_bytes = bytes,
                    "Local Volumes" => info.disk_volumes_bytes = bytes,
                    "Build Cache" => info.disk_cache_bytes = bytes,
                    _ => {}
                }
            }
        }
    }

    // Aggregate cpu + mem from running container stats.
    if let Ok(per_project) = get_all_running_stats().await {
        let mut cpu = 0.0;
        let mut mem = 0u64;
        for v in per_project.values() {
            cpu += v.cpu_percent;
            mem = mem.saturating_add(v.mem_used_bytes);
        }
        info.total_cpu_percent = cpu;
        info.mem_used_bytes = mem;
    }

    Ok(info)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub branch: String,
    pub dirty: bool,
    pub ahead: u32,
    pub behind: u32,
}

/// Best-effort git inspection. Returns None when the path is missing, isn't a
/// git repo, or git isn't installed — callers treat None as "not a repo" and
/// render the gray dot.
pub async fn get_git_status(path: &str) -> AppResult<Option<GitStatus>> {
    let p = Path::new(path);
    if !p.exists() {
        return Ok(None);
    }

    // First: is this even a git working tree? `rev-parse --is-inside-work-tree`
    // exits non-zero if not, which we treat as None.
    let inside = Command::new("git")
        .args(["-C", path, "rev-parse", "--is-inside-work-tree"])
        .output()
        .await;
    match inside {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            if s.trim() != "true" {
                return Ok(None);
            }
        }
        // git not installed, path bad, not a repo — all collapse to None.
        _ => return Ok(None),
    }

    let branch = match Command::new("git")
        .args(["-C", path, "rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .await
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => "HEAD".to_string(),
    };

    let dirty = match Command::new("git")
        .args(["-C", path, "status", "--porcelain=v1"])
        .output()
        .await
    {
        Ok(o) if o.status.success() => !String::from_utf8_lossy(&o.stdout).trim().is_empty(),
        _ => false,
    };

    // Best-effort: many freshly-cloned or local-only branches have no upstream.
    // `rev-list --left-right` fails in that case; we just return zeros.
    let (ahead, behind) = match Command::new("git")
        .args([
            "-C",
            path,
            "rev-list",
            "--count",
            "--left-right",
            "@{upstream}...HEAD",
        ])
        .output()
        .await
    {
        Ok(o) if o.status.success() => {
            let raw = String::from_utf8_lossy(&o.stdout);
            let mut parts = raw.split_whitespace();
            let behind = parts
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            let ahead = parts
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            (ahead, behind)
        }
        _ => (0, 0),
    };

    Ok(Some(GitStatus {
        branch,
        dirty,
        ahead,
        behind,
    }))
}
