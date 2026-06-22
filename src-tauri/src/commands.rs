use std::{
    io::Write,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use atomicwrites::{AllowOverwrite, AtomicFile};
use base64::{engine::general_purpose, Engine as _};
use chrono::{Local, TimeZone};
use regex::Regex;
use std::sync::LazyLock;
use rusqlite::{params_from_iter, types::Value};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_notification::NotificationExt;
use tokio::time::{timeout, Duration};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::HWND,
        UI::{
            Shell::ShellExecuteW,
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    },
};

use crate::{
    ai,
    cache::entry_from_path,
    db::{self, FileEntry, FileMetadata, IndexStats},
    indexer,
    settings::{self, Settings},
    search::windows_search,
    search::metadata::{kind_matches, parse_metadata_from_query, FileKind, MetadataFilters},
    state::{AppState, OpenSplitViewPayload},
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderEntry {
    path: String,
    filename: String,
    extension: String,
    size: i64,
    modified_timestamp: i64,
    is_directory: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveEntry {
    path: String,
    label: String,
}

fn validate_aqs(aqs: &str) -> Result<(), String> {
    let trimmed = aqs.trim();
    if trimmed.is_empty() || trimmed.len() > 4096 {
        return Err("Windows search query is empty or too long.".to_string());
    }
    if trimmed.contains(';') || trimmed.contains("--") || trimmed.contains('\0') {
        return Err("Windows search query contains unsupported characters.".to_string());
    }
    Ok(())
}

static QUOTED_CONTENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""([^"]+)""#).expect("quoted content regex")
});
static MARKED_CONTENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(?:containing|with\s+the\s+phrase|inside)\b\s*["']?(.+?)["']?\s*$"#)
        .expect("marked content regex")
});
static CONTENT_METADATA_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s+(?:modified|created|larger|bigger|greater|more|smaller|less|over|under|at\s+least|at\s+most|between|size)\b.*$")
        .expect("content metadata suffix regex")
});

fn extract_content_phrase(query: &str) -> Option<String> {
    let raw = QUOTED_CONTENT
        .captures(query)
        .and_then(|capture| capture.get(1))
        .or_else(|| MARKED_CONTENT.captures(query).and_then(|capture| capture.get(1)))?
        .as_str()
        .trim()
        .trim_matches(['"', '\''])
        .trim();
    let phrase = CONTENT_METADATA_SUFFIX.replace(raw, "");
    let phrase = phrase.trim();
    (!phrase.is_empty()).then(|| phrase.chars().take(512).collect())
}

fn aqs_date(timestamp: i64) -> Option<String> {
    Local.timestamp_opt(timestamp, 0).single().map(|value| value.format("%Y-%m-%d").to_string())
}

fn filters_to_aqs(filters: &MetadataFilters) -> Vec<String> {
    let mut conditions = Vec::new();
    if !filters.extensions.is_empty() {
        let extensions = filters.extensions.iter()
            .map(|extension| format!("System.FileExtension:.{}", extension.trim_start_matches('.')))
            .collect::<Vec<_>>();
        conditions.push(if extensions.len() == 1 { extensions[0].clone() } else { format!("({})", extensions.join(" OR ")) });
    }
    if let Some(kind) = &filters.kind {
        let value = match kind {
            FileKind::Document => "document", FileKind::Spreadsheet => "spreadsheet",
            FileKind::Image => "picture", FileKind::Video => "video", FileKind::Audio => "music",
            FileKind::Folder => "folder", FileKind::Code | FileKind::Archive => "document",
        };
        conditions.push(format!("System.Kind:={value}"));
    }
    if let Some(value) = filters.size_min { conditions.push(format!("System.Size:>={value}")); }
    if let Some(value) = filters.size_max { conditions.push(format!("System.Size:<={value}")); }
    for (property, operator, timestamp) in [
        ("System.DateModified", ">=", filters.modified_after),
        ("System.DateModified", "<=", filters.modified_before),
        ("System.DateCreated", ">=", filters.created_after),
        ("System.DateCreated", "<=", filters.created_before),
    ] {
        if let Some(date) = timestamp.and_then(aqs_date) {
            conditions.push(format!("{property}:{operator}{date}"));
        }
    }
    conditions
}

