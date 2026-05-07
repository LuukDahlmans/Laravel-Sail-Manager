use std::path::{Path, PathBuf};

use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose,
    Ia5String, IsCa, KeyPair, KeyUsagePurpose, SanType,
};
use time::{Duration, OffsetDateTime};
use tokio::process::Command;

use crate::error::{AppError, AppResult};

/// CA + per-TLD wildcard cert directory. Sits under the app data dir so it
/// follows the user's profile and survives upgrades.
pub fn tls_dir(app_data: &Path) -> PathBuf {
    app_data.join("tls")
}

pub fn ca_cert_path(tls_dir: &Path) -> PathBuf {
    tls_dir.join("ca.pem")
}
pub fn ca_key_path(tls_dir: &Path) -> PathBuf {
    tls_dir.join("ca-key.pem")
}
pub fn wildcard_cert_path(tls_dir: &Path, tld: &str) -> PathBuf {
    tls_dir.join(format!("{tld}-wildcard.pem"))
}
pub fn wildcard_key_path(tls_dir: &Path, tld: &str) -> PathBuf {
    tls_dir.join(format!("{tld}-wildcard-key.pem"))
}

const CA_COMMON_NAME: &str = "Sail Manager Local CA";

/// Generate `ca.pem` + `ca-key.pem` if they don't already exist. Idempotent —
/// won't regenerate if both files are present.
pub async fn ensure_ca(tls_dir: &Path) -> AppResult<()> {
    ensure_ca_inner(tls_dir, false).await
}

/// Always regenerate the CA, even if it exists. Used by the explicit toggle
/// path so an old CA with broken dates / metadata gets replaced.
pub async fn force_regen_ca(tls_dir: &Path) -> AppResult<()> {
    ensure_ca_inner(tls_dir, true).await
}

async fn ensure_ca_inner(tls_dir: &Path, force: bool) -> AppResult<()> {
    tokio::fs::create_dir_all(tls_dir).await?;
    let cert_path = ca_cert_path(tls_dir);
    let key_path = ca_key_path(tls_dir);
    if !force && cert_path.exists() && key_path.exists() {
        return Ok(());
    }

    let mut params = CertificateParams::default();
    // rcgen's defaults are 1975 → 4096, which Chrome rejects as suspicious
    // and surfaces back as ERR_CERT_COMMON_NAME_INVALID (cert verifier bails
    // before the SAN check). Pin to sensible bounds.
    let now = OffsetDateTime::now_utc();
    params.not_before = now - Duration::hours(1);
    params.not_after = now + Duration::days(365 * 10);
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, CA_COMMON_NAME);
    dn.push(DnType::OrganizationName, "Sail Manager");
    params.distinguished_name = dn;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];

    let key_pair =
        KeyPair::generate().map_err(|e| AppError::Other(format!("generate CA key: {e}")))?;
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| AppError::Other(format!("self-sign CA: {e}")))?;

    tokio::fs::write(&cert_path, cert.pem()).await?;
    // 0600 on the key would be ideal but std::fs::Permissions on tokio is
    // awkward; the tls_dir lives under ~/Library which is already user-only.
    tokio::fs::write(&key_path, key_pair.serialize_pem()).await?;
    Ok(())
}

/// Generate a cert valid for `*.<tld>`, the bare `<tld>`, `localhost`, and
/// each provided per-project hostname (`myapp.<tld>`, etc.). The explicit
/// per-project SANs are what actually get the green padlock — Chrome
/// silently rejects single-dot wildcard patterns like `*.sail` as too broad,
/// so the wildcard is just a safety net. Pass `force = true` from the
/// explicit toggle path; otherwise we re-issue only if missing or stale.
pub async fn ensure_wildcard_cert(
    tls_dir: &Path,
    tld: &str,
    extra_hosts: &[String],
) -> AppResult<()> {
    ensure_wildcard_cert_inner(tls_dir, tld, extra_hosts, false).await
}

pub async fn force_regen_wildcard_cert(
    tls_dir: &Path,
    tld: &str,
    extra_hosts: &[String],
) -> AppResult<()> {
    ensure_wildcard_cert_inner(tls_dir, tld, extra_hosts, true).await
}

