use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectStatus {
    Running,
    Stopped,
    Starting,
    Stopping,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceKind {
    Mysql,
    Pgsql,
    Mariadb,
    Redis,
    Valkey,
    Memcached,
    Mailpit,
    Meilisearch,
    Typesense,
    Mongodb,
    Minio,
    Selenium,
    Soketi,
}

impl ServiceKind {
    pub fn sail_install_arg(self) -> &'static str {
        match self {
            Self::Mysql => "mysql",
            Self::Pgsql => "pgsql",
            Self::Mariadb => "mariadb",
            Self::Redis => "redis",
            Self::Valkey => "valkey",
            Self::Memcached => "memcached",
            Self::Mailpit => "mailpit",
            Self::Meilisearch => "meilisearch",
            Self::Typesense => "typesense",
            Self::Mongodb => "mongodb",
            Self::Minio => "minio",
            Self::Selenium => "selenium",
            Self::Soketi => "soketi",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortService {
    App,
    Vite,
    Mysql,
    Pgsql,
    Mariadb,
    Redis,
    Valkey,
    Memcached,
    MailpitSmtp,
    MailpitUi,
    Meilisearch,
    Typesense,
    Mongodb,
    Minio,
    MinioConsole,
    Selenium,
    Soketi,
}

impl PortService {
    pub fn label(self) -> &'static str {
        match self {
            Self::App => "Web",
            Self::Vite => "Vite",
            Self::Mysql => "MySQL",
            Self::Pgsql => "Postgres",
            Self::Mariadb => "MariaDB",
            Self::Redis => "Redis",
            Self::Valkey => "Valkey",
            Self::Memcached => "Memcached",
            Self::MailpitSmtp => "Mail SMTP",
            Self::MailpitUi => "Mailpit",
            Self::Meilisearch => "Meili",
            Self::Typesense => "Typesense",
            Self::Mongodb => "Mongo",
            Self::Minio => "MinIO",
            Self::MinioConsole => "MinIO UI",
            Self::Selenium => "Selenium",
            Self::Soketi => "Soketi",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StarterKit {
    None,
    Breeze,
    Jetstream,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Port {
    pub service: PortService,
    pub label: String,
    pub host: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub compose_project_name: String,
    pub path: String,
    pub status: ProjectStatus,
    pub starter_kit: StarterKit,
    pub php_version: String,
    pub services: Vec<ServiceKind>,
    pub ports: Vec<Port>,
    pub created_at: DateTime<Utc>,
    pub last_started: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    pub name: String,
    pub starter_kit: StarterKit,
    pub php_version: String,
    pub services: Vec<ServiceKind>,
    #[serde(default)]
    pub custom_services: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryKind {
    Created,
    Started,
    Stopped,
    Errored,
    Imported,
    Cloned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: i64,
    pub project_id: String,
    pub kind: HistoryKind,
    pub detail: Option<String>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutoCommandRunMode {
    /// Run once on start, blocking until it exits.
    Once,
    /// Run detached as a long-running process inside the laravel.test container.
    Service,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoCommand {
    pub id: String,
    pub project_id: String,
    pub label: String,
    pub command: String,
    pub run_mode: AutoCommandRunMode,
    pub enabled: bool,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoCommandInput {
    pub id: Option<String>,
    pub project_id: String,
    pub label: String,
    pub command: String,
    pub run_mode: AutoCommandRunMode,
    pub enabled: bool,
    pub sort_order: i32,
}