fn metadata_matches(entry: &FileEntry, filters: &MetadataFilters) -> bool {
    if !filters.extensions.is_empty()
        && !filters
            .extensions
            .iter()
            .any(|extension| entry.extension.eq_ignore_ascii_case(extension.trim_start_matches('.')))
    {
        return false;
    }
    if let Some(kind) = &filters.kind {
        if !kind_matches(kind, &entry.extension, entry.is_directory) {
            return false;
        }
    }
    if let Some(minimum) = filters.size_min {
        if entry.is_directory || (entry.size.max(0) as u64) < minimum {
            return false;
        }
    }
    if let Some(maximum) = filters.size_max {
        if entry.is_directory || entry.size.max(0) as u64 > maximum {
            return false;
        }
    }
    if let Some(after) = filters.modified_after {
        if entry.modified_timestamp < after {
            return false;
        }
    }
    if let Some(before) = filters.modified_before {
        if entry.modified_timestamp > before {
            return false;
        }
    }
    if let Some(after) = filters.created_after {
        if entry.created_timestamp > 0 && entry.created_timestamp < after {
            return false;
        }
    }
    if let Some(before) = filters.created_before {
        if entry.created_timestamp > 0 && entry.created_timestamp > before {
            return false;
        }
    }
    true
}

fn score_name(entry: &FileEntry, query: &str) -> u8 {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return 10;
    }
    let filename = entry.filename.to_ascii_lowercase();
    if filename == query {
        0
    } else if filename.starts_with(&query) {
        1
    } else if filename.contains(&query) {
        2
    } else if entry
        .path
        .to_ascii_lowercase()
        .split('\\')
        .any(|part| part == query || part.starts_with(&query))
    {
        3
    } else {
        9
    }
}

fn sql_kind_extensions(kind: &FileKind) -> Option<&'static [&'static str]> {
    match kind {
        FileKind::Document => Some(&["pdf", "doc", "docx", "txt", "md", "rtf"]),
        FileKind::Spreadsheet => Some(&["xls", "xlsx", "csv"]),
        FileKind::Image => Some(&["png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "heic"]),
        FileKind::Video => Some(&["mp4", "mov", "avi", "mkv", "webm", "m4v"]),
        FileKind::Audio => Some(&["mp3", "wav", "flac", "aac", "ogg"]),
        FileKind::Code => Some(&["py", "js", "ts", "rs", "go", "java", "html", "css", "json", "xml", "yaml", "yml", "toml"]),
        FileKind::Archive => Some(&["zip", "rar", "7z"]),
        FileKind::Folder => None,
    }
}

fn append_in_clause(sql: &mut String, values: &mut Vec<Value>, column: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    sql.push_str(" AND ");
    sql.push_str(column);
    sql.push_str(" IN (");
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            sql.push(',');
        }
        sql.push('?');
        values.push(Value::Text(item.trim_start_matches('.').to_ascii_lowercase()));
    }
    sql.push(')');
}

