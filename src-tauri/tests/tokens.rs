use framestudio::tokens::Store;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Barrier};

fn token() -> String {
    format!(
        "fs_{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
fn bearer(created: &Value) -> &str {
    created["token"].as_str().unwrap()
}
fn legacy(root: &std::path::Path, token: &str) {
    let db = Connection::open(root.join("access.sqlite3")).unwrap();
    db.execute_batch("CREATE TABLE tokens(digest BLOB PRIMARY KEY,created TEXT NOT NULL);")
        .unwrap();
    db.execute(
        "INSERT INTO tokens VALUES(?1,?2)",
        params![
            Sha256::digest(token.as_bytes()).as_slice(),
            "2026-09-01 10:11:12"
        ],
    )
    .unwrap();
}
#[test]
fn legacy_migration_preserves_authentication_dates_and_stable_identity() {
    let dir = tempfile::tempdir().unwrap();
    let raw = token();
    legacy(dir.path(), &raw);
    let store = Store::open(dir.path()).unwrap();
    let item = store.list().unwrap()[0].clone();
    assert_eq!(item["scope"], "rest");
    assert_eq!(item["name"], "Bestehender API-Zugang");
    assert_eq!(item["created_at"], "2026-09-01 10:11:12");
    assert_eq!(item["last_used_at"], Value::Null);
    assert!(!store.authorized(&raw, "mcp"));
    assert_eq!(store.list().unwrap()[0]["last_used_at"], Value::Null);
    assert!(store.authorized(&raw, "rest"));
    drop(store);
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(reopened.list().unwrap()[0]["id"], item["id"]);
    assert!(reopened.list().unwrap()[0]["last_used_at"].is_string());
    assert!(reopened.authorized(&raw, "rest"));
}
#[test]
fn scoped_grants_have_independent_revoke_and_durable_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let rest = store
        .create(
            &json!({"name":"API","recipient":"Lokales Skript","purpose":"Export","scope":"rest"}),
        )
        .unwrap();
    let mcp = store
        .create(&json!({"name":"Agent","recipient":"Lokaler Agent","scope":"mcp"}))
        .unwrap();
    let both = store.create(&json!({"scope":"both"})).unwrap();
    assert!(store.authorized(bearer(&rest), "rest"));
    assert!(!store.authorized(bearer(&rest), "mcp"));
    assert!(store.authorized(bearer(&mcp), "mcp"));
    assert!(!store.authorized(bearer(&mcp), "rest"));
    for channel in ["rest", "mcp"] {
        assert!(store.authorized(bearer(&both), channel));
    }
    assert!(!store.authorized(bearer(&both), "unknown"));
    let edited = store.update(&json!({"id":rest["access"]["id"],"name":" API neu ","recipient":"York","purpose":"';DROP TABLE tokens; --"})).unwrap();
    assert_eq!(edited["name"], "API neu");
    assert_eq!(edited["scope"], "rest");
    assert_eq!(edited["created_at"], rest["access"]["created_at"]);
    assert!(store.authorized(bearer(&rest), "rest"));
    let revoked = store.revoke(&json!({"id":rest["access"]["id"]})).unwrap();
    assert_eq!(revoked["count"], 1);
    assert!(!store.authorized(bearer(&rest), "rest"));
    assert!(store.authorized(bearer(&mcp), "mcp"));
    assert_eq!(
        store.revoke(&json!({"id":rest["access"]["id"]})).unwrap()["count"],
        0
    );
    let reopened = Store::open(dir.path()).unwrap();
    let items = reopened.list().unwrap();
    let revoked = items
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == rest["access"]["id"])
        .unwrap();
    assert_eq!(revoked["active"], false);
    assert!(revoked["revoked_at"].is_string());
    assert_eq!(revoked["recipient"], "York");
    assert_eq!(reopened.revoke(&json!({})).unwrap()["count"], 2);
    assert!(!reopened.authorized(bearer(&mcp), "mcp"));
    assert!(!reopened.authorized(bearer(&both), "rest"));
    assert_eq!(reopened.list().unwrap().as_array().unwrap().len(), 3);
}
#[test]
fn token_plaintext_is_only_returned_once_and_never_persisted_even_with_wal() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let db = Connection::open(dir.path().join("access.sqlite3")).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
    let created = store.create(&json!({"name":"Synthetic"})).unwrap();
    let raw = bearer(&created);
    assert!(store.authorized(raw, "rest"));
    let list = store.list().unwrap();
    assert!(!list.to_string().contains(raw));
    assert!(list[0].get("digest").is_none());
    assert!(list[0].get("token").is_none());
    let edited = store
        .update(&json!({"id":created["access"]["id"],"purpose":"Read-only catalog"}))
        .unwrap();
    assert!(edited.get("token").is_none());
    for file in std::fs::read_dir(dir.path()).unwrap() {
        let bytes = std::fs::read(file.unwrap().path()).unwrap();
        assert!(
            !bytes
                .windows(raw.len())
                .any(|window| window == raw.as_bytes())
        );
    }
    let stored: Vec<u8> = db
        .query_row("SELECT digest FROM tokens", [], |r| r.get(0))
        .unwrap();
    assert_eq!(stored.len(), 32);
    assert!(bool::from(subtle::ConstantTimeEq::ct_eq(
        stored.as_slice(),
        Sha256::digest(raw.as_bytes()).as_slice()
    )));
}
#[test]
fn malformed_metadata_and_scope_changes_fail_without_echoing_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let secret = token();
    for args in [
        Value::Null,
        json!([]),
        json!({"key":secret}),
        json!({"scope":"admin"}),
        json!({"scope":null}),
        json!({"name":"   "}),
        json!({"name":false}),
        json!({"name":"a".repeat(121)}),
        json!({"recipient":"a".repeat(201)}),
        json!({"purpose":"a".repeat(1001)}),
        json!({"purpose":"line\nbreak"}),
        json!({"created_at":"2020-01-01"}),
        json!({"id":uuid::Uuid::new_v4().to_string()}),
    ] {
        let error = store.create(&args).unwrap_err().to_string();
        assert!(!error.contains(&secret));
    }
    let created = store.create(&json!({})).unwrap();
    assert!(
        store
            .update(&json!({"id":created["access"]["id"],"scope":"both"}))
            .is_err()
    );
    assert!(
        store
            .update(&json!({"id":created["access"]["id"],"created_at":"2020"}))
            .is_err()
    );
    assert!(store.update(&json!({"id":"invalid"})).is_err());
    assert!(
        store
            .update(&json!({"id":uuid::Uuid::new_v4().to_string()}))
            .is_err()
    );
    assert!(store.revoke(&json!({"id":null})).is_err());
    assert!(store.revoke(&json!({"id":"invalid"})).is_err());
    assert!(store.revoke(&json!({"all":true})).is_err());
    assert!(store.authorized(bearer(&created), "rest"));
    assert_eq!(store.list().unwrap().as_array().unwrap().len(), 1);
}
#[test]
fn concurrent_legacy_migration_and_creation_preserve_all_grants() {
    let dir = tempfile::tempdir().unwrap();
    let raw = token();
    legacy(dir.path(), &raw);
    let barrier = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|index| {
            let root = dir.path().to_path_buf();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let store = Store::open(&root).unwrap();
                let created = store
                    .create(&json!({"name":format!("Client {index}")}))
                    .unwrap();
                assert!(store.authorized(bearer(&created), "rest"));
                created["access"]["id"].clone()
            })
        })
        .collect();
    let mut ids = Vec::new();
    for thread in threads {
        ids.push(thread.join().unwrap());
    }
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.list().unwrap().as_array().unwrap().len(), 9);
    for id in ids {
        assert_eq!(
            store
                .list()
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .filter(|record| record["id"] == id)
                .count(),
            1
        );
    }
    assert!(store.authorized(&raw, "rest"));
}
#[test]
fn newer_token_schema_is_not_downgraded() {
    let dir = tempfile::tempdir().unwrap();
    let db = Connection::open(dir.path().join("access.sqlite3")).unwrap();
    db.execute_batch("PRAGMA user_version=2;").unwrap();
    assert!(Store::open(dir.path()).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[cfg(unix)]
#[test]
fn symbolic_database_and_sidecar_paths_are_rejected() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let p = dir.path().join(format!("access.sqlite3{suffix}"));
        symlink(other.path().join("external"), &p).unwrap();
        assert!(Store::open(dir.path()).is_err());
        std::fs::remove_file(&p).unwrap();
    }
    assert!(!other.path().join("external").exists());
    symlink(other.path(), dir.path().join("store-link")).unwrap();
    assert!(Store::open(&dir.path().join("store-link")).is_err());
}
