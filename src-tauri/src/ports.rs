use std::net::{Ipv4Addr, Ipv6Addr, TcpListener};

use crate::error::{AppError, AppResult};
use crate::models::{Port, PortService, ServiceKind};
use crate::store::ProjectStore;

const SCAN_RANGE: u16 = 1000;

pub struct PortAllocator;

impl PortAllocator {
    /// Find a fresh free host port for a single PortService. Used by the
    /// import path to resolve collisions one port at a time without rewriting
    /// the ports the user already has working.
    pub fn allocate_single(store: &ProjectStore, ps: PortService, taken: &[u16]) -> AppResult<u16> {
        next_free(store, ps, taken)
    }

    pub fn allocate_for_services(
        store: &ProjectStore,
        services: &[ServiceKind],
    ) -> AppResult<Vec<Port>> {
        let mut needed = vec![PortService::App, PortService::Vite];
        for s in services {
            match s {
                ServiceKind::Mysql => needed.push(PortService::Mysql),
                ServiceKind::Pgsql => needed.push(PortService::Pgsql),
                ServiceKind::Mariadb => needed.push(PortService::Mariadb),
                ServiceKind::Redis => needed.push(PortService::Redis),
                ServiceKind::Valkey => needed.push(PortService::Valkey),
                ServiceKind::Memcached => needed.push(PortService::Memcached),
                ServiceKind::Mailpit => {
                    needed.push(PortService::MailpitSmtp);
                    needed.push(PortService::MailpitUi);
                }
                ServiceKind::Meilisearch => needed.push(PortService::Meilisearch),
                ServiceKind::Typesense => needed.push(PortService::Typesense),
                ServiceKind::Mongodb => needed.push(PortService::Mongodb),
                ServiceKind::Minio => {
                    needed.push(PortService::Minio);
                    needed.push(PortService::MinioConsole);
                }
                ServiceKind::Selenium => needed.push(PortService::Selenium),
                ServiceKind::Soketi => needed.push(PortService::Soketi),
            }
        }

        let mut taken: Vec<u16> = Vec::new();
        let mut ports = Vec::with_capacity(needed.len());
        for ps in needed {
            let host = next_free(store, ps, &taken)?;
            taken.push(host);
            ports.push(Port {
                service: ps,
                label: ps.label().to_string(),
                host,
            });
        }
        Ok(ports)
    }
}

fn base_for(ps: PortService) -> u16 {
    match ps {
        PortService::App => 8000,
        PortService::Vite => 5173,
        PortService::Mysql => 33060,
        PortService::Pgsql => 35432,
        PortService::Mariadb => 33063,
        PortService::Redis => 63790,
        PortService::Valkey => 63793,
        PortService::Memcached => 11211,
        PortService::MailpitSmtp => 10250,
        PortService::MailpitUi => 18025,
        PortService::Meilisearch => 17700,
        PortService::Typesense => 18108,
        PortService::Mongodb => 27017,
        PortService::Minio => 19000,
        PortService::MinioConsole => 19001,
        PortService::Selenium => 14444,
        PortService::Soketi => 16001,
    }
}

fn next_free(store: &ProjectStore, ps: PortService, taken: &[u16]) -> AppResult<u16> {
    let base = base_for(ps);
    for offset in 0..SCAN_RANGE {
        let Some(candidate) = base.checked_add(offset) else {
            break;
        };
        if taken.contains(&candidate) {
            continue;
        }
        if store.host_port_in_use(candidate)? {
            continue;
        }
        if !is_bindable(candidate) {
            continue;
        }
        return Ok(candidate);
    }
    Err(AppError::PortsExhausted)
}