fn query_metadata_sql(
    connection: &rusqlite::Connection,
    name_query: &str,
    filters: &MetadataFilters,
    limit: usize,
) -> Result<Vec<FileEntry>, String> {
    let mut sql = String::from(
        "SELECT path, filename, extension, size, modified_timestamp, created_timestamp, is_directory
         FROM files WHERE 1=1",
    );
    let mut values = Vec::<Value>::new();

    let name_query = name_query.trim();
    if !name_query.is_empty() {
        sql.push_str(" AND (filename LIKE ? OR path LIKE ?)");
        let pattern = format!("%{}%", name_query.replace('%', "\\%").replace('_', "\\_"));
        values.push(Value::Text(pattern.clone()));
        values.push(Value::Text(pattern));
    }

    append_in_clause(&mut sql, &mut values, "extension", &filters.extensions);
    if let Some(kind) = &filters.kind {
        if matches!(kind, FileKind::Folder) {
            sql.push_str(" AND is_directory=1");
        } else if let Some(kind_extensions) = sql_kind_extensions(kind) {
            let extensions = kind_extensions.iter().map(|value| value.to_string()).collect::<Vec<_>>();
            append_in_clause(&mut sql, &mut values, "extension", &extensions);
            sql.push_str(" AND is_directory=0");
        }
    }
    if let Some(size_min) = filters.size_min {
        sql.push_str(" AND is_directory=0 AND size>=?");
        values.push(Value::Integer(size_min.min(i64::MAX as u64) as i64));
    }
    if let Some(size_max) = filters.size_max {
        sql.push_str(" AND is_directory=0 AND size<=?");
        values.push(Value::Integer(size_max.min(i64::MAX as u64) as i64));
    }
    if let Some(after) = filters.modified_after {
        sql.push_str(" AND modified_timestamp>=?");
        values.push(Value::Integer(after));
    }
    if let Some(before) = filters.modified_before {
        sql.push_str(" AND modified_timestamp<=?");
        values.push(Value::Integer(before));
    }
    if let Some(after) = filters.created_after {
        sql.push_str(" AND created_timestamp>=?");
        values.push(Value::Integer(after));
    }
    if let Some(before) = filters.created_before {
        sql.push_str(" AND created_timestamp<=?");
        values.push(Value::Integer(before));
    }
    sql.push_str(" ORDER BY is_directory DESC, modified_timestamp DESC, filename ASC LIMIT ?");
    values.push(Value::Integer(limit.min(500) as i64));

    let mut statement = connection.prepare_cached(&sql).map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params_from_iter(values), |row| {
            Ok(FileEntry {
                path: row.get(0)?,
                filename: row.get(1)?,
                extension: row.get(2)?,
                size: row.get(3)?,
                modified_timestamp: row.get(4)?,
                created_timestamp: row.get(5)?,
                is_directory: row.get::<_, i64>(6)? != 0,
                snippet: String::new(),
                is_content_match: false,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

async fn run_windows_search(
    state: &State<'_, AppState>,
    aqs: String,
    max_results: u32,
) -> Result<Vec<FileEntry>, String> {
    validate_aqs(&aqs)?;
    let roots = selected_drive_roots(state).await;
    if roots.is_empty() {
        return Ok(Vec::new());
    }
    let worker = tauri::async_runtime::spawn_blocking(move || {
        windows_search::execute(&aqs, &roots, max_results)
    });
    timeout(Duration::from_millis(1500), worker)
        .await
        .map_err(|_| "Windows Search took too long. Try a simpler query.".to_string())?
        .map_err(|error| format!("Windows Search worker failed: {error}"))?
}

#[tauri::command]
pub async fn execute_windows_search(
    state: State<'_, AppState>,
    aqs: String,
    max_results: u32,
) -> Result<Vec<FileEntry>, String> {
    run_windows_search(&state, aqs, max_results).await
}

#[tauri::command]
pub async fn search_content_phrase(
    state: State<'_, AppState>,
    query: String,
    filters: Option<MetadataFilters>,
) -> Result<Vec<FileEntry>, String> {
    let Some(phrase) = extract_content_phrase(&query) else {
        return Ok(Vec::new());
    };
    let escaped = phrase.replace('"', "\"\"");
    let mut conditions = vec![format!(r#"System.Search.Contents:"{escaped}""#)];
    if let Some(filters) = filters.map(MetadataFilters::normalized) {
        conditions.extend(filters_to_aqs(&filters));
    }
    run_windows_search(&state, conditions.join(" AND "), 30).await
}

fn looks_like_deep_search(query: &str) -> bool {
    query.contains('*') || query.contains('"') || query.contains(':') || query.contains(" AND ")
}

fn safe_filename(input: &str) -> String {
    let raw = Path::new(input.trim())
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("untitled.txt");
    let mut safe = raw
        .chars()
        .map(|character| {
            if r#"<>:"/\|?*"#.contains(character) {
                '-'
            } else {
                character
            }
        })
        .collect::<String>();
    safe = safe.split_whitespace().collect::<Vec<_>>().join("-");
    if safe.is_empty() {
        safe = "untitled.txt".to_string();
    }
    if Path::new(&safe).extension().is_none() {
        safe.push_str(".txt");
    }
    safe
}

fn default_parent_dir() -> PathBuf {
    dirs::document_dir()
        .or_else(dirs::desktop_dir)
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("C:\\"))
}

async fn selected_drive_roots(state: &State<'_, AppState>) -> Vec<PathBuf> {
    state
        .settings
        .read()
        .await
        .selected_drives
        .iter()
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .collect()
}

async fn normalize_creation_path(state: &State<'_, AppState>, input: &str, extension: Option<&str>) -> PathBuf {
    let requested = PathBuf::from(input.trim());
    let mut target = if requested.has_root() && requested.file_name().is_some() {
        requested
    } else {
        let default_dir = state
            .settings
            .read()
            .await
            .default_save_location
            .trim()
            .to_string();
        let parent = if default_dir.is_empty() {
            default_parent_dir()
        } else {
            PathBuf::from(default_dir)
        };
        parent.join(safe_filename(input))
    };
    if let Some(extension) = extension.map(|value| value.trim().trim_start_matches('.')).filter(|value| !value.is_empty()) {
        target.set_extension(extension);
    }
    target
}

fn minimal_template(extension: &str) -> &'static str {
    match extension.trim_start_matches('.').to_ascii_lowercase().as_str() {
        "py" => "def main():\n    pass\n\n\nif __name__ == \"__main__\":\n    main()\n",
        "js" | "ts" => "function main() {\n  // TODO: implement\n}\n\nmain();\n",
        "rs" => "fn main() {\n    // TODO: implement\n}\n",
        "go" => "package main\n\nfunc main() {\n\t// TODO: implement\n}\n",
        "json" => "{}\n",
        "md" => "# New Document\n",
        "csv" => "Name,Value\n",
        "html" => "<!doctype html>\n<html>\n<head><meta charset=\"utf-8\"><title>Untitled</title></head>\n<body></body>\n</html>\n",
        _ => "",
    }
}

fn metadata_from_path(path: &Path) -> Result<FileMetadata, String> {
    let record = indexer::local_record(path)
        .ok_or_else(|| format!("Could not read metadata for {}.", path.display()))?;
    Ok(FileMetadata {
        path: record.path,
        filename: record.filename,
        extension: record.extension,
        size: record.size,
        modified_timestamp: record.modified_timestamp,
        created_timestamp: record.created_timestamp,
        is_directory: record.is_directory,
    })
}

fn open_split_payload(path: &Path, ai_available: bool) -> OpenSplitViewPayload {
    let exists = path.exists();
    let is_directory = path.is_dir();
    OpenSplitViewPayload {
        path: path.to_string_lossy().into_owned(),
        exists,
        is_directory,
        ai_available,
    }
}

#[tauri::command]
pub async fn search_files(
    state: State<'_, AppState>,
    app: AppHandle,
    query: String,
) -> Result<Vec<FileEntry>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    {
        let cache = state.cache.read().await;
        if !cache.is_empty() && !looks_like_deep_search(trimmed) {
            return Ok(cache.search(trimmed, 20));
        }
    }

    if state.cache.read().await.is_empty() && state.try_begin_indexing() {
        let index_app = app.clone();
        tauri::async_runtime::spawn(async move {
            let Some(window) = index_app.get_webview_window("spotlight") else {
                index_app.state::<AppState>().finish_indexing();
                return;
            };
            let roots = index_app
                .state::<AppState>()
                .settings
                .read()
                .await
                .selected_drives
                .iter()
                .map(PathBuf::from)
                .filter(|path| path.is_dir())
                .collect::<Vec<_>>();
            let result = indexer::index_roots(index_app.clone(), window, roots).await;
            index_app.state::<AppState>().finish_indexing();
            if let Err(error) = result {
                log::warn!("On-demand selected-drive index failed: {error}");
            }
        });
        return Ok(Vec::new());
    }

    let connection = state.db.lock().await;
    db::search_fts(&connection, trimmed)
}

#[tauri::command]
pub async fn search_with_filters(
    state: State<'_, AppState>,
    app: AppHandle,
    query: String,
    filters: Option<MetadataFilters>,
) -> Result<Vec<FileEntry>, String> {
    let trimmed = query.trim();
    let filters = filters.map(MetadataFilters::normalized);
    if filters.as_ref().is_none_or(MetadataFilters::is_empty) {
        return search_files(state, app, query).await;
    }
    let filters = filters.expect("checked above");
    let search_text = filters
        .name_query
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(trimmed);

    if state.cache.read().await.is_empty() && state.try_begin_indexing() {
        let index_app = app.clone();
        tauri::async_runtime::spawn(async move {
            let Some(window) = index_app.get_webview_window("spotlight") else {
                index_app.state::<AppState>().finish_indexing();
                return;
            };
            let roots = index_app
                .state::<AppState>()
                .settings
                .read()
                .await
                .selected_drives
                .iter()
                .map(PathBuf::from)
                .filter(|path| path.is_dir())
                .collect::<Vec<_>>();
            let result = indexer::index_roots(index_app.clone(), window, roots).await;
            index_app.state::<AppState>().finish_indexing();
            if let Err(error) = result {
                log::warn!("On-demand selected-drive index failed: {error}");
            }
        });
        return Ok(Vec::new());
    }

    if !search_text.is_empty() {
        let cache = state.cache.read().await;
        let mut results = cache
            .search(search_text, 500)
            .into_iter()
            .filter(|entry| metadata_matches(entry, &filters))
            .collect::<Vec<_>>();
        results.sort_by(|left, right| {
            score_name(left, search_text)
                .cmp(&score_name(right, search_text))
                .then_with(|| right.is_directory.cmp(&left.is_directory))
                .then_with(|| right.modified_timestamp.cmp(&left.modified_timestamp))
                .then_with(|| left.filename.cmp(&right.filename))
        });
        results.truncate(20);
        return Ok(results);
    }

    let connection = state.db.lock().await;
    query_metadata_sql(&connection, "", &filters, 30)
}

#[tauri::command]
pub async fn parse_natural_language(
    state: State<'_, AppState>,
    query: String,
) -> Result<MetadataFilters, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(MetadataFilters {
            extensions: Vec::new(),
            kind: None,
            size_min: None,
            size_max: None,
            modified_after: None,
            modified_before: None,
            created_after: None,
            created_before: None,
            name_query: None,
            has_content_intent: false,
        });
    }
    let fallback = || MetadataFilters {
        extensions: Vec::new(),
        kind: None,
        size_min: None,
        size_max: None,
        modified_after: None,
        modified_before: None,
        created_after: None,
        created_before: None,
        name_query: Some(trimmed.chars().take(256).collect::<String>()),
        has_content_intent: false,
    };
    if trimmed.len() > 1024 {
        return Ok(fallback().normalized());
    }
    if let Some(filters) = parse_metadata_from_query(trimmed) {
        return Ok(filters);
    }
    let key = format!(
        "metadata:{}",
        trimmed.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
    );
    if let Some(cached) = state.natural_query_cache.lock().await.get(&key).cloned() {
        if let Ok(filters) = serde_json::from_str::<MetadataFilters>(&cached) {
            return Ok(filters.normalized());
        }
    }
    if state.gemini_api_key.read().await.is_none() {
        return Ok(fallback().normalized());
    }
    let today = Local::now().format("%Y-%m-%d").to_string();
    let parsed = match timeout(
        Duration::from_millis(1200),
        ai::translate_metadata_filters(&state, trimmed, &today),
    )
    .await
    {
        Ok(Ok(filters)) => filters,
        Ok(Err(error)) => {
            log::debug!("AI metadata parsing failed; falling back to filename search. {error}");
            return Ok(fallback().normalized());
        }
        Err(_) => {
            log::debug!("AI metadata parsing timed out after 1200ms; falling back to filename search.");
            return Ok(fallback().normalized());
        }
    };
    let parsed = parsed.normalized();
    if parsed.is_empty() {
        return Ok(fallback().normalized());
    }
    let mut cache = state.natural_query_cache.lock().await;
    if cache.len() >= 500 {
        cache.clear();
    }
    cache.insert(
        key,
        serde_json::to_string(&parsed).map_err(|error| error.to_string())?,
    );
    Ok(parsed)
}

