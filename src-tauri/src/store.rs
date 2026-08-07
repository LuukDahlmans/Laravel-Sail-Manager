use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{AppError, AppResult};
use crate::models::{
    AutoCommand, AutoCommandRunMode, HistoryEntry, HistoryKind, Port, PortService, Project,
    ProjectStatus, ServiceKind, StarterKit,
};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    compose_project_name TEXT NOT NULL,
    path TEXT NOT NULL,
    status TEXT NOT NULL,
    starter_kit TEXT NOT NULL,
    php_version TEXT NOT NULL,
    services TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_started TEXT
);

CREATE TABLE IF NOT EXISTS ports (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    service TEXT NOT NULL,
    label TEXT NOT NULL,
    host INTEGER NOT NULL,
    PRIMARY KEY (project_id, service)
);

CREATE INDEX IF NOT EXISTS idx_ports_host ON ports(host);

CREATE TABLE IF NOT EXISTS project_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    detail TEXT,
    at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_history_project ON project_history(project_id, at DESC);

CREATE TABLE IF NOT EXISTS auto_commands (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    command TEXT NOT NULL,
    run_mode TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_autocmd_project ON auto_commands(project_id, sort_order);
"#;

pub struct ProjectStore {
    conn: Mutex<Connection>,
}

impl ProjectStore {
    pub fn open<P: AsRef<Path>>(db_path: P) -> AppResult<Self> {
        if let Some(parent) = db_path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn list(&self) -> AppResult<Vec<Project>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, name, compose_project_name, path, status, starter_kit,
                    php_version, services, created_at, last_started
             FROM projects ORDER BY created_at DESC",
        )?;
        let rows = stmt
            .query_map([], project_from_row)?
            .collect::<Result<Vec<_>, _>>()?;

        let mut projects = Vec::with_capacity(rows.len());
        for mut p in rows {
            p.ports = load_ports(&conn, &p.id)?;
            projects.push(p);
        }
        Ok(projects)
    }

    pub fn get(&self, id: &str) -> AppResult<Project> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, name, compose_project_name, path, status, starter_kit,
                    php_version, services, created_at, last_started
             FROM projects WHERE id = ?1",
        )?;
        let mut project = stmt
            .query_row(params![id], project_from_row)
            .optional()?
            .ok_or(AppError::NotFound)?;
        project.ports = load_ports(&conn, &project.id)?;
        Ok(project)
    }

    pub fn name_exists(&self, name: &str) -> AppResult<bool> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM projects WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn host_port_in_use(&self, host: u16) -> AppResult<bool> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ports WHERE host = ?1",
            params![host as i64],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn insert(&self, project: &Project) -> AppResult<()> {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO projects (id, name, compose_project_name, path, status,
                                   starter_kit, php_version, services, created_at, last_started)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                project.id,
                project.name,
                project.compose_project_name,
                project.path,
                serde_json::to_string(&project.status)?,
                serde_json::to_string(&project.starter_kit)?,
                project.php_version,
                serde_json::to_string(&project.services)?,
                project.created_at.to_rfc3339(),
                project.last_started.as_ref().map(|d| d.to_rfc3339()),
            ],
        )?;
        for port in &project.ports {
            tx.execute(
                "INSERT INTO ports (project_id, service, label, host)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    project.id,
                    serde_json::to_string(&port.service)?,
                    port.label,
                    port.host as i64,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn update_status(&self, id: &str, status: ProjectStatus) -> AppResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let n = conn.execute(
            "UPDATE projects SET status = ?1 WHERE id = ?2",
            params![serde_json::to_string(&status)?, id],
        )?;
        if n == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    pub fn touch_last_started(&self, id: &str, when: DateTime<Utc>) -> AppResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "UPDATE projects SET last_started = ?1 WHERE id = ?2",
            params![when.to_rfc3339(), id],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let n = conn.execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    pub fn add_history(
        &self,
        project_id: &str,
        kind: HistoryKind,
        detail: Option<&str>,
    ) -> AppResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO project_history (project_id, kind, detail, at) VALUES (?1, ?2, ?3, ?4)",
            params![
                project_id,
                serde_json::to_string(&kind)?.trim_matches('"').to_string(),
                detail,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_history(&self, project_id: &str, limit: u32) -> AppResult<Vec<HistoryEntry>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, project_id, kind, detail, at
             FROM project_history WHERE project_id = ?1 ORDER BY at DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![project_id, limit as i64], |row| {
                let id: i64 = row.get(0)?;
                let project_id: String = row.get(1)?;
                let kind_s: String = row.get(2)?;
                let detail: Option<String> = row.get(3)?;
                let at_s: String = row.get(4)?;
                let kind: HistoryKind =
                    serde_json::from_str(&format!("\"{kind_s}\"")).map_err(serde_err)?;
                let at = DateTime::parse_from_rfc3339(&at_s)
                    .map_err(|e| {
                        serde_err(serde_json::Error::io(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            e.to_string(),
                        )))
                    })?
                    .with_timezone(&Utc);
                Ok(HistoryEntry {
                    id,
                    project_id,
                    kind,
                    detail,
                    at,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn list_auto_commands(&self, project_id: &str) -> AppResult<Vec<AutoCommand>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, project_id, label, command, run_mode, enabled, sort_order
             FROM auto_commands WHERE project_id = ?1 ORDER BY sort_order ASC",
        )?;
        let rows = stmt
            .query_map(params![project_id], |row| {
                let id: String = row.get(0)?;
                let project_id: String = row.get(1)?;
                let label: String = row.get(2)?;
                let command: String = row.get(3)?;
                let run_mode_s: String = row.get(4)?;
                let enabled_i: i64 = row.get(5)?;
                let sort_order: i64 = row.get(6)?;
                let run_mode: AutoCommandRunMode =
                    serde_json::from_str(&format!("\"{run_mode_s}\"")).map_err(serde_err)?;
                Ok(AutoCommand {
                    id,
                    project_id,
                    label,
                    command,
                    run_mode,
                    enabled: enabled_i != 0,
                    sort_order: sort_order as i32,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn upsert_auto_command(&self, cmd: &AutoCommand) -> AppResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let run_mode_s = serde_json::to_string(&cmd.run_mode)?
            .trim_matches('"')
            .to_string();
        conn.execute(
            "INSERT INTO auto_commands (id, project_id, label, command, run_mode, enabled, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                label = excluded.label,
                command = excluded.command,
                run_mode = excluded.run_mode,
                enabled = excluded.enabled,
                sort_order = excluded.sort_order",
            params![
                cmd.id,
                cmd.project_id,
                cmd.label,
                cmd.command,
                run_mode_s,
                if cmd.enabled { 1_i64 } else { 0_i64 },
                cmd.sort_order as i64,
            ],
        )?;
        Ok(())
    }

    pub fn delete_auto_command(&self, id: &str) -> AppResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM auto_commands WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Wipe every row from every table. Used by the "Reset application" flow.
    /// Project folders on disk are NOT touched — only the app's state.
    pub fn clear_all(&self) -> AppResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute_batch(
            "DELETE FROM project_history;
             DELETE FROM auto_commands;
             DELETE FROM ports;
             DELETE FROM projects;",
        )?;
        Ok(())
    }
}

fn project_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    let id: String = row.get("id")?;
    let name: String = row.get("name")?;
    let compose_project_name: String = row.get("compose_project_name")?;
    let path: String = row.get("path")?;
    let status_s: String = row.get("status")?;
    let starter_kit_s: String = row.get("starter_kit")?;
    let php_version: String = row.get("php_version")?;
    let services_s: String = row.get("services")?;
    let created_at_s: String = row.get("created_at")?;
    let last_started_s: Option<String> = row.get("last_started")?;

    let status: ProjectStatus = serde_json::from_str(&status_s).map_err(serde_err)?;
    let starter_kit: StarterKit = serde_json::from_str(&starter_kit_s).map_err(serde_err)?;
    let services: Vec<ServiceKind> = serde_json::from_str(&services_s).map_err(serde_err)?;

    let created_at = DateTime::parse_from_rfc3339(&created_at_s)
        .map_err(|e| {
            serde_err(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            )))
        })?
        .with_timezone(&Utc);

    let last_started = last_started_s
        .map(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| {
                    serde_err(serde_json::Error::io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        e.to_string(),
                    )))
                })
        })
        .transpose()?;

    Ok(Project {
        id,
        name,
        compose_project_name,
        path,
        status,
        starter_kit,
        php_version,
        services,
        ports: Vec::new(),
        created_at,
        last_started,
    })
}

fn load_ports(conn: &Connection, project_id: &str) -> AppResult<Vec<Port>> {
    let mut stmt =
        conn.prepare("SELECT service, label, host FROM ports WHERE project_id = ?1 ORDER BY host")?;
    let ports = stmt
        .query_map(params![project_id], |row| {
            let service_s: String = row.get(0)?;
            let label: String = row.get(1)?;
            let host: i64 = row.get(2)?;
            let service: PortService = serde_json::from_str(&service_s).map_err(serde_err)?;
            Ok(Port {
                service,
                label,
                host: host as u16,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ports)
}

fn serde_err(e: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
}
