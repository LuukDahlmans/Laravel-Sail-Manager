//! Static safety audit for a project's compose file.
//!
//! "Clone from Git" and "Import existing" register a project whose
//! `compose.yaml` is authored by whoever wrote the repo — not us. The very
//! next thing the user does is click Start, which runs `docker compose up`
//! against that file. On Docker Desktop for macOS a container can escape to
//! the host if the compose file mounts the Docker socket or the host root,
//! runs `privileged`, shares the host PID namespace, etc. So before we let an
//! untrusted compose file anywhere near the daemon we scan it for those
//! escape hatches and refuse (with a precise reason) if we find one.
//!
//! This is a deliberately conservative *text* scan rather than a full YAML
//! parse: Sail's own stock compose contains none of these directives, so a
//! scaffolded project never trips it, and the false-positive surface on real
//! Laravel repos is essentially nil. A determined author can still obfuscate
//! past a text scan, so this is a safety net, not a sandbox — but it stops the
//! obvious "malicious repo mounts `/`" case cold.

/// A single risky directive found in a compose file, phrased for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeRisk {
    pub directive: String,
    pub explanation: String,
}

/// Normalize a line for matching: strip a trailing `# comment`, trim, and
/// lowercase. Returns `None` for blank/comment-only lines.
fn significant(line: &str) -> Option<String> {
    let no_comment = match line.find('#') {
        Some(i) => &line[..i],
        None => line,
    };
    let trimmed = no_comment.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_lowercase())
}

/// True if `value` (the part after a `key:` or a `- ` list entry) refers to
/// the host root as a bind-mount source. Matches `/:...`, `"/:..."`, and a
/// bare `/` source, but NOT `/app`, `/var/...`, `./foo`, named volumes, etc.
fn is_host_root_mount(value: &str) -> bool {
    let v = value.trim().trim_matches(|c| c == '"' || c == '\'');
    // Short-form `- /:/host` or long-form `source: /`.
    v == "/" || v.starts_with("/:")
}

/// Scan compose file text and return every escape-hatch directive found.
/// Empty result = no host-escape directives detected.
pub fn audit(compose_text: &str) -> Vec<ComposeRisk> {
    let mut risks: Vec<ComposeRisk> = Vec::new();
    let mut push = |directive: &str, explanation: &str| {
        if !risks.iter().any(|r| r.directive == directive) {
            risks.push(ComposeRisk {
                directive: directive.to_string(),
                explanation: explanation.to_string(),
            });
        }
    };

    for raw in compose_text.lines() {
        let Some(line) = significant(raw) else {
            continue;
        };
        // Collapse internal whitespace so `privileged:   true` matches.
        let compact = line.split_whitespace().collect::<Vec<_>>().join(" ");

        if compact == "privileged: true" || compact == "privileged: \"true\"" {
            push(
                "privileged: true",
                "runs the container with full host device access — a container escape gives root on your Mac.",
            );
        }
        if line.contains("docker.sock") {
            push(
                "docker socket mount",
                "mounts the Docker daemon socket, letting the container start further containers as root on the host.",
            );
        }
        if compact == "pid: host" || compact == "pid: \"host\"" {
            push(
                "pid: host",
                "shares the host process namespace, exposing and allowing interference with host processes.",
            );
        }
        if compact == "network_mode: host" || compact == "network_mode: \"host\"" {
            push(
                "network_mode: host",
                "removes network isolation, binding the container directly onto your host's network stack.",
            );
        }
        if compact == "ipc: host" || compact == "ipc: \"host\"" {
            push(
                "ipc: host",
                "shares the host IPC namespace with the container.",
            );
        }
        if line.contains("unconfined") {
            push(
                "security_opt: unconfined",
                "disables the default seccomp/AppArmor sandbox, widening the container-escape surface.",
            );
        }
        if compact.contains("sys_admin") || compact == "- all" {
            push(
                "cap_add: SYS_ADMIN/ALL",
                "grants dangerous kernel capabilities that enable host escape.",
            );
        }

        // Bind-mount of host root, in either short or long form.
        //   - /:/host          (short)
        //   source: /          (long)
        if let Some(rest) = line.strip_prefix("- ") {
            if is_host_root_mount(rest) {
                push(
                    "host root bind-mount",
                    "mounts your entire filesystem (`/`) into the container, exposing every file on your Mac.",
                );
            }
        }
        if let Some(rest) = line.strip_prefix("source:") {
            if is_host_root_mount(rest) {
                push(
                    "host root bind-mount",
                    "mounts your entire filesystem (`/`) into the container, exposing every file on your Mac.",
                );
            }
        }
    }

    risks
}

/// Render the risks into a single user-facing error message.
pub fn describe(risks: &[ComposeRisk]) -> String {
    let mut msg = String::from(
        "This project's compose file requests host access that could compromise your Mac when the containers start:\n",
    );
    for r in risks {
        msg.push_str(&format!("\n  \u{2022} {} — {}", r.directive, r.explanation));
    }
    msg.push_str(
        "\n\nSail Manager won't run it as-is. If you trust this project, remove those directives from its compose file and try again.",
    );
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_sail_compose_is_clean() {
        // Representative slice of Sail's generated compose.yaml.
        let sail = r#"
services:
    laravel.test:
        build:
            context: './vendor/laravel/sail/runtimes/8.3'
        ports:
            - '${APP_PORT:-80}:80'
        environment:
            WWWUSER: '${WWWUSER}'
        volumes:
            - '.:/var/www/html'
        networks:
            - sail
        depends_on:
            - mysql
    mysql:
        image: 'mysql/mysql-server:8.0'
        volumes:
            - 'sail-mysql:/var/lib/mysql'
"#;
        assert!(audit(sail).is_empty());
    }

    #[test]
    fn flags_privileged() {
        let risks = audit("services:\n  x:\n    privileged: true\n");
        assert!(risks.iter().any(|r| r.directive == "privileged: true"));
    }

    #[test]
    fn flags_docker_socket_mount() {
        let risks = audit("    volumes:\n      - /var/run/docker.sock:/var/run/docker.sock\n");
        assert!(risks.iter().any(|r| r.directive == "docker socket mount"));
    }

    #[test]
    fn flags_host_root_mount_short_and_long_form() {
        assert!(audit("    volumes:\n      - /:/host\n")
            .iter()
            .any(|r| r.directive == "host root bind-mount"));
        assert!(
            audit("      - type: bind\n        source: /\n        target: /host\n")
                .iter()
                .any(|r| r.directive == "host root bind-mount")
        );
    }

    #[test]
    fn does_not_flag_normal_app_mount() {
        // The common `.:/var/www/html` and absolute non-root mounts are fine.
        assert!(audit("      - '.:/var/www/html'\n").is_empty());
        assert!(audit("      - /Users/me/project:/app\n").is_empty());
    }

    #[test]
    fn flags_pid_and_network_host() {
        assert!(audit("    pid: host\n")
            .iter()
            .any(|r| r.directive == "pid: host"));
        assert!(audit("    network_mode: \"host\"\n")
            .iter()
            .any(|r| r.directive == "network_mode: host"));
    }

    #[test]
    fn ignores_directives_inside_comments() {
        assert!(audit("    # privileged: true is dangerous, don't do it\n").is_empty());
    }
}