#[tauri::command]
pub async fn index_drives(
    state: State<'_, AppState>,
    app: AppHandle,
    window: WebviewWindow,
) -> Result<IndexStats, String> {
    if !state.try_begin_indexing() {
        return Err("Indexing is already running.".to_string());
    }
    let roots = selected_drive_roots(&state).await;
    let result = indexer::index_roots(app.clone(), window, roots).await;
    state.finish_indexing();
    result
}

#[tauri::command]
pub async fn open_file(path: String) -> Result<(), String> {
    let target = PathBuf::from(path.trim());
    if !target.exists() {
        return Err("File not found".to_string());
    }
    let canonical = target
        .canonicalize()
        .map_err(|_| "File not found".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let operation = "open".encode_utf16().chain(std::iter::once(0)).collect::<Vec<_>>();
        let file = canonical
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let result = unsafe {
            ShellExecuteW(
                HWND(std::ptr::null_mut()),
                PCWSTR(operation.as_ptr()),
                PCWSTR(file.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        if result.0 as isize <= 32 {
            return Err(format!(
                "Could not open {}. Windows ShellExecute failed with code {}.",
                canonical.display(),
                result.0 as isize
            ));
        }
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn open_file_or_folder(path: String) -> Result<(), String> {
    open_file(path).await
}

#[tauri::command]
pub async fn open_split_view(
    state: State<'_, AppState>,
    app: AppHandle,
    path: String,
) -> Result<(), String> {
    let path = PathBuf::from(path);
    let payload = open_split_payload(&path, state.gemini_api_key.read().await.is_some());
    *state.pending_split_view.lock().await = Some(payload.clone());
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "The Split View window is unavailable.".to_string())?;
    main.show().map_err(|error| error.to_string())?;
    main.maximize().map_err(|error| error.to_string())?;
    main.set_focus().map_err(|error| error.to_string())?;
    if let Some(spotlight) = app.get_webview_window("spotlight") {
        let _ = spotlight.hide();
    }
    app.emit_to("main", "open-split-view", payload)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_file_void(
    state: State<'_, AppState>,
    app: AppHandle,
    _window: WebviewWindow,
    desired_path: String,
) -> Result<(), String> {
    create_file(
        state,
        app,
        desired_path,
        Option::<String>::None,
        Option::<String>::None,
    )
    .await
}

#[tauri::command]
pub async fn create_file(
    state: State<'_, AppState>,
    app: AppHandle,
    path: String,
    extension: Option<String>,
    content: Option<String>,
) -> Result<(), String> {
    let target = normalize_creation_path(&state, &path, extension.as_deref()).await;
    if target.exists() {
        return Err("A file or folder with that name already exists.".to_string());
    }
    if target.file_name().is_none() {
        return Err("Choose a valid file name.".to_string());
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("Could not create target folder: {error}"))?;
    }

    let extension = target
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    let initial_content = content.unwrap_or_else(|| minimal_template(&extension).to_string());
    let bytes = initial_content.into_bytes();
    let write_target = target.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AtomicFile::new(&write_target, AllowOverwrite)
            .write(|file| file.write_all(&bytes))
            .map_err(|error| format!("Could not create file: {error}"))
    })
    .await
    .map_err(|error| error.to_string())??;

    let record = indexer::local_record(&target)
        .ok_or_else(|| "The file was created but metadata could not be read.".to_string())?;
    let mut connection = state.db.lock().await;
    db::upsert_one(&mut connection, &record)?;
    drop(connection);
    state.cache.write().await.upsert(record);

    open_split_view(state, app, target.to_string_lossy().into_owned()).await
}

#[tauri::command]
pub async fn get_pending_split_view(
    state: State<'_, AppState>,
) -> Result<Option<OpenSplitViewPayload>, String> {
    Ok(state.pending_split_view.lock().await.clone())
}

#[tauri::command]
pub async fn ai_available(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.gemini_api_key.read().await.is_some())
}

#[tauri::command]
pub async fn read_file_content(path: String) -> Result<String, String> {
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|error| format!("Could not read file metadata: {error}"))?;
    if metadata.is_dir() {
        return Err("Folders do not have text content.".to_string());
    }
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|error| format!("Could not read file: {error}"))?;
    let max = 1_048_576_usize;
    let truncated = bytes.len() > max;
    let slice = if truncated { &bytes[..max] } else { &bytes };
    let text = String::from_utf8_lossy(slice).to_string();
    if truncated {
        Ok(format!("{text}\n\n[Eidos note: file is larger than 1 MB; preview truncated.]"))
    } else {
        Ok(text)
    }
}