async fn ensure_wildcard_cert_inner(
    tls_dir: &Path,
    tld: &str,
    extra_hosts: &[String],
    force: bool,
) -> AppResult<()> {
    ensure_ca(tls_dir).await?;
    let cert_path = wildcard_cert_path(tls_dir, tld);
    let key_path = wildcard_key_path(tls_dir, tld);

    // Re-issue if the SAN list on disk doesn't already cover every host we
    // need. Without this check, adding a new project would leave the cert
    // out of date until something else triggered a regen. Cheap parse — we
    // only check the PEM contains the host as a literal DNS name.
    let needs_regen = if force || !cert_path.exists() || !key_path.exists() {
        true
    } else {
        match tokio::fs::read_to_string(&cert_path).await {
            Ok(_) => {
                let pem = tokio::fs::read_to_string(&cert_path)
                    .await
                    .unwrap_or_default();
                let parsed = CertificateParams::from_ca_cert_pem(&pem);
                let mut on_disk: Vec<String> = match parsed {
                    Ok(p) => p
                        .subject_alt_names
                        .iter()
                        .filter_map(|s| match s {
                            SanType::DnsName(d) => Some(d.to_string()),
                            _ => None,
                        })
                        .collect(),
                    Err(_) => Vec::new(),
                };
                on_disk.sort();
                let want: Vec<String> = std::iter::once(format!("*.{tld}"))
                    .chain(std::iter::once(tld.to_string()))
                    .chain(std::iter::once("localhost".to_string()))
                    .chain(extra_hosts.iter().cloned())
                    .collect();
                let mut want_sorted = want.clone();
                want_sorted.sort();
                want_sorted.dedup();
                let missing = want_sorted.iter().any(|h| !on_disk.contains(h));
                if missing {
                    true
                } else {
                    // Also age-check: refresh if older than ~300 days.
                    match tokio::fs::metadata(&cert_path).await {
                        Ok(m) => match m.modified() {
                            Ok(t) => t
                                .elapsed()
                                .map(|d| d.as_secs() >= 60 * 60 * 24 * 300)
                                .unwrap_or(true),
                            Err(_) => true,
                        },
                        Err(_) => true,
                    }
                }
            }
            Err(_) => true,
        }
    };
    if !needs_regen {
        return Ok(());
    }

    let ca_pem = tokio::fs::read_to_string(ca_cert_path(tls_dir)).await?;
    let ca_key_pem = tokio::fs::read_to_string(ca_key_path(tls_dir)).await?;
    let ca_key_pair =
        KeyPair::from_pem(&ca_key_pem).map_err(|e| AppError::Other(format!("load CA key: {e}")))?;
    let ca_params = CertificateParams::from_ca_cert_pem(&ca_pem)
        .map_err(|e| AppError::Other(format!("parse CA cert: {e}")))?;
    let ca_cert = ca_params
        .self_signed(&ca_key_pair)
        .map_err(|e| AppError::Other(format!("rebuild CA cert: {e}")))?;

    // Build SANs explicitly as DnsName entries so rcgen doesn't have to guess.
    // Browsers verify against SANs only — CN is informational since Chrome 58.
    let mut params = CertificateParams::default();
    // Validity must be within Apple's 825-day cap or the cert is rejected
    // wholesale. We refresh well before that via the 300-day check above.
    let now = OffsetDateTime::now_utc();
    params.not_before = now - Duration::hours(1);
    params.not_after = now + Duration::days(365);

    let mut sans: Vec<SanType> = Vec::with_capacity(3 + extra_hosts.len());
    let push_dns = |s: &str, sans: &mut Vec<SanType>| -> AppResult<()> {
        let ia = Ia5String::try_from(s.to_string())
            .map_err(|e| AppError::Other(format!("invalid SAN '{s}': {e}")))?;
        sans.push(SanType::DnsName(ia));
        Ok(())
    };
    push_dns(&format!("*.{tld}"), &mut sans)?;
    push_dns(tld, &mut sans)?;
    push_dns("localhost", &mut sans)?;
    // Per-project explicit SANs. These are what actually get matched in
    // browsers — Chrome rejects `*.sail`-style single-label wildcards as
    // too broad, so we always need the literal hostname in the SAN list.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    seen.insert(format!("*.{tld}"));
    seen.insert(tld.to_string());
    seen.insert("localhost".to_string());
    for host in extra_hosts {
        if seen.insert(host.clone()) {
            push_dns(host, &mut sans)?;
        }
    }
    params.subject_alt_names = sans;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, format!("*.{tld}"));
    dn.push(DnType::OrganizationName, "Sail Manager");
    params.distinguished_name = dn;
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

    let key_pair =
        KeyPair::generate().map_err(|e| AppError::Other(format!("generate cert key: {e}")))?;
    let cert = params
        .signed_by(&key_pair, &ca_cert, &ca_key_pair)
        .map_err(|e| AppError::Other(format!("sign cert: {e}")))?;

    tokio::fs::write(&cert_path, cert.pem()).await?;
    tokio::fs::write(&key_path, key_pair.serialize_pem()).await?;
    Ok(())
}

