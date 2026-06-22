use std::{
    collections::HashSet,
    ffi::c_void,
    mem::{offset_of, size_of, ManuallyDrop},
    path::PathBuf,
    ptr::{null, null_mut},
    time::{SystemTime, UNIX_EPOCH},
};

use windows::{
    core::{Interface, PCWSTR, PWSTR},
    Win32::System::{
        Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED},
        Search::{
            IAccessor, ICommand, ICommandText, IDataInitialize, IDBCreateCommand, IDBCreateSession,
            IDBInitialize, IRowset, ISearchManager, CSearchManager, DBACCESSOR_ROWDATA,
            DBBINDING, DBMEMOWNER_CLIENTOWNED, DBPARAMIO_NOTPARAM, DBPART_LENGTH,
            DBPART_STATUS, DBPART_VALUE, DBSTATUS_S_OK, DBTYPE_WSTR, HACCESSOR,
            MSDAINITIALIZE,
        },
    },
};

use crate::db::FileEntry;

const DBGUID_DEFAULT: windows::core::GUID = windows::core::GUID::from_u128(0xc8b521fb_5cf3_11ce_ade5_00aa0044773d);
const PATH_CAPACITY: usize = 32_768;

#[repr(C)]
struct PathRow {
    status: i32,
    length: usize,
    value: [u16; PATH_CAPACITY],
}

impl Default for PathRow {
    fn default() -> Self {
        Self { status: 0, length: 0, value: [0; PATH_CAPACITY] }
    }
}

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
    fn drop(&mut self) { unsafe { CoUninitialize() }; }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn take_wide(value: PWSTR) -> String {
    let text = value.to_string().unwrap_or_default();
    CoTaskMemFree(Some(value.0.cast()));
    text
}

