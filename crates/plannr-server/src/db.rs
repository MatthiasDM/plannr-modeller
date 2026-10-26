//! SQLite persistence for projects.

use std::path::Path;
use std::sync::Mutex;

use chrono::Utc;
use plannr_core::Document;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub version: u64,
    pub entity_count: usize,
}

#[derive(Debug, Clone)]
pub struct StoredProject {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub document: Document,
}

pub struct Db { conn: Mutex<Connection> }

pub fn now() -> String { Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true) }

impl Db {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(dir) = path.parent() { if !dir.as_os_str().is_empty() { std::fs::create_dir_all(dir)?; } }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, document TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
             );",
        )?;
        // public instances: every project belongs to the browser that made it
        let has_owner = conn.prepare("SELECT 1 FROM pragma_table_info('projects') WHERE name = 'owner'")?.exists([])?;
        if !has_owner { conn.execute_batch("ALTER TABLE projects ADD COLUMN owner TEXT; CREATE INDEX IF NOT EXISTS projects_owner ON projects(owner);")?; }
        Ok(Self { conn: Mutex::new(conn) })
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> { self.conn.lock().unwrap_or_else(|e| e.into_inner()) }

    pub fn list(&self) -> ApiResult<Vec<ProjectSummary>> { self.list_for(None) }

    /// The projects of one owner (public instances), or all of them.
    pub fn list_for(&self, owner: Option<&str>) -> ApiResult<Vec<ProjectSummary>> {
        let conn = self.lock();
        let sql = if owner.is_some() { "SELECT id, name, created_at, updated_at, version, document FROM projects WHERE owner = ?1 ORDER BY updated_at DESC" }
                  else { "SELECT id, name, created_at, updated_at, version, document FROM projects WHERE ?1 IS NULL OR 1 ORDER BY updated_at DESC" };
        let mut st = conn.prepare(sql)?;
        let rows = st.query_map(params![owner], |r| {
            let doc: String = r.get(5)?;
            let count = serde_json::from_str::<Document>(&doc).map(|d| d.entity_count()).unwrap_or(0);
            Ok(ProjectSummary { id: r.get(0)?, name: r.get(1)?, created_at: r.get(2)?, updated_at: r.get(3)?, version: r.get::<_, i64>(4)? as u64, entity_count: count })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn create(&self, name: &str, doc: &Document) -> ApiResult<StoredProject> {
        let id = Uuid::new_v4().to_string();
        let ts = now();
        let json = serde_json::to_string(doc)?;
        self.lock().execute(
            "INSERT INTO projects (id, name, document, version, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![id, name, json, doc.version as i64, ts],
        )?;
        Ok(StoredProject { id, name: name.to_string(), created_at: ts.clone(), updated_at: ts, document: doc.clone() })
    }

    pub fn get(&self, id: &str) -> ApiResult<StoredProject> {
        let conn = self.lock();
        let row = conn
            .query_row("SELECT id, name, created_at, updated_at, document FROM projects WHERE id = ?1", params![id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?))
            })
            .optional()?
            .ok_or_else(|| ApiError::NotFound("project not found".into()))?;
        let mut document: Document = serde_json::from_str(&row.4).map_err(|e| ApiError::Internal(anyhow::anyhow!("stored document is invalid: {e}")))?;
        document.normalize();
        Ok(StoredProject { id: row.0, name: row.1, created_at: row.2, updated_at: row.3, document })
    }

    pub fn id_by_name(&self, name: &str) -> ApiResult<Option<String>> {
        Ok(self.lock().query_row("SELECT id FROM projects WHERE name = ?1 ORDER BY updated_at DESC LIMIT 1", params![name], |r| r.get(0)).optional()?)
    }
    pub fn first_id(&self) -> ApiResult<Option<String>> {
        Ok(self.lock().query_row("SELECT id FROM projects ORDER BY updated_at DESC LIMIT 1", [], |r| r.get(0)).optional()?)
    }

    pub fn summary(&self, id: &str) -> ApiResult<ProjectSummary> {
        let p = self.get(id)?;
        Ok(ProjectSummary { id: p.id, name: p.name, created_at: p.created_at, updated_at: p.updated_at, version: p.document.version, entity_count: p.document.entity_count() })
    }

    pub fn rename(&self, id: &str, name: &str) -> ApiResult<()> {
        let n = self.lock().execute("UPDATE projects SET name = ?2, updated_at = ?3 WHERE id = ?1", params![id, name, now()])?;
        if n == 0 { return Err(ApiError::NotFound("project not found".into())); }
        Ok(())
    }

    pub fn save(&self, id: &str, doc: &Document) -> ApiResult<String> {
        let ts = now();
        let json = serde_json::to_string(doc)?;
        let n = self.lock().execute("UPDATE projects SET document = ?2, version = ?3, updated_at = ?4 WHERE id = ?1", params![id, json, doc.version as i64, ts])?;
        if n == 0 { return Err(ApiError::NotFound("project not found".into())); }
        Ok(ts)
    }

    pub fn set_owner(&self, id: &str, owner: &str) -> ApiResult<()> {
        self.lock().execute("UPDATE projects SET owner = ?2 WHERE id = ?1", params![id, owner])?;
        Ok(())
    }
    pub fn owner_of(&self, id: &str) -> ApiResult<Option<String>> {
        Ok(self.lock().query_row("SELECT owner FROM projects WHERE id = ?1", params![id], |r| r.get::<_, Option<String>>(0)).optional()?.flatten())
    }
    pub fn count_for(&self, owner: &str) -> ApiResult<usize> {
        Ok(self.lock().query_row("SELECT COUNT(*) FROM projects WHERE owner = ?1", params![owner], |r| r.get::<_, i64>(0))? as usize)
    }

    pub fn delete(&self, id: &str) -> ApiResult<()> {
        let n = self.lock().execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        if n == 0 { return Err(ApiError::NotFound("project not found".into())); }
        Ok(())
    }
}