/// Trust the CA in the user's login keychain so Apple's TLS verifier honors
/// the chain. We avoid the system keychain on purpose: writing there needs
/// admin trust-setting authorization which the Security Server only allows
/// from interactive Terminal contexts (sudo/TTY) — `osascript with
/// administrator privileges` runs as root but still hits "no user
/// interaction was possible" because the trust-settings authorization wants
/// its own UI prompt that osascript can't satisfy.
///
/// User-domain trust on login.keychain IS honored by macOS for SSL chain
/// validation (both Chrome and Safari go through the Apple verifier), so
/// long as the cert itself isn't otherwise broken (dates, SAN, etc.).
pub async fn install_ca_to_keychain(tls_dir: &Path) -> AppResult<()> {
    let cert_path = ca_cert_path(tls_dir);
    if !cert_path.exists() {
        return Err(AppError::Other(
            "CA cert is missing — run ensure_ca first".into(),
        ));
    }
    let home =
        std::env::var("HOME").map_err(|_| AppError::Other("HOME env var is not set".into()))?;
    let keychain = format!("{home}/Library/Keychains/login.keychain-db");

    let out = Command::new("/usr/bin/security")
        .args([
            "add-trusted-cert",
            "-r",
            "trustRoot",
            "-k",
            &keychain,
            cert_path.to_str().unwrap_or(""),
        ])
        .output()
        .await
        .map_err(|e| AppError::Other(format!("invoke security: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(AppError::Other(format!(
            "could not install CA into login keychain: {stderr}"
        )));
    }
    Ok(())
}

/// Remove our CA from the login keychain. Best-effort — silent if not present.
pub async fn remove_ca_from_keychain() -> AppResult<()> {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return Ok(()),
    };
    let keychain = format!("{home}/Library/Keychains/login.keychain-db");

    let out = Command::new("/usr/bin/security")
        .args(["delete-certificate", "-c", CA_COMMON_NAME, &keychain])
        .output()
        .await;
    // Silent on failure — caller tolerates this (e.g. cert was never installed).
    if let Ok(o) = out {
        if !o.status.success() {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if !stderr.contains("could not be found") {
                // Real failure: log but don't return error so the toggle UI
                // doesn't get stuck on stale-state cleanup.
                eprintln!("remove_ca_from_keychain: {}", stderr.trim());
            }
        }
    }
    Ok(())
}

/// Best-effort check: is our CA cert present in the user's login keychain?
/// Kept for future health-check callers; the toggle path always reinstalls
/// so it doesn't consult this.
#[allow(dead_code)]
pub async fn is_ca_trusted() -> bool {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return false,
    };
    let keychain = format!("{home}/Library/Keychains/login.keychain-db");
    let out = Command::new("/usr/bin/security")
        .args(["find-certificate", "-c", CA_COMMON_NAME, &keychain])
        .output()
        .await;
    matches!(out, Ok(o) if o.status.success())
}
