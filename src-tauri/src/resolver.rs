use tokio::process::Command;

use crate::error::{AppError, AppResult};

const RESOLVER_DIR: &str = "/etc/resolver";

fn resolver_path(tld: &str) -> String {
    format!("{RESOLVER_DIR}/{tld}")
}

fn expected_content(port: u16) -> String {
    format!("# Sail Manager\nnameserver 127.0.0.1\nport {port}\n")
}

pub async fn ensure_resolver(tld: &str, port: u16) -> AppResult<()> {
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
    let out = Command::new("osascript")
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
    use super::{expected_content, resolver_path};

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