fn is_bindable(port: u16) -> bool {
    // Docker maps published ports on 0.0.0.0 (IPv4 wildcard) and [::] (IPv6
    // wildcard). On macOS the docker-proxy listens on the IPv6 wildcard with
    // v4-mapped fallback, so a probe on 127.0.0.1 misses the conflict. Bind
    // both wildcards: if either fails, treat the port as taken.
    let v4_ok = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).is_ok();
    let v6_ok = TcpListener::bind((Ipv6Addr::UNSPECIFIED, port)).is_ok();
    v4_ok && v6_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::PortService;

    #[test]
    fn base_ports_are_distinct_and_match_design() {
        // Sanity-check the curated base ports the allocator starts scanning
        // from. If anyone tweaks one, this catches collisions with the others
        // and asserts the documented values.
        assert_eq!(base_for(PortService::App), 8000);
        assert_eq!(base_for(PortService::Vite), 5173);
        assert_eq!(base_for(PortService::Mysql), 33060);
        assert_eq!(base_for(PortService::Pgsql), 35432);
        assert_eq!(base_for(PortService::Mariadb), 33063);
        assert_eq!(base_for(PortService::Redis), 63790);
        assert_eq!(base_for(PortService::Valkey), 63793);
        assert_eq!(base_for(PortService::Memcached), 11211);
        assert_eq!(base_for(PortService::MailpitSmtp), 10250);
        assert_eq!(base_for(PortService::MailpitUi), 18025);
        assert_eq!(base_for(PortService::Meilisearch), 17700);
        assert_eq!(base_for(PortService::Typesense), 18108);
        assert_eq!(base_for(PortService::Mongodb), 27017);
        assert_eq!(base_for(PortService::Minio), 19000);
        assert_eq!(base_for(PortService::MinioConsole), 19001);
        assert_eq!(base_for(PortService::Selenium), 14444);
        assert_eq!(base_for(PortService::Soketi), 16001);
    }

    #[test]
    fn base_ports_have_no_duplicates() {
        let bases = [
            base_for(PortService::App),
            base_for(PortService::Vite),
            base_for(PortService::Mysql),
            base_for(PortService::Pgsql),
            base_for(PortService::Mariadb),
            base_for(PortService::Redis),
            base_for(PortService::Valkey),
            base_for(PortService::Memcached),
            base_for(PortService::MailpitSmtp),
            base_for(PortService::MailpitUi),
            base_for(PortService::Meilisearch),
            base_for(PortService::Typesense),
            base_for(PortService::Mongodb),
            base_for(PortService::Minio),
            base_for(PortService::MinioConsole),
            base_for(PortService::Selenium),
            base_for(PortService::Soketi),
        ];
        let mut sorted = bases.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), bases.len(), "duplicate port bases detected");
    }

    #[test]
    fn next_free_skips_already_taken_in_session() {
        let store = scratch_store();
        let base = base_for(PortService::App);
        // Pre-take the first three candidates (the in-session collision check
        // happens before any DB or socket probe).
        let taken = vec![base, base + 1, base + 2];
        let chosen = next_free(&store, PortService::App, &taken).expect("port available");
        assert!(
            chosen >= base + 3,
            "expected port >= {}, got {chosen}",
            base + 3
        );
        assert!(!taken.contains(&chosen));
    }

    #[test]
    fn next_free_returns_within_scan_range() {
        let store = scratch_store();
        let base = base_for(PortService::App);
        let chosen = next_free(&store, PortService::App, &[]).expect("port available");
        assert!(chosen >= base);
        assert!(chosen < base + SCAN_RANGE);
    }

    #[test]
    fn allocate_for_services_includes_app_and_vite_baseline() {
        let store = scratch_store();
        let ports = PortAllocator::allocate_for_services(&store, &[]).expect("allocation succeeds");
        // Empty services still gets App + Vite as the baseline.
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].service, PortService::App);
        assert_eq!(ports[1].service, PortService::Vite);
    }

    #[test]
    fn allocate_for_services_expands_mailpit_into_two_ports() {
        let store = scratch_store();
        let ports = PortAllocator::allocate_for_services(&store, &[ServiceKind::Mailpit])
            .expect("allocation succeeds");
        assert_eq!(ports.len(), 4);
        let services: Vec<PortService> = ports.iter().map(|p| p.service).collect();
        assert!(services.contains(&PortService::MailpitSmtp));
        assert!(services.contains(&PortService::MailpitUi));
    }

    #[test]
    fn allocate_for_services_expands_minio_into_two_ports() {
        let store = scratch_store();
        let ports = PortAllocator::allocate_for_services(&store, &[ServiceKind::Minio])
            .expect("allocation succeeds");
        assert_eq!(ports.len(), 4);
        let services: Vec<PortService> = ports.iter().map(|p| p.service).collect();
        assert!(services.contains(&PortService::Minio));
        assert!(services.contains(&PortService::MinioConsole));
    }

    #[test]
    fn allocate_for_services_returns_distinct_ports_across_session() {
        let store = scratch_store();
        // A reasonably typical project: web + db + cache + mail.
        let ports = PortAllocator::allocate_for_services(
            &store,
            &[ServiceKind::Mysql, ServiceKind::Redis, ServiceKind::Mailpit],
        )
        .expect("allocation succeeds");
        let mut hosts: Vec<u16> = ports.iter().map(|p| p.host).collect();
        let len_before = hosts.len();
        hosts.sort();
        hosts.dedup();
        assert_eq!(hosts.len(), len_before, "got duplicate host ports");
    }

    /// Build an isolated SQLite-backed store for the test. Each test gets its
    /// own file so they don't interfere.
    fn scratch_store() -> crate::store::ProjectStore {
        let dir = std::env::temp_dir();
        let id = uuid::Uuid::new_v4();
        let path = dir.join(format!("sail-manager-tests-ports-{id}.db"));
        crate::store::ProjectStore::open(&path).expect("open scratch store")
    }
}
