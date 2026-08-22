//! Cos72 — a SECOND domain OS, present to prove the boundary (ME-4).
//!
//! ME-1 built the kernel↔domain-OS seam and ME-2 made which OS runs a config
//! decision. Both were argued for on the strength of a single module, and a
//! boundary with one thing on the far side of it is an assertion, not a fact. So
//! this crate exists to falsify it: if anything in the kernel still assumes
//! "the domain OS" is singular, mounting a second one alongside Sin90 is what
//! surfaces it.
//!
//! **It is a SKELETON, and that word is doing real work.** What is here is
//! exactly enough to be a domain OS the kernel can mount, store data for, route
//! to and relay events from — one table, three routes, its own database. What is
//! NOT here is a community-OS domain model. Cos72 is MushroomDAO's product;
//! inventing its schema to make this file look substantial would be guessing at
//! someone else's design and then shipping the guess. The generic `entries`
//! table is a placeholder whose only job is to be data that MUST NOT appear in
//! Sin90's store.
//!
//! The structure below is deliberately the same shape as `agent24-sin90-os`:
//! embedded manifest, `MANIFEST_NAME`/`MANIFEST_VERSION` constants so the kernel
//! can name it without constructing it, `StorageMode` as a constructor choice,
//! `OnceLock` store, relative routes. That sameness IS the deliverable — two
//! modules written against the same contract, with no kernel branch between them.

use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, OnceLock};

use agent24_domain::http::{error_response, module_unavailable, read_body_or_response};
use agent24_domain::{DomainError, DomainModule, DomainOsManifest, KernelCtx, Result};
use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path as AxPath, State};
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

const MANIFEST: &str = include_str!("../domain-os.yml");

/// Identity WITHOUT construction — see `agent24_sin90_os` for why the kernel
/// needs this: a module that fails to construct must still have a name for
/// `agent24 os disable` to act on. `identity_matches_the_manifest` pins these
/// against the parsed manifest.
pub const MANIFEST_NAME: &str = "cos72";
pub const MANIFEST_VERSION: &str = "0.1.0";

#[derive(Debug, thiserror::Error)]
pub enum Cos72Error {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("not found: {0}")]
    NotFound(String),
}

/// Where this module's data lives. Same constructor-choice shape as Sin90's, for
/// the same reason: the trait takes only a path, and widening it with a mode flag
/// would push every module's storage policy into the shared contract.
#[derive(Debug, Clone)]
pub enum StorageMode {
    /// `cos72.db` inside the kernel-assigned directory.
    Persistent,
    /// In-memory, for `--ephemeral` daemons and tests.
    Memory,
}

pub struct Cos72Module {
    manifest: DomainOsManifest,
    mode: StorageMode,
    store: OnceLock<Cos72Store>,
}

impl Cos72Module {
    pub fn new(mode: StorageMode) -> Result<Self> {
        Ok(Self {
            manifest: DomainOsManifest::from_yaml(MANIFEST)?,
            mode,
            store: OnceLock::new(),
        })
    }
}

#[async_trait::async_trait]
impl DomainModule for Cos72Module {
    fn manifest(&self) -> &DomainOsManifest {
        &self.manifest
    }

    async fn open_store(&self, dir: &Path) -> Result<()> {
        let store = match &self.mode {
            StorageMode::Memory => Cos72Store::open_memory().await,
            // `dir` is the directory the KERNEL assigned, derived from this
            // module's validated name. Cos72 never spells `~/.agent24/os/cos72`
            // itself — that is what stops it from ever pointing at Sin90's data,
            // and it is checked by `two_modules_never_share_a_store` in the
            // kernel's mounter tests.
            StorageMode::Persistent => Cos72Store::open(&dir.join("cos72.db")).await,
        }
        .map_err(|e| DomainError::Store(e.to_string()))?;
        self.store
            .set(store)
            .map_err(|_| DomainError::Store("open_store called more than once".into()))
    }

    fn routes(&self, ctx: Arc<dyn KernelCtx>) -> axum::Router {
        let Some(store) = self.store.get() else {
            // Unreachable through the mounter; a panic here would kill the daemon
            // over one module.
            return axum::Router::new().fallback(|| async { module_unavailable(MANIFEST_NAME) });
        };
        axum::Router::new()
            .route("/entries", post(create_entry).get(list_entries))
            .route("/entries/{id}", get(get_entry))
            .with_state(Cos72State {
                store: store.clone(),
                ctx,
            })
    }
}

#[derive(Clone)]
struct Cos72State {
    store: Cos72Store,
    ctx: Arc<dyn KernelCtx>,
}

impl Cos72State {
    fn emit(&self, kind: &str, payload: serde_json::Value) {
        let Some(sink) = self.ctx.events() else {
            return;
        };
        let serde_json::Value::Object(map) = payload else {
            debug_assert!(false, "cos72 event payload must be an object");
            return;
        };
        if let Err(e) = sink.emit(kind, map) {
            debug_assert!(false, "cos72 emitted an invalid event kind: {e}");
        }
    }
}

// ---- store ----------------------------------------------------------------

