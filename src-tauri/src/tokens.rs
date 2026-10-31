//! Local access grants: only random-token digests and user-supplied metadata persist.
use anyhow::{Context, Result, ensure};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use subtle::ConstantTimeEq;

pub struct Store {
    path: PathBuf,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        ensure!(
            root.is_dir() && !std::fs::symlink_metadata(root)?.file_type().is_symlink(),
            "Unsicheres Zugriffsverzeichnis"
        );
        let store = Self {
            path: root.canonicalize()?.join("access.sqlite3"),
        };
        let mut db = store.connect()?;
        migrate(&mut db)?;
        Ok(store)
    }
    fn connect(&self) -> Result<Connection> {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let path = PathBuf::from(format!("{}{suffix}", self.path.display()));
            ensure!(
                !std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()),
                "Unsicherer Tokenpfad"
            );
        }
        let db = Connection::open(&self.path)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch("PRAGMA synchronous=FULL;")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(db)
    }
    pub fn create(&self, args: &Value) -> Result<Value> {
        fields(args, &["name", "recipient", "purpose", "scope"])?;
        let name = label(args, "name", "Lokaler API-Zugang", 120)?;
        ensure!(!name.is_empty(), "Zugangsname fehlt");
        let recipient = label(args, "recipient", "", 200)?;
        let purpose = label(args, "purpose", "", 1000)?;
        let scope = label(args, "scope", "rest", 8)?;
        ensure!(
            ["rest", "mcp", "both"].contains(&scope.as_str()),
            "Ungültiger Zugangsbereich"
        );
        let token = format!(
            "fs_{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let digest = Sha256::digest(token.as_bytes());
        let id = uuid::Uuid::new_v4().to_string();
        let db = self.connect()?;
        db.execute("INSERT INTO tokens(digest,created,id,name,recipient,purpose,scope) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![digest.as_slice(),now(),id,name,recipient,purpose,scope])?;
        Ok(
            json!({"token":token,"access":read_access(&db,&id)?,"notice":"Token nur jetzt sichtbar. Danach bleiben Name, Empfänger und Verlauf verfügbar; der Token kann einzeln widerrufen werden."}),
        )
    }
    pub fn list(&self) -> Result<Value> {
        let db = self.connect()?;
        let mut query = db.prepare(&format!(
            "SELECT {PUBLIC_FIELDS} FROM tokens ORDER BY created DESC,id"
        ))?;
        let rows = query.query_map([], access)?;
        Ok(json!(rows.collect::<rusqlite::Result<Vec<_>>>()?))
    }
    pub fn update(&self, args: &Value) -> Result<Value> {
        fields(args, &["id", "name", "recipient", "purpose"])?;
        let id = access_id(args)?;
        let mut db = self.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous = read_access(&tx, &id)?;
        let name = label(args, "name", previous["name"].as_str().unwrap_or(""), 120)?;
        ensure!(!name.is_empty(), "Zugangsname fehlt");
        let recipient = label(
            args,
            "recipient",
            previous["recipient"].as_str().unwrap_or(""),
            200,
        )?;
        let purpose = label(
            args,
            "purpose",
            previous["purpose"].as_str().unwrap_or(""),
            1000,
        )?;
        tx.execute(
            "UPDATE tokens SET name=?1,recipient=?2,purpose=?3 WHERE id=?4",
            params![name, recipient, purpose, id],
        )?;
        let updated = read_access(&tx, &id)?;
        tx.commit()?;
        Ok(updated)
    }
    pub fn revoke(&self, args: &Value) -> Result<Value> {
        fields(args, &["id"])?;
        let mut db = self.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count = if args.get("id").is_some() {
            let id = access_id(args)?;
            read_access(&tx, &id)?;
            tx.execute(
                "UPDATE tokens SET revoked_at=?1 WHERE id=?2 AND revoked_at IS NULL",
                params![now(), id],
            )?
        } else {
            tx.execute(
                "UPDATE tokens SET revoked_at=?1 WHERE revoked_at IS NULL",
                [now()],
            )?
        };
        tx.commit()?;
        Ok(json!({"revoked":true,"count":count}))
    }
    pub fn authorized(&self, token: &str, channel: &str) -> bool {
        if token.len() != 67 || !token.starts_with("fs_") || !["rest", "mcp"].contains(&channel) {
            return false;
        }
        self.authenticated_access(token, channel)
            .is_ok_and(|access| access.is_some())
    }
    pub fn authenticated_access(&self, token: &str, channel: &str) -> Result<Option<Value>> {
        if token.len() != 67 || !token.starts_with("fs_") || !["rest", "mcp"].contains(&channel) {
            return Ok(None);
        }
        let digest = Sha256::digest(token.as_bytes());
        let db = self.connect()?;
        let mut query = db.prepare(
            "SELECT id,digest FROM tokens WHERE revoked_at IS NULL AND (scope=?1 OR scope='both')",
        )?;
        let rows = query.query_map([channel], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        let mut matched = None;
        for row in rows {
            let (id, stored) = row?;
            if bool::from(stored.ct_eq(digest.as_slice())) {
                matched = Some(id);
            }
        }
        drop(query);
        let Some(id) = matched else {
            return Ok(None);
        };
        // Repeat the active/scope predicate: a concurrent revoke must not be undone.
        if db.execute("UPDATE tokens SET last_used_at=?1 WHERE id=?2 AND revoked_at IS NULL AND (scope=?3 OR scope='both')",params![now(),id,channel])? != 1 { return Ok(None); }
        Ok(Some(read_access(&db, &id)?))
    }
}
const PUBLIC_FIELDS: &str = "id,name,recipient,purpose,scope,created,last_used_at,revoked_at";
fn access(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let revoked: Option<String> = row.get(7)?;
    Ok(
        json!({"id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"recipient":row.get::<_,String>(2)?,"purpose":row.get::<_,String>(3)?,"scope":row.get::<_,String>(4)?,"created_at":row.get::<_,String>(5)?,"last_used_at":row.get::<_,Option<String>>(6)?,"active":revoked.is_none(),"revoked_at":revoked}),
    )
}
fn read_access(db: &Connection, id: &str) -> Result<Value> {
    db.query_row(
        &format!("SELECT {PUBLIC_FIELDS} FROM tokens WHERE id=?1"),
        [id],
        access,
    )
    .optional()?
    .context("Zugang wurde nicht gefunden")
}
fn now() -> String {
    Utc::now().to_rfc3339()
}
fn fields(value: &Value, allowed: &[&str]) -> Result<()> {
    let fields = value
        .as_object()
        .context("Zugangsangaben müssen ein Objekt sein")?;
    ensure!(
        fields.keys().all(|field| allowed.contains(&field.as_str())),
        "Unbekanntes Feld in Zugangsangaben"
    );
    Ok(())
}
fn label(value: &Value, name: &str, fallback: &str, limit: usize) -> Result<String> {
    let text = match value.get(name) {
        Some(value) => value.as_str().context("Zugangsangaben müssen Text sein")?,
        None => fallback,
    };
    ensure!(
        text.len() <= limit && !text.chars().any(char::is_control),
        "Zugangsangabe ist zu lang oder enthält Steuerzeichen"
    );
    Ok(text.trim().to_owned())
}
fn access_id(args: &Value) -> Result<String> {
    let id = label(args, "id", "", 36)?;
    uuid::Uuid::parse_str(&id).context("Ungültige Zugangs-ID")?;
    Ok(id)
}
fn migrate(db: &mut Connection) -> Result<()> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version: i64 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    ensure!(
        version <= 1,
        "Zugangsdatenbank stammt aus einer neueren App-Version"
    );
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS tokens(digest BLOB PRIMARY KEY,created TEXT NOT NULL);",
    )?;
    let columns = {
        let mut query = tx.prepare("PRAGMA table_info(tokens)")?;
        query
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (column, definition) in [
        ("id", "TEXT"),
        ("name", "TEXT NOT NULL DEFAULT 'Bestehender API-Zugang'"),
        ("recipient", "TEXT NOT NULL DEFAULT ''"),
        ("purpose", "TEXT NOT NULL DEFAULT ''"),
        ("scope", "TEXT NOT NULL DEFAULT 'rest'"),
        ("last_used_at", "TEXT"),
        ("revoked_at", "TEXT"),
    ] {
        if !columns.iter().any(|name| name == column) {
            tx.execute_batch(&format!(
                "ALTER TABLE tokens ADD COLUMN {column} {definition}"
            ))?;
        }
    }
    let old_rows = {
        let mut query = tx.prepare("SELECT rowid FROM tokens WHERE id IS NULL")?;
        query
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for row in old_rows {
        tx.execute(
            "UPDATE tokens SET id=?1 WHERE rowid=?2",
            params![uuid::Uuid::new_v4().to_string(), row],
        )?;
    }
    tx.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS tokens_id ON tokens(id); PRAGMA user_version=1;",
    )?;
    tx.commit()?;
    Ok(())
}
