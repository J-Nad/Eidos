#[cfg(not(target_os = "windows"))]
compile_error!("Eidos is Windows-only. Build with the x86_64-pc-windows-msvc toolchain.");

use std::{
    collections::HashSet,
    fs,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::UNIX_EPOCH,
};

use rayon::iter::{ParallelBridge, ParallelIterator};
use rayon::slice::ParallelSliceMut;
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use windows::{
    core::PCWSTR,
    Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives},
};
use xxhash_rust::xxh3::Xxh3;

use crate::{
    db::{self, FileRecord, IndexStats},
    state::AppState,
};

pub const EXCLUDED_PATTERNS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".cache",
    ".next",
    ".svelte-kit",
    "node_modules",
    "target",
    "__pycache__",
    "$recycle.bin",
    "system volume information",
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "recovery",
    "msocache",
    "appdata",
    "intel",
    "perflogs",
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressPayload {
    current_dir: String,
    files_processed: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexReadyPayload {
    pub total_files: u64,
}

pub fn priority_roots() -> Vec<PathBuf> {
    let mut roots = [dirs::desktop_dir(), dirs::document_dir(), dirs::download_dir()]
        .into_iter()
        .flatten()
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    roots
}

fn modified_timestamp(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn created_timestamp(metadata: &fs::Metadata) -> i64 {
    metadata
        .created()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_else(|| modified_timestamp(metadata))
}

fn skipped_name(name: &str) -> bool {
    name.starts_with('.')
        || EXCLUDED_PATTERNS
            .iter()
            .any(|pattern| name.eq_ignore_ascii_case(pattern))
}

fn hidden_or_system(metadata: &fs::Metadata) -> bool {
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    metadata.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0
}

fn fingerprint(path: &Path, size: u64, modified: i64, is_directory: bool) -> String {
    let mut hash = Xxh3::new();
    hash.update(path.to_string_lossy().as_bytes());
    hash.update(&size.to_le_bytes());
    hash.update(&modified.to_le_bytes());
    hash.update(&[is_directory as u8]);
    format!("{:016x}", hash.digest())
}

fn record_from_metadata(path: &Path, metadata: &fs::Metadata, root: &Path) -> Option<FileRecord> {
    if hidden_or_system(metadata) {
        return None;
    }
    let is_directory = metadata.is_dir();
    if !metadata.is_file() && !is_directory {
        return None;
    }
    let filename = path.file_name()?.to_str()?.to_string();
    if skipped_name(&filename) {
        return None;
    }
    let size = if is_directory { 0 } else { metadata.len() };
    let modified = modified_timestamp(metadata);
    let created = created_timestamp(metadata);
    Some(FileRecord {
        path: path.to_string_lossy().into_owned(),
        filename,
        extension: if is_directory {
            String::new()
        } else {
            path.extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase()
        },
        size: size.min(i64::MAX as u64) as i64,
        modified_timestamp: modified,
        created_timestamp: created,
        is_directory,
        metadata: json!({
            "root": root.to_string_lossy(),
            "fingerprint": fingerprint(path, size, modified, is_directory)
        })
        .to_string(),
    })
}

pub fn local_record(path: &Path) -> Option<FileRecord> {
    let metadata = fs::symlink_metadata(path).ok()?;
    let root = path
        .components()
        .next()
        .map(|component| PathBuf::from(component.as_os_str()))
        .unwrap_or_else(|| PathBuf::from("C:\\"));
    record_from_metadata(path, &metadata, &root)
}

pub fn fixed_drive_roots() -> Vec<PathBuf> {
    let mask = unsafe { GetLogicalDrives() };
    let mut roots = Vec::new();
    for index in 0..26 {
        if mask & (1 << index) == 0 {
            continue;
        }
        let letter = (b'A' + index as u8) as char;
        let root = format!("{letter}:\\");
        let wide = root.encode_utf16().chain(std::iter::once(0)).collect::<Vec<_>>();
        let drive_type = unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) };
        if drive_type == 3 {
            roots.push(PathBuf::from(root));
        }
    }
    roots
}

