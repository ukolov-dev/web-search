//! Optional, content-checked local cache. Never stores generative answers or credentials.
use anyhow::{Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const APPLICATION_ID: i64 = 0x57534331;
const MAX_ENTRY: usize = 8 * 1024 * 1024;
const MAX_PAYLOAD: i64 = 480 * 1024 * 1024;
pub const SEARCH_TTL: i64 = 900;
pub const DOCUMENT_TTL: i64 = 86400;
pub const NEWS_TTL: i64 = 300;
static CACHE: OnceLock<Cache> = OnceLock::new();
static REFRESH: OnceLock<bool> = OnceLock::new();
static HITS: AtomicU64 = AtomicU64::new(0);
static MISSES: AtomicU64 = AtomicU64::new(0);
static ERRORS: AtomicU64 = AtomicU64::new(0);
static WRITES: AtomicU64 = AtomicU64::new(0);
static HTTP: AtomicU64 = AtomicU64::new(0);
static QWEN: AtomicU64 = AtomicU64::new(0);
static EXTRACT: AtomicU64 = AtomicU64::new(0);
tokio::task_local! { static REQUEST_REFRESH: bool; }

pub async fn request_mode<F: std::future::Future>(refresh: bool, work: F) -> F::Output {
    REQUEST_REFRESH.scope(refresh, work).await
}

pub struct Cache {
    db: Mutex<Connection>,
    _lease: std::fs::File,
}
impl Drop for Cache {
    fn drop(&mut self) {
        // Release explicitly: a short-lived fork in another thread may still hold an inherited fd.
        let _ = self._lease.unlock();
    }
}
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn key(parts: &Value) -> String {
    hash(&serde_json::to_vec(parts).expect("JSON value"))
}
pub fn enabled() -> bool {
    CACHE.get().is_some()
}
pub fn refresh() -> bool {
    REFRESH.get().copied().unwrap_or(false)
        || REQUEST_REFRESH.try_with(|value| *value).unwrap_or(false)
}
pub fn http_read() {
    HTTP.fetch_add(1, Ordering::Relaxed);
}
pub fn qwen_read() {
    QWEN.fetch_add(1, Ordering::Relaxed);
}
pub fn extraction() {
    EXTRACT.fetch_add(1, Ordering::Relaxed);
}

#[derive(Clone, Copy)]
pub struct Counters([u64; 7]);
pub fn counters() -> Counters {
    Counters(
        [&HITS, &MISSES, &ERRORS, &WRITES, &HTTP, &QWEN, &EXTRACT]
            .map(|x| x.load(Ordering::Relaxed)),
    )
}
pub fn diagnostics(before: Counters) -> Value {
    let delta: Vec<_> = counters()
        .0
        .into_iter()
        .zip(before.0)
        .map(|(a, b)| a.saturating_sub(b))
        .collect();
    json!({"enabled":enabled(),"refresh":refresh(),"hits":delta[0],"misses":delta[1],"errors":delta[2],"writes":delta[3],"http_reads":delta[4],"qwen_calls":delta[5],"extractions":delta[6]})
}
fn warning(error: anyhow::Error) {
    ERRORS.fetch_add(1, Ordering::Relaxed);
    eprintln!("cache unavailable for this operation: {error}");
}

impl Cache {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .mode(0o600)
                .open(path)?;
        }
        let lease = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path.with_extension("lock"))?;
        lease
            .try_lock()
            .map_err(|_| anyhow::anyhow!("cache already in use by another process"))?;
        let db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(2))?;
        let id: i64 = db.query_row("PRAGMA application_id", [], |r| r.get(0))?;
        let tables: i64 = db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table'",
            [],
            |r| r.get(0),
        )?;
        ensure!(
            id == APPLICATION_ID || id == 0 && tables == 0,
            "not a web-search cache"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA wal_autocheckpoint=1000; PRAGMA journal_size_limit=8388608; PRAGMA max_page_count=262144;")?;
        db.pragma_update(None, "application_id", APPLICATION_ID)?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS entries(kind TEXT NOT NULL, key TEXT NOT NULL, url TEXT NOT NULL, payload BLOB NOT NULL, digest TEXT NOT NULL, fetched INTEGER NOT NULL, checked INTEGER NOT NULL, expires INTEGER NOT NULL, accessed INTEGER NOT NULL, bytes INTEGER NOT NULL, PRIMARY KEY(kind,key)); CREATE INDEX IF NOT EXISTS expiry ON entries(expires); CREATE INDEX IF NOT EXISTS lru ON entries(accessed);")?;
        Ok(Self {
            db: Mutex::new(db),
            _lease: lease,
        })
    }

    fn get(&self, kind: &str, key: &str, at: i64, stale: bool) -> Result<Option<Value>> {
        let db = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("cache lock poisoned"))?;
        let row: Option<(Vec<u8>,String,i64,i64)> = db.query_row("SELECT payload,digest,checked,expires FROM entries WHERE kind=?1 AND key=?2 AND bytes<=?3", params![kind,key,MAX_ENTRY as i64], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let Some((bytes, digest, checked, expires)) = row else {
            return Ok(None);
        };
        // A wall-clock rollback cannot turn an old body into a newly verified version.
        if checked > at || !stale && expires <= at {
            return Ok(None);
        }
        if hash(&bytes) != digest || bytes.len() > MAX_ENTRY {
            db.execute(
                "DELETE FROM entries WHERE kind=?1 AND key=?2",
                params![kind, key],
            )?;
            anyhow::bail!("cache integrity failure")
        }
        let value = serde_json::from_slice(&bytes)?;
        db.execute(
            "UPDATE entries SET accessed=?3 WHERE kind=?1 AND key=?2",
            params![kind, key, at],
        )?;
        Ok(Some(value))
    }

    fn put(
        &self,
        kind: &str,
        key: &str,
        url: &str,
        value: &Value,
        times: [i64; 3],
        limit: i64,
    ) -> Result<()> {
        let [fetched, checked, expires] = times;
        let bytes = serde_json::to_vec(value)?;
        ensure!(
            bytes.len() <= MAX_ENTRY && (bytes.len() as i64) <= limit,
            "cache entry limit"
        );
        ensure!(
            fetched <= checked && checked <= expires,
            "invalid cache timestamps"
        );
        let mut db = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("cache lock poisoned"))?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute("DELETE FROM entries WHERE expires<=?1", [checked])?;
        tx.execute(
            "DELETE FROM entries WHERE kind=?1 AND key=?2",
            params![kind, key],
        )?;
        loop {
            let (size, count): (i64, i64) = tx.query_row(
                "SELECT coalesce(sum(bytes),0),count(*) FROM entries",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if size + bytes.len() as i64 <= limit && count < 10000 {
                break;
            }
            tx.execute("DELETE FROM entries WHERE rowid=(SELECT rowid FROM entries ORDER BY accessed,rowid LIMIT 1)",[])?;
        }
        tx.execute(
            "INSERT INTO entries VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                kind,
                key,
                url,
                bytes,
                hash(&bytes),
                fetched,
                checked,
                expires,
                checked,
                bytes.len() as i64
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn clear(&self, url: Option<&str>) -> Result<usize> {
        let db = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("cache lock poisoned"))?;
        // Derived entries have content keys; clear them too so URL cleanup cannot leave stale dependants.
        let count = if let Some(url) = url {
            db.execute("DELETE FROM entries WHERE url=?1 OR kind!='html-v1'", [url])?
        } else {
            db.execute("DELETE FROM entries", [])?
        };
        db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")?;
        Ok(count)
    }

    fn remove(&self, kind: &str, key: &str) -> Result<()> {
        self.db
            .lock()
            .map_err(|_| anyhow::anyhow!("cache lock poisoned"))?
            .execute(
                "DELETE FROM entries WHERE kind=?1 AND key=?2",
                params![kind, key],
            )?;
        Ok(())
    }
}

