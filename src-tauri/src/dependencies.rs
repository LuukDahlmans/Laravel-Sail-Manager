use std::time::Duration;

use serde::Serialize;
use tokio::process::Command;
use tokio::time::timeout;

/// Status of a single external tool the user might want installed locally.
/// Used by the welcome wizard's system-check step.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    /// Stable id used by the frontend to key list items.
    pub id: String,
    pub label: String,
    pub purpose: String,
    /// True for tools Sail Manager itself can't run without (Docker only).
    pub required: bool,
    pub installed: bool,
    pub version: Option<String>,
    pub install_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyCheck {
    pub tools: Vec<ToolStatus>,
}

struct Probe {
    id: &'static str,
    label: &'static str,
    purpose: &'static str,
    required: bool,
    bin: &'static str,
    args: &'static [&'static str],
    install_url: &'static str,
}

const PROBES: &[Probe] = &[
    Probe {
        id: "docker",
        label: "Docker Desktop",
        purpose: "Required. Runs every Sail project's containers.",
        required: true,
        bin: "docker",
        args: &["--version"],
        install_url: "https://www.docker.com/products/docker-desktop/",
    },
    Probe {
        id: "git",
        label: "Git",
        purpose: "Used by the Clone from Git flow and version-control tooling.",
        required: false,
        bin: "git",
        args: &["--version"],
        install_url: "https://git-scm.com/download/mac",
    },
    Probe {
        id: "php",
        label: "PHP",
        purpose: "Optional. Sail runs PHP in Docker, but a local PHP is handy for ad-hoc commands.",
        required: false,
        bin: "php",
        args: &["--version"],
        install_url: "https://herd.laravel.com/",
    },
    Probe {
        id: "composer",
        label: "Composer",
        purpose: "Optional. Useful if you script outside of Sail (e.g. composer global packages).",
        required: false,
        bin: "composer",
        args: &["--version"],
        install_url: "https://getcomposer.org/download/",
    },
    Probe {
        id: "node",
        label: "Node.js",
        purpose: "Optional. Most Laravel apps run Vite inside Sail; local Node is only needed if you build assets on the host.",
        required: false,
        bin: "node",
        args: &["--version"],
        install_url: "https://nodejs.org/",
    },
    Probe {
        id: "laravel",
        label: "Laravel installer",
        purpose: "Optional. Sail Manager scaffolds via Docker, so the global laravel binary isn't required.",
        required: false,
        bin: "laravel",
        args: &["--version"],
        install_url: "https://laravel.com/docs/installation",
    },
];

/// Probe each tool and return their statuses in the order declared above.
/// Each probe is bounded by a short timeout so a hung binary can't stall the
/// welcome wizard. Sequential rather than parallel — missing binaries fail
/// fast, and the whole check finishes well under a second on a typical Mac.
pub async fn check_all() -> DependencyCheck {
    let mut tools = Vec::with_capacity(PROBES.len());
    for p in PROBES {
        let (installed, version) = match probe(p).await {
            Some(v) => (true, Some(v)),
            None => (false, None),
        };
        tools.push(ToolStatus {
            id: p.id.to_string(),
            label: p.label.to_string(),
            purpose: p.purpose.to_string(),
            required: p.required,
            installed,
            version,
            install_url: p.install_url.to_string(),
        });
    }
    DependencyCheck { tools }
}

async fn probe(p: &Probe) -> Option<String> {
    let mut cmd = Command::new(p.bin);
    cmd.args(p.args);
    let run = cmd.output();
    let out = timeout(Duration::from_secs(3), run).await.ok()?.ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&out.stdout);
    let first = raw.lines().next().unwrap_or("").trim();
    if first.is_empty() {
        return None;
    }
    Some(extract_version(first).unwrap_or_else(|| first.to_string()))
}

/// Pull a "X.Y(.Z)" semver-ish substring out of a noisy `--version` line, so
/// the UI shows "2.46.1" rather than "git version 2.46.1 (Apple Git-...)".
fn extract_version(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let mut start = None;
    for (i, b) in bytes.iter().enumerate() {
        if b.is_ascii_digit() {
            start = Some(i);
            break;
        }
    }
    let s = start?;
    let mut end = s;
    while end < bytes.len() {
        let b = bytes[end];
        if b.is_ascii_digit() || b == b'.' {
            end += 1;
        } else {
            break;
        }
    }
    let v = &line[s..end];
    if v.is_empty() {
        None
    } else {
        Some(v.trim_end_matches('.').to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::extract_version;

    #[test]
    fn extracts_simple_semver() {
        assert_eq!(extract_version("git version 2.46.1"), Some("2.46.1".into()));
    }

    #[test]
    fn extracts_v_prefix() {
        assert_eq!(extract_version("v20.10.0"), Some("20.10.0".into()));
    }

    #[test]
    fn extracts_from_php_line() {
        assert_eq!(
            extract_version("PHP 8.3.10 (cli) (built: ...)"),
            Some("8.3.10".into()),
        );
    }

    #[test]
    fn returns_none_when_no_digits() {
        assert_eq!(extract_version("nothing here"), None);
    }
}
