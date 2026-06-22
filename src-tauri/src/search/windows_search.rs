use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::BOOL,
        System::{
            Com::{
                CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
            },
            Search::{ICondition, IQueryParser, IQueryParserManager, QueryParserManager},
        },
        UI::Shell::{
            IEnumShellItems, ISearchFolderItemFactory, IShellItem, IShellItemArray,
            SearchFolderItemFactory, BHID_EnumItems, SHCreateItemFromParsingName,
            SHCreateShellItemArrayFromShellItem, SIGDN_FILESYSPATH,
        },
    },
};

use crate::db::FileEntry;

struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self, String> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|error| format!("Could not initialize Windows Search COM: {error}"))?;
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn file_entry(path: PathBuf) -> Option<FileEntry> {
    let metadata = std::fs::metadata(&path).ok()?;
    let filename = path.file_name()?.to_string_lossy().into_owned();
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let modified_timestamp = metadata
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    Some(FileEntry {
        path: path.to_string_lossy().into_owned(),
        filename,
        extension,
        size: metadata.len().min(i64::MAX as u64) as i64,
        modified_timestamp,
        created_timestamp: metadata
            .created()
            .unwrap_or(SystemTime::UNIX_EPOCH)
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64,
        is_directory: metadata.is_dir(),
        snippet: "Content match".to_string(),
        is_content_match: true,
    })
}

unsafe fn parse_condition(aqs: &str) -> Result<ICondition, String> {
    let manager: IQueryParserManager =
        CoCreateInstance(&QueryParserManager, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| format!("Windows query parser is unavailable: {error}"))?;
    let parser: IQueryParser = manager
        .CreateLoadedParser(PCWSTR(wide("SystemIndex").as_ptr()), 0x0409)
        .map_err(|error| format!("Could not load the SystemIndex query parser: {error}"))?;
    manager
        .InitializeOptions(BOOL(1), BOOL(1), &parser)
        .map_err(|error| format!("Could not configure the Windows query parser: {error}"))?;
    let solution = parser
        .Parse(PCWSTR(wide(aqs).as_ptr()), None)
        .map_err(|error| format!("Invalid Windows search query: {error}"))?;
    let mut condition = None;
    solution
        .GetQuery(Some(&mut condition), None)
        .map_err(|error| format!("Windows could not resolve the search query: {error}"))?;
    condition.ok_or_else(|| "Windows produced an empty search condition.".to_string())
}

unsafe fn search_root(
    condition: &ICondition,
    root: &Path,
    remaining: usize,
) -> Result<Vec<FileEntry>, String> {
    let root_text = root.to_string_lossy();
    let root_item: IShellItem = SHCreateItemFromParsingName(
        PCWSTR(wide(&root_text).as_ptr()),
        None,
    )
    .map_err(|error| format!("Could not create Windows Search scope for {root_text}: {error}"))?;
    let scope: IShellItemArray = SHCreateShellItemArrayFromShellItem(&root_item)
        .map_err(|error| format!("Could not scope Windows Search to {root_text}: {error}"))?;
    let factory: ISearchFolderItemFactory =
        CoCreateInstance(&SearchFolderItemFactory, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| format!("Windows Search folder factory is unavailable: {error}"))?;
    factory
        .SetScope(&scope)
        .map_err(|error| format!("Could not apply search scope {root_text}: {error}"))?;
    factory
        .SetCondition(condition)
        .map_err(|error| format!("Could not apply the Windows search condition: {error}"))?;
    let search_folder: IShellItem = factory
        .GetShellItem()
        .map_err(|error| format!("Could not create the Windows search folder: {error}"))?;
    let enumerator: IEnumShellItems = search_folder
        .BindToHandler(None, &BHID_EnumItems)
        .map_err(|error| format!("Could not enumerate Windows Search results: {error}"))?;

    let mut results = Vec::with_capacity(remaining.min(32));
    while results.len() < remaining {
        let mut item = [None];
        let mut fetched = 0;
        enumerator
            .Next(&mut item, Some(&mut fetched))
            .map_err(|error| format!("Windows Search enumeration failed: {error}"))?;
        if fetched == 0 {
            break;
        }
        let Some(item) = item[0].take() else {
            continue;
        };
        let display_name = match item.GetDisplayName(SIGDN_FILESYSPATH) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let path_text = display_name.to_string().unwrap_or_default();
        CoTaskMemFree(Some(display_name.0.cast()));
        if path_text.is_empty() {
            continue;
        }
        if let Some(entry) = file_entry(PathBuf::from(path_text)) {
            results.push(entry);
        }
    }
    Ok(results)
}

pub fn execute(aqs: &str, roots: &[PathBuf], max_results: u32) -> Result<Vec<FileEntry>, String> {
    let limit = max_results.clamp(1, 100) as usize;
    if aqs.trim().is_empty() {
        return Ok(Vec::new());
    }
    let _com = ComApartment::initialize()?;
    let condition = unsafe { parse_condition(aqs) }?;
    let mut results = Vec::with_capacity(limit);
    let mut seen = HashSet::with_capacity(limit);
    for root in roots {
        if results.len() >= limit {
            break;
        }
        let entries = unsafe { search_root(&condition, root, limit - results.len()) }?;
        for entry in entries {
            let key = entry.path.to_ascii_lowercase();
            if seen.insert(key) {
                results.push(entry);
            }
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    #[test]
    fn system_index_accepts_canonical_aqs() {
        let result = super::execute(
            "System.FileExtension:.pdf",
            &[PathBuf::from("C:\\")],
            1,
        );
        assert!(result.is_ok(), "{}", result.unwrap_err());
    }
}
