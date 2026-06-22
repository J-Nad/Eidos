use std::{
    collections::HashMap,
    env,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tauri::tray::TrayIcon;
use tokio::sync::{oneshot, Mutex as AsyncMutex, RwLock};

use crate::{cache::FileCache, settings::Settings};

pub const GEMINI_MODEL: &str = "gemini-3.1-flash-lite";
pub const GEMINI_STREAM_ENDPOINT: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-flash-lite:streamGenerateContent";

pub fn gemini_api_key() -> Option<String> {
    env::var("GEMINI_API_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSplitViewPayload {
    pub path: String,
    pub exists: bool,
    pub is_directory: bool,
    pub ai_available: bool,
}

pub struct AppState {
    pub db: AsyncMutex<Connection>,
    pub cache: RwLock<FileCache>,
    pub settings: RwLock<Settings>,
    pub settings_path: PathBuf,
    pub indexing: AtomicBool,
    pub gemini_api_key: RwLock<Option<String>>,
    pub abort_handles: AsyncMutex<HashMap<String, oneshot::Sender<()>>>,
    pub natural_query_cache: AsyncMutex<HashMap<String, String>>,
    pub http: reqwest::Client,
    pub pending_split_view: AsyncMutex<Option<OpenSplitViewPayload>>,
    pub tray: Mutex<Option<TrayIcon>>,
}

impl AppState {
    pub fn try_begin_indexing(&self) -> bool {
        self.indexing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub fn finish_indexing(&self) {
        self.indexing.store(false, Ordering::SeqCst);
    }
}
