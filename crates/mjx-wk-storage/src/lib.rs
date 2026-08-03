//! L4 — local and session storage, IndexedDB, cookies, and workers.
//!
//! **Phase 7.**
//!
//! # WebKit has no `Storage` domain
//!
//! Cookies live on `Page` — `getCookies`, `setCookie`, `deleteCookie`. Chrome
//! moved them to `Storage` years ago, so this is another place where CDP
//! muscle memory misleads.

use std::sync::Arc;

use async_trait::async_trait;
use mjx_wk_dialect::NormalizedFrame;
use mjx_wk_protocol::Domain;
use mjx_wk_session::{DomainAgent, SessionError, SessionHandle};

/// A local or session storage area for one origin.
#[derive(Debug, Clone)]
pub struct DomStorageArea {
    pub origin: String,
    /// False for session storage.
    pub is_local: bool,
    pub entries: Vec<(String, String)>,
}

/// One IndexedDB database.
#[derive(Debug, Clone)]
pub struct IndexedDbDatabase {
    pub name: String,
    pub version: i64,
    pub object_stores: Vec<ObjectStore>,
}

/// One object store.
#[derive(Debug, Clone)]
pub struct ObjectStore {
    pub name: String,
    pub key_path: Option<String>,
    pub auto_increment: bool,
    pub indexes: Vec<String>,
}

/// One cookie.
#[derive(Debug, Clone)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires: Option<f64>,
    pub http_only: bool,
    pub secure: bool,
    pub same_site: Option<String>,
}

/// The Application panel.
#[derive(Debug, Default)]
pub struct StorageModel {
    pub storage_areas: Vec<DomStorageArea>,
    pub databases: Vec<IndexedDbDatabase>,
    pub cookies: Vec<Cookie>,
    /// Workers attached to this page, by protocol target id.
    pub workers: Vec<(String, String)>,
}

/// Owns Domain::DomStorage, Domain::IndexedDb, Domain::Worker, Domain::ServiceWorker, Domain::Page.
#[derive(Debug, Default)]
pub struct StorageAgent {
    _private: (),
}

#[async_trait]
impl DomainAgent for StorageAgent {
    type Model = StorageModel;

    const DOMAINS: &'static [Domain] = &[
        Domain::DomStorage,
        Domain::IndexedDb,
        Domain::Worker,
        Domain::ServiceWorker,
        Domain::Page,
    ];
    const NAME: &'static str = "mjx-wk-storage";

    async fn attach(&mut self, _session: &SessionHandle) -> Result<(), SessionError> {
        todo!("Phase 7 — docs/tasks/T-701-storage-panel.md")
    }

    async fn on_event(&mut self, _event: &NormalizedFrame) -> Result<(), SessionError> {
        todo!("Phase 7 — docs/tasks/T-701-storage-panel.md")
    }

    fn snapshot(&self) -> Arc<Self::Model> {
        todo!("Phase 7 — docs/tasks/T-701-storage-panel.md")
    }
}