/// Cos72's OWN database. Deliberately not shared with Sin90 and not the kernel's:
/// the isolation ME-4 exists to demonstrate is physical, not a convention.
#[derive(Clone)]
pub struct Cos72Store {
    pool: SqlitePool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub id: String,
    pub text: String,
    pub created_at: String,
}

impl Cos72Store {
    pub async fn open(path: &Path) -> std::result::Result<Self, Cos72Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                Cos72Error::Sqlx(sqlx::Error::Io(std::io::Error::other(e.to_string())))
            })?;
        }
        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5))
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn open_memory() -> std::result::Result<Self, Cos72Error> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?
            .busy_timeout(std::time::Duration::from_secs(5))
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn create(&self, text: &str) -> std::result::Result<Entry, Cos72Error> {
        let entry = Entry {
            id: agent24_core::util::ulid(),
            text: text.to_owned(),
            created_at: agent24_core::util::now_iso8601(),
        };
        sqlx::query("INSERT INTO cos72_entries (id, text, created_at) VALUES (?, ?, ?)")
            .bind(&entry.id)
            .bind(&entry.text)
            .bind(&entry.created_at)
            .execute(&self.pool)
            .await?;
        Ok(entry)
    }

    pub async fn list(&self) -> std::result::Result<Vec<Entry>, Cos72Error> {
        // `id` as the tie-break, not `created_at` alone: ULIDs are monotonic and
        // second-resolution timestamps are not, so ordering by time alone leaves
        // same-second rows in an undefined order (the hazard `improvement/`
        // records for five `agent24-store` queries).
        let rows = sqlx::query(
            "SELECT id, text, created_at FROM cos72_entries ORDER BY created_at DESC, id DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| Entry {
                id: r.get("id"),
                text: r.get("text"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    pub async fn get(&self, id: &str) -> std::result::Result<Entry, Cos72Error> {
        let row = sqlx::query("SELECT id, text, created_at FROM cos72_entries WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| Cos72Error::NotFound(id.to_owned()))?;
        Ok(Entry {
            id: row.get("id"),
            text: row.get("text"),
            created_at: row.get("created_at"),
        })
    }
}

// ---- handlers -------------------------------------------------------------

fn map_err(err: Cos72Error) -> Response {
    match err {
        Cos72Error::NotFound(id) => error_response(
            StatusCode::NOT_FOUND,
            "not_found",
            &format!("no entry {id}"),
        ),
        Cos72Error::Sqlx(_) | Cos72Error::Migrate(_) => {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal", "store error")
        }
    }
}

#[allow(clippy::result_large_err)]
fn parse<T: for<'de> Deserialize<'de>>(bytes: &Bytes) -> std::result::Result<T, Response> {
    serde_json::from_slice(bytes).map_err(|e| {
        error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            &format!("invalid entry: {e}"),
        )
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewEntry {
    text: String,
}

async fn create_entry(State(state): State<Cos72State>, req: Request<Body>) -> Response {
    let bytes = match read_body_or_response(req).await {
        Ok(b) => b,
        Err(r) => return r,
    };
    let body: NewEntry = match parse(&bytes) {
        Ok(b) => b,
        Err(r) => return r,
    };
    match state.store.create(&body.text).await {
        Ok(e) => {
            state.emit("entry.created", serde_json::json!({ "id": e.id }));
            (StatusCode::CREATED, Json(e)).into_response()
        }
        Err(e) => map_err(e),
    }
}

async fn list_entries(State(state): State<Cos72State>) -> Response {
    match state.store.list().await {
        Ok(v) => Json(serde_json::json!({ "entries": v })).into_response(),
        Err(e) => map_err(e),
    }
}

async fn get_entry(State(state): State<Cos72State>, AxPath(id): AxPath<String>) -> Response {
    match state.store.get(&id).await {
        Ok(e) => Json(e).into_response(),
        Err(e) => map_err(e),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn identity_matches_the_manifest() {
        // The kernel's catalogue uses the CONSTANTS; everything downstream uses the
        // parsed manifest. If they disagreed, `agent24 os enable cos72` would write
        // an entry for a name the mounter never sees.
        let m = DomainOsManifest::from_yaml(MANIFEST).expect("the compiled-in manifest is valid");
        assert_eq!(m.name(), MANIFEST_NAME);
        assert_eq!(m.version(), MANIFEST_VERSION);
        assert_eq!(m.route_namespace(), "/api/v1/cos72");
        assert_eq!(m.event_module(), "cos72");
    }

    #[tokio::test]
    async fn the_store_round_trips_and_orders_deterministically() {
        let s = Cos72Store::open_memory().await.unwrap();
        let a = s.create("first").await.unwrap();
        let b = s.create("second").await.unwrap();
        assert_eq!(s.get(&a.id).await.unwrap(), a);
        // Both rows almost certainly share a second, so this is exactly the
        // same-second case that an ORDER BY on the timestamp alone leaves
        // undefined.
        let listed = s.list().await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(
            listed[0].id, b.id,
            "newest first, broken by ulid not chance"
        );
        assert!(matches!(s.get("nope").await, Err(Cos72Error::NotFound(_))));
    }
}