#[tauri::command]
pub async fn file_data_url(path: String) -> Result<String, String> {
    let extension = Path::new(&path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mime = match extension.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "m4v" => "video/mp4",
        _ => "application/octet-stream",
    };
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|error| format!("Could not read binary file: {error}"))?;
    Ok(format!(
        "data:{mime};base64,{}",
        general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
pub async fn list_folder(path: String) -> Result<Vec<FolderEntry>, String> {
    let mut entries = Vec::new();
    let mut reader = tokio::fs::read_dir(&path)
        .await
        .map_err(|error| format!("Could not list folder: {error}"))?;
    while let Some(entry) = reader
        .next_entry()
        .await
        .map_err(|error| format!("Could not read folder entry: {error}"))?
    {
        let item_path = entry.path();
        if let Ok(metadata) = entry.metadata().await {
            let filename = item_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string();
            entries.push(FolderEntry {
                extension: if metadata.is_dir() {
                    String::new()
                } else {
                    item_path
                        .extension()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                },
                path: item_path.to_string_lossy().into_owned(),
                filename,
                size: if metadata.is_dir() { 0 } else { metadata.len().min(i64::MAX as u64) as i64 },
                modified_timestamp: metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs() as i64)
                    .unwrap_or_default(),
                is_directory: metadata.is_dir(),
            });
        }
    }
    entries.sort_by(|left, right| {
        right
            .is_directory
            .cmp(&left.is_directory)
            .then_with(|| left.filename.to_ascii_lowercase().cmp(&right.filename.to_ascii_lowercase()))
    });
    Ok(entries)
}

#[tauri::command]
pub async fn get_file_metadata(
    state: State<'_, AppState>,
    path: String,
) -> Result<FileMetadata, String> {
    let connection = state.db.lock().await;
    if let Some(metadata) = db::file_metadata(&connection, &path)? {
        return Ok(metadata);
    }
    drop(connection);
    metadata_from_path(Path::new(&path))
}

#[tauri::command]
pub async fn write_file(
    state: State<'_, AppState>,
    app: AppHandle,
    path: String,
    content: String,
) -> Result<(), String> {
    let target = PathBuf::from(&path);
    if target.file_name().is_none() {
        return Err("Choose a valid target filename.".to_string());
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("Could not create target folder: {error}"))?;
    }
    let bytes = content.into_bytes();
    let write_target = target.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AtomicFile::new(&write_target, AllowOverwrite)
            .write(|file| file.write_all(&bytes))
            .map_err(|error| format!("Atomic write failed: {error}"))
    })
    .await
    .map_err(|error| error.to_string())??;

    let record = indexer::local_record(&target)
        .ok_or_else(|| "The file was written but metadata could not be read.".to_string())?;
    let mut connection = state.db.lock().await;
    db::upsert_one(&mut connection, &record)?;
    drop(connection);
    state.cache.write().await.upsert(record);

    let _ = app
        .notification()
        .builder()
        .title("Eidos")
        .body(format!(
            "{} was saved.",
            target
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("File")
        ))
        .show();
    app.emit("file-written", serde_json::json!({ "path": target }))
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn ai_chat_message(
    state: State<'_, AppState>,
    window: WebviewWindow,
    file_path: Option<String>,
    message: String,
    mode: Option<String>,
    context: Option<String>,
) -> Result<(), String> {
    let file_content = if let Some(path) = &file_path {
        read_file_content(path.clone()).await.ok()
    } else {
        None
    };
    let instruction_path = file_path.clone();
    let repository_instructions = tauri::async_runtime::spawn_blocking(move || {
        let Some(path) = instruction_path else { return String::new() };
        let mut directory = PathBuf::from(path);
        if !directory.is_dir() { directory.pop(); }
        let mut sections = Vec::new();
        for _ in 0..12 {
            for name in ["AGENTS.md", "CLAUDE.md", ".cursorrules"] {
                let candidate = directory.join(name);
                if let Ok(mut text) = std::fs::read_to_string(&candidate) {
                    text.truncate(64 * 1024);
                    sections.push(format!("REPOSITORY INSTRUCTIONS ({name}):\n{text}"));
                }
            }
            if !directory.pop() { break; }
        }
        sections.join("\n\n")
    }).await.map_err(|error| error.to_string())?;
    let mut combined_context = context.unwrap_or_default();
    if !repository_instructions.is_empty() {
        if !combined_context.is_empty() { combined_context.push_str("\n\n---\n\n"); }
        combined_context.push_str(&repository_instructions);
    }
    combined_context = combined_context.chars().take(192 * 1024).collect();
    ai::stream_chat(
        state, window, file_path, file_content, message, mode,
        (!combined_context.is_empty()).then_some(combined_context),
    ).await
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings.read().await.clone())
}

