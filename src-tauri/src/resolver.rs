use tokio::process::Command;

use crate::error::{AppError, AppResult};

const RESOLVER_DIR: &str = "/etc/resolver";

/// TLDs we refuse to route to 127.0.0.1. Two classes, both harmful:
///   1. Real, delegated public TLDs — hijacking these breaks the user's
///      browsing machine-wide (every `*.com` lookup goes to loopback). `dev`
///      and `app` are Google-owned and HSTS-preloaded, so they'd hard-break
///      with no click-through.
///   2. Special-use / system TLDs — `local` (mDNS/Bonjour), `internal`, etc.
///      would break local name resolution.
///
/// Made-up dev TLDs (`sail`, `ddev`, …) and RFC 6761 `test` are intentionally
/// allowed: they never resolve publicly, so routing them locally is safe.
const RESERVED_TLDS: &[&str] = &[
    // Special-use / system (RFC 6761 / 8375 / mDNS / Tor)
    "local",
    "localhost",
    "home",
    "lan",
    "intranet",
    "internal",
    "corp",
    "arpa",
    "onion",
    "example",
    "invalid",
    "alt", // Popular real gTLDs / ccTLDs a user might reasonably type
    "com",
    "net",
    "org",
    "edu",
    "gov",
    "mil",
    "int",
    "io",
    "co",
    "dev",
    "app",
    "xyz",
    "me",
    "info",
    "biz",
    "online",
    "site",
    "tech",
    "store",
    "shop",
    "cloud",
    "page",
    "web",
    "uk",
    "de",
    "nl",
    "fr",
    "es",
    "it",
    "us",
    "ca",
    "au",
    "jp",
    "cn",
    "ru",
    "br",
    "in",
    "eu",
];

