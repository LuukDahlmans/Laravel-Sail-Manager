use std::path::PathBuf;
use std::sync::RwLock;

use tokio::sync::Mutex as TokioMutex;

use crate::settings::SettingsStore;
use crate::store::ProjectStore;
use crate::templates::TemplateStore;

pub struct AppState {
    pub store: ProjectStore,
    pub settings: SettingsStore,
    pub template_store: TemplateStore,
    /// Where new projects are scaffolded / cloned. Configurable at runtime via
    /// `set_projects_root`, persisted to settings so it survives restarts.
    /// Wrapped in a RwLock so the few writers don't block the many readers.
    projects_root_inner: RwLock<PathBuf>,
    pub proxy_conf_dir: PathBuf,
    pub dns_conf_dir: PathBuf,
    pub tls_dir: PathBuf,
    /// Serializes concurrent `create_project` calls so port allocation +
    /// scaffold + DB insert can't race against each other and end up
    /// allocating the same host ports to two new projects.
    pub create_lock: TokioMutex<()>,
}

impl AppState {
    pub fn new(
        store: ProjectStore,
        settings: SettingsStore,
        template_store: TemplateStore,
        projects_root: PathBuf,
        proxy_conf_dir: PathBuf,
        dns_conf_dir: PathBuf,
        tls_dir: PathBuf,
    ) -> Self {
        Self {
            store,
            settings,
            template_store,
            projects_root_inner: RwLock::new(projects_root),
            proxy_conf_dir,
            dns_conf_dir,
            tls_dir,
            create_lock: TokioMutex::new(()),
        }
    }

    /// Snapshot of the current projects root.
    pub fn projects_root(&self) -> PathBuf {
        self.projects_root_inner
            .read()
            .expect("projects_root lock poisoned")
            .clone()
    }

    pub fn set_projects_root(&self, path: PathBuf) {
        *self
            .projects_root_inner
            .write()
            .expect("projects_root lock poisoned") = path;
    }
}