pub fn configure(path: &Path, refresh: bool) -> Result<()> {
    CACHE
        .set(Cache::open(path)?)
        .map_err(|_| anyhow::anyhow!("cache already configured"))?;
    REFRESH
        .set(refresh)
        .map_err(|_| anyhow::anyhow!("cache mode already configured"))?;
    Ok(())
}

pub fn get(kind: &str, key: &str, stale: bool) -> Option<Value> {
    let cache = CACHE.get()?;
    match cache.get(kind, key, now(), stale) {
        Ok(Some(v)) => {
            HITS.fetch_add(1, Ordering::Relaxed);
            Some(v)
        }
        Ok(None) => {
            MISSES.fetch_add(1, Ordering::Relaxed);
            None
        }
        Err(e) => {
            warning(e);
            None
        }
    }
}
pub fn put(
    kind: &str,
    key: &str,
    url: &str,
    value: &Value,
    fetched: i64,
    checked: i64,
    expires: i64,
) {
    if let Some(cache) = CACHE.get() {
        match cache.put(
            kind,
            key,
            url,
            value,
            [fetched, checked, expires],
            MAX_PAYLOAD,
        ) {
            Ok(()) => {
                WRITES.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => warning(e),
        }
    }
}
pub async fn read(kind: &'static str, key: String, stale: bool) -> Option<Value> {
    if !enabled() {
        return None;
    }
    tokio::task::spawn_blocking(move || get(kind, &key, stale))
        .await
        .ok()
        .flatten()
}
pub async fn write(
    kind: &'static str,
    key: String,
    url: String,
    value: Value,
    fetched: i64,
    checked: i64,
    expires: i64,
) {
    if enabled() {
        let _ = tokio::task::spawn_blocking(move || {
            put(kind, &key, &url, &value, fetched, checked, expires)
        })
        .await;
    }
}
pub async fn remove(kind: &'static str, key: String) {
    if let Some(cache) = CACHE.get() {
        let _ = tokio::task::spawn_blocking(move || {
            if let Err(e) = cache.remove(kind, &key) {
                warning(e);
            }
        })
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn request_refresh_is_task_local_and_restored_after_cancellation() {
        assert!(!refresh());
        request_mode(true, async {
            assert!(refresh());
            assert!(!tokio::spawn(async { refresh() }).await.unwrap());
        })
        .await;
        assert!(!refresh());
        let result = tokio::time::timeout(
            Duration::from_millis(1),
            request_mode(true, async {
                assert!(refresh());
                std::future::pending::<()>().await;
            }),
        )
        .await;
        assert!(result.is_err());
        assert!(!refresh());
    }
    #[test]
    fn ttl_restart_integrity_clock_and_clear() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("cache.sqlite");
        let c = Cache::open(&path).unwrap();
        let v = json!({"text":"verified"});
        assert!(Cache::open(&path).is_err(), "one MCP owner per database");
        c.put(
            "extract-v1",
            "key",
            "https://example.org/a?q=1",
            &v,
            [10, 20, 30],
            1000,
        )
        .unwrap();
        drop(c);
        let c = Cache::open(&path).unwrap();
        assert_eq!(
            c.get("extract-v1", "key", 29, false).unwrap(),
            Some(v.clone())
        );
        assert!(c.get("extract-v1", "key", 30, false).unwrap().is_none());
        assert_eq!(c.get("extract-v1", "key", 30, true).unwrap(), Some(v));
        assert!(c.get("extract-v1", "key", 19, true).unwrap().is_none());
        assert!(c.get("extract-v2", "key", 29, false).unwrap().is_none());
        c.db.lock()
            .unwrap()
            .execute("UPDATE entries SET payload=x'7b7d'", [])
            .unwrap();
        assert!(c.get("extract-v1", "key", 29, false).is_err());
        assert!(c.get("extract-v1", "key", 29, false).unwrap().is_none());
        c.put(
            "html-v1",
            "a",
            "https://example.org/a?q=1",
            &json!({}),
            [10, 20, 50],
            1000,
        )
        .unwrap();
        c.put(
            "html-v1",
            "b",
            "https://example.org/a?q=2",
            &json!({}),
            [10, 20, 50],
            1000,
        )
        .unwrap();
        c.clear(Some("https://example.org/a?q=1")).unwrap();
        assert!(c.get("html-v1", "a", 21, false).unwrap().is_none());
        assert!(c.get("html-v1", "b", 21, false).unwrap().is_some());
        assert_eq!(c.clear(None).unwrap(), 1);
        assert_ne!(key(&json!(["url?q=1"])), key(&json!(["url?q=2"])));
    }
    #[test]
    fn evicts_expired_then_lru_and_rejects_foreign_db() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("c.sqlite");
        let c = Cache::open(&path).unwrap();
        let v = json!({"v":"a"});
        let n = serde_json::to_vec(&v).unwrap().len() as i64;
        c.put("test", "expired", "", &v, [1, 1, 2], n * 2).unwrap();
        c.put("test", "old", "", &v, [1, 2, 20], n * 2).unwrap();
        c.put("test", "new", "", &v, [1, 3, 20], n * 2).unwrap();
        assert!(c.get("test", "expired", 3, true).unwrap().is_none());
        c.get("test", "old", 4, false).unwrap();
        c.put("test", "third", "", &v, [1, 5, 20], n * 2).unwrap();
        assert!(c.get("test", "new", 5, false).unwrap().is_none());
        assert!(c.get("test", "old", 5, false).unwrap().is_some());
        let other = temp.path().join("foreign.sqlite");
        Connection::open(&other)
            .unwrap()
            .execute("CREATE TABLE private(x)", [])
            .unwrap();
        assert!(Cache::open(&other).is_err());
    }
}