#[tauri::command]
pub fn available_drives() -> Vec<DriveEntry> {
    indexer::fixed_drive_roots()
        .into_iter()
        .map(|path| {
            let text = path.to_string_lossy().into_owned();
            DriveEntry {
                label: text.trim_end_matches('\\').to_string(),
                path: text,
            }
        })
        .collect()
}

#[tauri::command]
pub async fn set_settings(
    state: State<'_, AppState>,
    app: AppHandle,
    window: WebviewWindow,
    settings: Settings,
) -> Result<(), String> {
    let previous = state.settings.read().await.clone();
    let mut normalized = settings.clone();
    normalized.selected_drives = normalized
        .selected_drives
        .iter()
        .map(|drive| settings::normalize_drive(drive))
        .collect();
    settings::save_settings(&state.settings_path, &normalized)?;
    settings::set_autostart(normalized.autostart, &std::env::current_exe().map_err(|error| error.to_string())?)?;
    *state.settings.write().await = normalized.clone();
    *state.gemini_api_key.write().await = if normalized.gemini_api_key.trim().is_empty() {
        crate::state::gemini_api_key()
    } else {
        Some(normalized.gemini_api_key.trim().to_string())
    };
    if previous.selected_drives != normalized.selected_drives {
        if state.try_begin_indexing() {
            let roots = selected_drive_roots(&state).await;
            let result = indexer::index_roots(app, window, roots).await;
            state.finish_indexing();
            result?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn show_settings(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("settings")
        .ok_or_else(|| "The Settings window is unavailable.".to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn abort_ai(state: State<'_, AppState>, stream_id: Option<String>) -> Result<(), String> {
    let mut handles = state.abort_handles.lock().await;
    if let Some(stream_id) = stream_id {
        if let Some(sender) = handles.remove(&stream_id) {
            let _ = sender.send(());
        }
    } else {
        for (_, sender) in handles.drain() {
            let _ = sender.send(());
        }
    }
    Ok(())
}

#[tauri::command]
pub fn return_to_spotlight(app: AppHandle) -> Result<(), String> {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
    let spotlight = app
        .get_webview_window("spotlight")
        .ok_or_else(|| "The Eidos command bar is unavailable.".to_string())?;
    spotlight.show().map_err(|error| error.to_string())?;
    spotlight.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn hide_spotlight(app: AppHandle) -> Result<(), String> {
    if let Some(spotlight) = app.get_webview_window("spotlight") {
        spotlight.hide().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn rebuild_search_index(state: State<'_, AppState>) -> Result<(), String> {
    let connection = state.db.lock().await;
    db::rebuild_fts(&connection)
}

#[tauri::command]
pub async fn cache_size(state: State<'_, AppState>) -> Result<usize, String> {
    Ok(state.cache.read().await.len())
}

#[tauri::command]
pub async fn refresh_file_in_index(
    state: State<'_, AppState>,
    path: String,
) -> Result<Option<FileEntry>, String> {
    let path = PathBuf::from(path);
    let Some(entry) = entry_from_path(&path) else {
        let connection = state.db.lock().await;
        db::delete_path(&connection, &path)?;
        drop(connection);
        state.cache.write().await.remove(&path.to_string_lossy());
        return Ok(None);
    };
    if let Some(record) = indexer::local_record(&path) {
        let mut connection = state.db.lock().await;
        db::upsert_one(&mut connection, &record)?;
        state.cache.write().await.upsert(record);
    }
    Ok(Some(entry))
}