fn file_entry(path: PathBuf) -> Option<FileEntry> {
    let metadata = std::fs::metadata(&path).ok()?;
    let filename = path.file_name()?.to_string_lossy().into_owned();
    let extension = path.extension().map(|value| value.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let modified_timestamp = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
    let created_timestamp = metadata.created().unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
    Some(FileEntry {
        path: path.to_string_lossy().into_owned(), filename, extension,
        size: metadata.len().min(i64::MAX as u64) as i64,
        modified_timestamp, created_timestamp, is_directory: metadata.is_dir(),
        snippet: "Content match".to_string(), is_content_match: true,
    })
}

fn scope_restriction(roots: &[PathBuf]) -> String {
    let scopes = roots.iter().map(|root| {
        let value = root.to_string_lossy().replace('\\', "\\\\").replace('\'', "''");
        format!("SCOPE='file:{value}'")
    }).collect::<Vec<_>>();
    if scopes.is_empty() { String::new() } else { format!("AND ({})", scopes.join(" OR ")) }
}

unsafe fn generate_sql(aqs: &str, roots: &[PathBuf], max_results: u32) -> Result<(String, String), String> {
    let manager: ISearchManager = CoCreateInstance(&CSearchManager, None, CLSCTX_ALL)
        .map_err(|error| format!("Windows Search manager is unavailable: {error}"))?;
    let catalog = manager.GetCatalog(PCWSTR(wide("SystemIndex").as_ptr()))
        .map_err(|error| format!("Windows SystemIndex is unavailable: {error}"))?;
    let helper = catalog.GetQueryHelper()
        .map_err(|error| format!("Windows Search query helper is unavailable: {error}"))?;
    helper.SetQuerySelectColumns(PCWSTR(wide("System.ItemPathDisplay").as_ptr()))
        .map_err(|error| format!("Could not select Windows Search result columns: {error}"))?;
    helper.SetQueryMaxResults(max_results.clamp(1, 100) as i32)
        .map_err(|error| format!("Could not limit Windows Search results: {error}"))?;
    let restriction = scope_restriction(roots);
    if !restriction.is_empty() {
        helper.SetQueryWhereRestrictions(PCWSTR(wide(&restriction).as_ptr()))
            .map_err(|error| format!("Could not scope Windows Search: {error}"))?;
    }
    let sql = take_wide(helper.GenerateSQLFromUserQuery(PCWSTR(wide(aqs).as_ptr()))
        .map_err(|error| format!("Invalid Windows Advanced Query Syntax: {error}"))?);
    let connection = take_wide(helper.ConnectionString()
        .map_err(|error| format!("Could not obtain the Windows Search connection: {error}"))?);
    Ok((sql, connection))
}

unsafe fn open_rowset(sql: &str, connection: &str) -> Result<(IRowset, IDBInitialize), String> {
    let initializer: IDataInitialize = CoCreateInstance(&MSDAINITIALIZE, None, CLSCTX_INPROC_SERVER)
        .map_err(|error| format!("OLE DB initialization is unavailable: {error}"))?;
    let mut source = None;
    initializer.GetDataSource(None, CLSCTX_INPROC_SERVER.0, PCWSTR(wide(connection).as_ptr()), &IDBInitialize::IID, &mut source)
        .map_err(|error| format!("Could not connect to Windows Search: {error}"))?;
    let database: IDBInitialize = source.ok_or_else(|| "Windows Search returned no data source.".to_string())?.cast()
        .map_err(|error| format!("Windows Search data source is invalid: {error}"))?;
    database.Initialize().map_err(|error| format!("Could not initialize Windows Search data source: {error}"))?;
    let session_factory: IDBCreateSession = database.cast()
        .map_err(|error| format!("Windows Search cannot create sessions: {error}"))?;
    let session = session_factory.CreateSession(None, &IDBCreateCommand::IID)
        .map_err(|error| format!("Could not create Windows Search session: {error}"))?;
    let command_factory: IDBCreateCommand = session.cast()
        .map_err(|error| format!("Windows Search session cannot create commands: {error}"))?;
    let command: ICommandText = command_factory.CreateCommand(None, &ICommandText::IID)
        .map_err(|error| format!("Could not create Windows Search command: {error}"))?.cast()
        .map_err(|error| format!("Windows Search command is invalid: {error}"))?;
    command.SetCommandText(&DBGUID_DEFAULT, PCWSTR(wide(sql).as_ptr()))
        .map_err(|error| format!("Could not prepare Windows Search SQL: {error}"))?;
    let command_base: ICommand = command.cast()
        .map_err(|error| format!("Windows Search command cannot execute: {error}"))?;
    let mut rowset_unknown = None;
    command_base.Execute(None, &IRowset::IID, None, None, Some(&mut rowset_unknown))
        .map_err(|error| format!("Windows Search query failed: {error}"))?;
    let rowset: IRowset = rowset_unknown.ok_or_else(|| "Windows Search returned no rowset.".to_string())?.cast()
        .map_err(|error| format!("Windows Search returned an invalid rowset: {error}"))?;
    Ok((rowset, database))
}

unsafe fn read_paths(rowset: &IRowset, limit: usize) -> Result<Vec<PathBuf>, String> {
    let accessor: IAccessor = rowset.cast().map_err(|error| format!("Windows Search rowset cannot expose data: {error}"))?;
    let binding = DBBINDING {
        iOrdinal: 1, obValue: offset_of!(PathRow, value), obLength: offset_of!(PathRow, length), obStatus: offset_of!(PathRow, status),
        pTypeInfo: ManuallyDrop::new(None), pObject: null_mut(), pBindExt: null_mut(),
        dwPart: (DBPART_VALUE.0 | DBPART_LENGTH.0 | DBPART_STATUS.0) as u32,
        dwMemOwner: DBMEMOWNER_CLIENTOWNED.0 as u32, eParamIO: DBPARAMIO_NOTPARAM.0 as u32,
        cbMaxLen: PATH_CAPACITY * size_of::<u16>(), dwFlags: 0, wType: DBTYPE_WSTR.0 as u16,
        bPrecision: 0, bScale: 0,
    };
    let mut handle = HACCESSOR::default();
    let mut binding_status = 0;
    accessor.CreateAccessor(DBACCESSOR_ROWDATA.0 as u32, 1, &binding, size_of::<PathRow>(), &mut handle, Some(&mut binding_status))
        .map_err(|error| format!("Could not bind Windows Search path column: {error}"))?;
    if binding_status != 0 {
        let _ = accessor.ReleaseAccessor(handle, None);
        return Err(format!("Windows Search rejected the path binding ({binding_status})."));
    }
    let mut paths = Vec::with_capacity(limit);
    while paths.len() < limit {
        let mut obtained = 0usize;
        let mut row_handles: *mut usize = null_mut();
        rowset.GetNextRows(0, 0, &mut obtained, std::slice::from_mut(&mut row_handles))
            .map_err(|error| format!("Could not read Windows Search rows: {error}"))?;
        if obtained == 0 || row_handles.is_null() { break; }
        for index in 0..obtained {
            if paths.len() >= limit { break; }
            let row_handle = *row_handles.add(index);
            let mut data = PathRow::default();
            rowset.GetData(row_handle, handle, (&mut data as *mut PathRow).cast::<c_void>())
                .map_err(|error| format!("Could not read a Windows Search result: {error}"))?;
            if data.status == DBSTATUS_S_OK.0 {
                let units = (data.length / size_of::<u16>()).min(PATH_CAPACITY);
                let text = String::from_utf16_lossy(&data.value[..units]);
                if !text.is_empty() { paths.push(PathBuf::from(text)); }
            }
        }
        rowset.ReleaseRows(obtained, row_handles, null(), null_mut(), null_mut())
            .map_err(|error| format!("Could not release Windows Search rows: {error}"))?;
        CoTaskMemFree(Some(row_handles.cast()));
    }
    accessor.ReleaseAccessor(handle, None)
        .map_err(|error| format!("Could not release Windows Search binding: {error}"))?;
    Ok(paths)
}

pub fn execute(aqs: &str, roots: &[PathBuf], max_results: u32) -> Result<Vec<FileEntry>, String> {
    let limit = max_results.clamp(1, 100) as usize;
    if aqs.trim().is_empty() { return Ok(Vec::new()); }
    let _com = ComApartment::initialize()?;
    let (sql, connection) = unsafe { generate_sql(aqs, roots, max_results) }?;
    let (rowset, database) = unsafe { open_rowset(&sql, &connection) }?;
    let paths = unsafe { read_paths(&rowset, limit) }?;
    drop(rowset);
    unsafe { database.Uninitialize().map_err(|error| format!("Could not close Windows Search data source: {error}"))?; }
    let mut seen = HashSet::with_capacity(limit);
    Ok(paths.into_iter().filter_map(file_entry)
        .filter(|entry| seen.insert(entry.path.to_ascii_lowercase())).take(limit).collect())
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::Instant};

    #[test]
    fn system_index_accepts_canonical_aqs() {
        let started = Instant::now();
        let result = super::execute("System.FileExtension:.pdf", &[PathBuf::from("C:\\")], 1);
        assert!(result.is_ok(), "{}", result.unwrap_err());
        assert!(started.elapsed().as_secs() < 10, "Windows Search query was unexpectedly slow");
    }
}