fn collect_root(
    window: &WebviewWindow,
    root: &Path,
    all_roots: Arc<HashSet<PathBuf>>,
    processed: Arc<AtomicU64>,
) -> Vec<FileRecord> {
    let walk_root = root.to_path_buf();
    let filter_root = root.to_path_buf();
    let filter_roots = all_roots;
    let walker = jwalk::WalkDir::new(&walk_root)
        .skip_hidden(true)
        .follow_links(false)
        .parallelism(jwalk::Parallelism::RayonNewPool(rayon::current_num_threads()))
        .process_read_dir(move |_depth, parent, _state, children| {
            children.retain(|result| {
                let Ok(entry) = result else { return false };
                let path = parent.join(entry.file_name());
                if path != filter_root && filter_roots.contains(&path) {
                    return false;
                }
                !entry.file_name().to_str().is_some_and(skipped_name)
            });
        });

    walker
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() || entry.file_type().is_dir())
        .par_bridge()
        .filter_map(|entry| {
            let path = entry.path();
            if path == walk_root {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            let record = record_from_metadata(&path, &metadata, &walk_root)?;
            let count = processed.fetch_add(1, Ordering::Relaxed) + 1;
            if count == 1 || count % 2_500 == 0 {
                let _ = window.emit(
                    "index-progress",
                    ProgressPayload {
                        current_dir: path
                            .parent()
                            .unwrap_or(&walk_root)
                            .to_string_lossy()
                            .into_owned(),
                        files_processed: count,
                    },
                );
            }
            Some(record)
        })
        .collect()
}

pub fn collect_drives(window: &WebviewWindow, roots: &[PathBuf]) -> Result<Vec<FileRecord>, String> {
    if roots.is_empty() {
        return Err("No fixed NTFS/FAT/exFAT/ReFS drives were found.".to_string());
    }
    let root_set = Arc::new(roots.iter().cloned().collect::<HashSet<_>>());
    let processed = Arc::new(AtomicU64::new(0));
    let mut records = Vec::new();
    for root in roots {
        if root.is_dir() {
            records.extend(collect_root(
                window,
                root,
                root_set.clone(),
                processed.clone(),
            ));
        }
    }
    records.par_sort_unstable_by(|left, right| left.path.cmp(&right.path));
    records.dedup_by(|left, right| left.path == right.path);
    let _ = window.emit(
        "index-progress",
        ProgressPayload {
            current_dir: "Index complete".to_string(),
            files_processed: records.len() as u64,
        },
    );
    Ok(records)
}

pub async fn index_roots(
    app: AppHandle,
    window: WebviewWindow,
    roots: Vec<PathBuf>,
) -> Result<IndexStats, String> {
    let crawl_window = window.clone();
    let crawl_roots = roots.clone();
    let records = tauri::async_runtime::spawn_blocking(move || {
        collect_drives(&crawl_window, &crawl_roots)
    })
    .await
    .map_err(|error| error.to_string())??;

    let state = app.state::<AppState>();
    let mut connection = state.db.lock().await;
    let stats = db::sync_all(&mut connection, &records)?;
    drop(connection);

    state.cache.write().await.replace(&records);
    let _ = app.emit("index-ready", IndexReadyPayload { total_files: records.len() as u64 });
    let _ = app.emit("index-updated", &stats);
    Ok(stats)
}

pub async fn index_priority_roots(
    app: AppHandle,
    window: WebviewWindow,
    roots: Vec<PathBuf>,
) -> Result<IndexStats, String> {
    if roots.is_empty() {
        return Ok(IndexStats { files_added: 0, files_removed: 0, total_size: 0 });
    }
    let _ = window.emit("index-progress", ProgressPayload {
        current_dir: "Indexing Desktop, Documents, Downloads…".to_string(),
        files_processed: 0,
    });
    let crawl_window = window.clone();
    let records = tauri::async_runtime::spawn_blocking(move || collect_drives(&crawl_window, &roots))
        .await
        .map_err(|error| error.to_string())??;

    let state = app.state::<AppState>();
    let mut connection = state.db.lock().await;
    let stats = db::upsert_many(&mut connection, &records)?;
    let all_records = db::all_records(&connection)?;
    let total_files = all_records.len() as u64;
    drop(connection);

    state.cache.write().await.replace(&all_records);
    let _ = app.emit("index-ready", IndexReadyPayload { total_files });
    let _ = app.emit("index-updated", &stats);
    Ok(stats)
}