/// True when `tld` is safe to interpolate into the elevated shell command
/// built in `run_admin`. Restricting to `[a-z0-9-]` (no dots, spaces, `$`,
/// backticks, quotes, `;`) is what actually closes the root command-injection
/// vector — the value flows into an `osascript … with administrator
/// privileges` string. Validated at every sink, not just the setter, because a
/// hand-edited `settings.json` bypasses the setter's validation.
pub fn is_shell_safe_tld(tld: &str) -> bool {
    let bytes = tld.as_bytes();
    if bytes.len() < 2 || bytes.len() > 32 {
        return false;
    }
    if bytes[0] == b'-' || bytes[bytes.len() - 1] == b'-' {
        return false;
    }
    bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// True when `tld` is both shell-safe AND not a reserved/real TLD we'd be
/// dangerous to hijack. Use this to *create* a resolver; use
/// `is_shell_safe_tld` alone when *removing* one (cleanup must still work for
/// an already-persisted bad TLD).
pub fn is_allowed_tld(tld: &str) -> bool {
    is_shell_safe_tld(tld) && !RESERVED_TLDS.contains(&tld)
}

fn resolver_path(tld: &str) -> String {
    format!("{RESOLVER_DIR}/{tld}")
}

fn expected_content(port: u16) -> String {
    format!("# Sail Manager\nnameserver 127.0.0.1\nport {port}\n")
}

pub async fn ensure_resolver(tld: &str, port: u16) -> AppResult<()> {
    // Defense in depth: never build the elevated command for a TLD that is
    // unsafe to interpolate or dangerous to route, even if a corrupt settings
    // file slipped one past the setter.
    if !is_allowed_tld(tld) {
        return Err(AppError::Other(format!(
            "refusing to route reserved or unsafe TLD '.{tld}' to localhost"
        )));
    }
    let path = resolver_path(tld);
    let want = expected_content(port);
    if let Ok(current) = tokio::fs::read_to_string(&path).await {
        if current == want {
            return Ok(());
        }
    }

    let tmp = std::env::temp_dir().join(format!(
        "sail-manager-resolver-{}-{}.tmp",
        tld,
        std::process::id()
    ));
    tokio::fs::write(&tmp, &want).await?;

    let cmd = format!(
        "/bin/mkdir -p {dir} && /bin/cp {tmp:?} {path:?} && /bin/chmod 644 {path:?}",
        dir = RESOLVER_DIR,
        tmp = tmp.display(),
        path = path,
    );

    run_admin(&cmd, &format!(
        "Sail Manager wants to add a DNS resolver for *.{tld} (one-time setup so .{tld} URLs work without future password prompts)."
    ))
    .await?;

    let _ = tokio::fs::remove_file(&tmp).await;
    Ok(())
}

pub async fn remove_resolver(tld: &str, also_clear_legacy_hosts_block: bool) -> AppResult<()> {
    // Removal must still work for a reserved TLD (so a bad state is cleanable),
    // but the value is interpolated into the elevated command, so it must be
    // shell-safe. A non-shell-safe TLD could never have been written, so treat
    // it as nothing-to-remove rather than erroring.
    if !is_shell_safe_tld(tld) {
        return Ok(());
    }
    let path = resolver_path(tld);
    let resolver_exists = tokio::fs::metadata(&path).await.is_ok();
    let hosts_has_block = if also_clear_legacy_hosts_block {
        match tokio::fs::read_to_string("/etc/hosts").await {
            Ok(s) => s.contains("# >>> sail-manager begin"),
            Err(_) => false,
        }
    } else {
        false
    };

    if !resolver_exists && !hosts_has_block {
        return Ok(());
    }

    let mut parts: Vec<String> = Vec::new();
    if resolver_exists {
        parts.push(format!("/bin/rm -f {path:?}"));
    }
    if hosts_has_block {
        // Strip the managed block from /etc/hosts via sed.
        parts.push(
            "/usr/bin/sed -i '' '/# >>> sail-manager begin/,/# <<< sail-manager end/d' /etc/hosts"
                .to_string(),
        );
        parts.push("/usr/bin/dscacheutil -flushcache".into());
        parts.push("/usr/bin/killall -HUP mDNSResponder".into());
    }
    let cmd = parts.join(" && ");

    run_admin(
        &cmd,
        "Sail Manager wants to remove its DNS resolver and any legacy /etc/hosts entries.",
    )
    .await?;
    Ok(())
}

async fn run_admin(shell_cmd: &str, prompt: &str) -> AppResult<()> {
    let osa = format!(
        r#"do shell script "{cmd}" with prompt "{prompt}" with administrator privileges"#,
        cmd = shell_cmd.replace('\\', "\\\\").replace('"', "\\\""),
        prompt = prompt.replace('\\', "\\\\").replace('"', "\\\""),
    );
    // Absolute path: the elevated prompt must not be shadowed by a rogue
    // `osascript` dropped earlier on PATH (e.g. a writable /usr/local/bin).
    let out = Command::new("/usr/bin/osascript")
        .args(["-e", &osa])
        .output()
        .await?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        if stderr.contains("User canceled") || stderr.contains("(-128)") {
            return Err(AppError::Other(
                "admin password prompt was cancelled".into(),
            ));
        }
        return Err(AppError::Other(format!(
            "admin operation failed: {}",
            stderr.trim()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{expected_content, is_allowed_tld, is_shell_safe_tld, resolver_path};

    #[test]
    fn resolver_path_is_under_etc_resolver() {
        assert_eq!(resolver_path("sail"), "/etc/resolver/sail");
        assert_eq!(resolver_path("local-dev"), "/etc/resolver/local-dev");
    }

    #[test]
    fn expected_content_starts_with_marker_comment() {
        let s = expected_content(5354);
        assert!(s.starts_with("# Sail Manager\n"));
    }

    #[test]
    fn shell_injection_tlds_are_rejected() {
        // The root-command-injection vectors the validation exists to stop.
        for bad in [
            "a$(touch /tmp/x)",
            "a`id`",
            "a;rm",
            "a b",
            "a\"b",
            "a.b",
            "a/b",
        ] {
            assert!(!is_shell_safe_tld(bad), "{bad} must be rejected");
            assert!(!is_allowed_tld(bad), "{bad} must be rejected");
        }
    }

    #[test]
    fn reserved_and_real_tlds_are_not_allowed_but_still_removable() {
        for reserved in ["com", "dev", "app", "local", "localhost", "internal"] {
            assert!(!is_allowed_tld(reserved), "{reserved} must not be routable");
            // …yet shell-safe, so an already-written resolver can be cleaned up.
            assert!(is_shell_safe_tld(reserved));
        }
    }

    #[test]
    fn made_up_dev_tlds_are_allowed() {
        for ok in ["sail", "test", "ddev", "wip", "local-dev"] {
            assert!(is_allowed_tld(ok), "{ok} should be allowed");
        }
    }

    #[test]
    fn expected_content_includes_loopback_nameserver() {
        let s = expected_content(5354);
        assert!(s.contains("nameserver 127.0.0.1\n"));
    }

    #[test]
    fn expected_content_embeds_port() {
        let s = expected_content(5354);
        assert!(s.contains("port 5354\n"));
    }

    #[test]
    fn expected_content_uses_custom_port() {
        let s = expected_content(5300);
        assert!(s.contains("port 5300\n"));
        assert!(!s.contains("port 5354"));
    }

    #[test]
    fn expected_content_ends_with_newline() {
        let s = expected_content(5354);
        assert!(s.ends_with('\n'));
    }
}
