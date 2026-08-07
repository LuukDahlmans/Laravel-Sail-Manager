use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::{ServiceKind, StarterKit};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub id: String,
    pub name: String,
    pub description: String,
    pub services: Vec<ServiceKind>,
    pub php_version: String,
    pub starter_kit: StarterKit,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInput {
    pub name: String,
    pub description: String,
    pub services: Vec<ServiceKind>,
    pub php_version: String,
    pub starter_kit: StarterKit,
}

pub struct TemplateStore {
    path: PathBuf,
    inner: Mutex<Vec<Template>>,
}

impl TemplateStore {
    pub fn open(path: PathBuf) -> AppResult<Self> {
        // On a corrupt or missing file, re-seed the built-in defaults rather
        // than starting from an empty list — otherwise a parse failure would
        // leave the user with zero templates (not even the seeds).
        let templates = match crate::persist::load_json::<Vec<Template>>(&path) {
            Some(t) if !t.is_empty() => t,
            _ => {
                let seeds = default_seeds();
                persist(&path, &seeds)?;
                seeds
            }
        };
        Ok(Self {
            path,
            inner: Mutex::new(templates),
        })
    }

    pub fn list(&self) -> Vec<Template> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn get(&self, id: &str) -> Option<Template> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|t| t.id == id)
            .cloned()
    }

    pub fn create(&self, input: TemplateInput) -> AppResult<Template> {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::Other("name is required".into()));
        }
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if guard.iter().any(|t| t.name.eq_ignore_ascii_case(&name)) {
            return Err(AppError::Other("name already in use".into()));
        }
        let template = Template {
            id: Uuid::new_v4().to_string(),
            name,
            description: input.description.trim().to_string(),
            services: input.services,
            php_version: input.php_version,
            starter_kit: input.starter_kit,
            created_at: Utc::now(),
        };
        guard.push(template.clone());
        persist(&self.path, &guard)?;
        Ok(template)
    }

    pub fn update(&self, id: &str, input: TemplateInput) -> AppResult<Template> {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::Other("name is required".into()));
        }
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if guard
            .iter()
            .any(|t| t.id != id && t.name.eq_ignore_ascii_case(&name))
        {
            return Err(AppError::Other("name already in use".into()));
        }
        let position = guard
            .iter()
            .position(|t| t.id == id)
            .ok_or(AppError::NotFound)?;
        let existing = &guard[position];
        let updated = Template {
            id: existing.id.clone(),
            name,
            description: input.description.trim().to_string(),
            services: input.services,
            php_version: input.php_version,
            starter_kit: input.starter_kit,
            created_at: existing.created_at,
        };
        guard[position] = updated.clone();
        persist(&self.path, &guard)?;
        Ok(updated)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let before = guard.len();
        guard.retain(|t| t.id != id);
        if guard.len() == before {
            return Err(AppError::NotFound);
        }
        persist(&self.path, &guard)?;
        Ok(())
    }

    /// Reset to the seeded defaults. Used by the "Reset application" flow.
    pub fn reset_to_seeds(&self) -> AppResult<()> {
        let seeds = default_seeds();
        persist(&self.path, &seeds)?;
        *self.inner.lock().unwrap_or_else(|e| e.into_inner()) = seeds;
        Ok(())
    }
}

fn persist(path: &Path, templates: &[Template]) -> AppResult<()> {
    let json = serde_json::to_string_pretty(templates)?;
    crate::persist::write_atomic(path, &json)?;
    Ok(())
}

fn default_seeds() -> Vec<Template> {
    let now = Utc::now();
    vec![
        Template {
            id: Uuid::new_v4().to_string(),
            name: "Plain Laravel".to_string(),
            description: "A bare Laravel install with the usual local services.".to_string(),
            services: vec![ServiceKind::Mysql, ServiceKind::Redis, ServiceKind::Mailpit],
            php_version: "8.3".to_string(),
            starter_kit: StarterKit::None,
            created_at: now,
        },
        Template {
            id: Uuid::new_v4().to_string(),
            name: "Breeze + MySQL".to_string(),
            description: "Laravel Breeze auth scaffolding on top of MySQL.".to_string(),
            services: vec![ServiceKind::Mysql, ServiceKind::Redis, ServiceKind::Mailpit],
            php_version: "8.3".to_string(),
            starter_kit: StarterKit::Breeze,
            created_at: now,
        },
        Template {
            id: Uuid::new_v4().to_string(),
            name: "API only".to_string(),
            description: "Backend-only setup — no frontend scaffolding, no mail catcher."
                .to_string(),
            services: vec![ServiceKind::Mysql, ServiceKind::Redis],
            php_version: "8.3".to_string(),
            starter_kit: StarterKit::None,
            created_at: now,
        },
    ]
}

pub fn templates_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("templates.json")
}
