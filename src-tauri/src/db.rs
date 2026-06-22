use std::{collections::HashSet, path::Path, time::Duration};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;

#[derive(Clone, Debug)]
pub struct FileRecord {
    pub path: String,
    pub filename: String,
    pub extension: String,
    pub size: i64,
    pub modified_timestamp: i64,
    pub created_timestamp: i64,
    pub is_directory: bool,
    pub metadata: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub filename: String,
    pub extension: String,
    pub size: i64,
    pub modified_timestamp: i64,
    pub created_timestamp: i64,
    pub is_directory: bool,
    pub snippet: String,
    pub is_content_match: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStats {
    pub files_added: u64,
    pub files_removed: u64,
    pub total_size: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMetadata {
    pub path: String,
    pub filename: String,
    pub extension: String,
    pub size: i64,
    pub modified_timestamp: i64,
    pub created_timestamp: i64,
    pub is_directory: bool,
}

const FILES_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS files (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        path TEXT UNIQUE NOT NULL,
        filename TEXT NOT NULL,
        extension TEXT NOT NULL DEFAULT '',
        size INTEGER NOT NULL DEFAULT 0,
        modified_timestamp INTEGER NOT NULL DEFAULT 0,
        created_timestamp INTEGER NOT NULL DEFAULT 0,
        is_directory INTEGER NOT NULL DEFAULT 0,
        metadata TEXT DEFAULT '{}'
    );";

fn existing_columns(connection: &Connection, table: &str) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| error.to_string())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(columns)
}

fn recreate_files_table(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys=OFF;
             BEGIN IMMEDIATE;
             DROP TRIGGER IF EXISTS files_ai;
             DROP TRIGGER IF EXISTS files_ad;
             DROP TRIGGER IF EXISTS files_au;
             DROP TABLE IF EXISTS files_fts;
             DROP INDEX IF EXISTS files_filename_idx;
             DROP INDEX IF EXISTS files_modified_idx;
             ALTER TABLE files RENAME TO files_legacy;
             CREATE TABLE files (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 path TEXT UNIQUE NOT NULL,
                 filename TEXT NOT NULL,
                 extension TEXT NOT NULL DEFAULT '',
                 size INTEGER NOT NULL DEFAULT 0,
                 modified_timestamp INTEGER NOT NULL DEFAULT 0,
                 created_timestamp INTEGER NOT NULL DEFAULT 0,
                 is_directory INTEGER NOT NULL DEFAULT 0,
                 metadata TEXT DEFAULT '{}'
             );
             INSERT INTO files(path, filename, extension, size, modified_timestamp, created_timestamp, is_directory, metadata)
             SELECT path, filename, extension, size, modified_timestamp, 0, 0, COALESCE(metadata, '{}')
             FROM files_legacy;
             DROP TABLE files_legacy;
             COMMIT;
             PRAGMA foreign_keys=ON;",
        )
        .map_err(|error| error.to_string())
}

pub fn initialize_database(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;
             PRAGMA temp_store=MEMORY;
             PRAGMA cache_size=-32768;
             PRAGMA mmap_size=268435456;",
        )
        .map_err(|error| error.to_string())?;

    let files_exists: Option<String> = connection
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='files'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if files_exists.is_some() {
        let columns = existing_columns(&connection, "files")?;
        if columns.iter().any(|column| column == "source") || !columns.iter().any(|column| column == "is_directory") {
            recreate_files_table(&connection)?;
        } else if !columns.iter().any(|column| column == "created_timestamp") {
            connection
                .execute_batch("ALTER TABLE files ADD COLUMN created_timestamp INTEGER NOT NULL DEFAULT 0;")
                .map_err(|error| error.to_string())?;
        }
    }

    connection
        .execute_batch(FILES_SCHEMA)
        .map_err(|error| error.to_string())?;

    let fts_columns = existing_columns(&connection, "files_fts").unwrap_or_default();
    if !fts_columns.is_empty() && fts_columns != ["filename", "extension", "path"] {
        connection
            .execute_batch(
                "DROP TRIGGER IF EXISTS files_ai;
                 DROP TRIGGER IF EXISTS files_ad;
                 DROP TRIGGER IF EXISTS files_au;
                 DROP TABLE IF EXISTS files_fts;",
            )
            .map_err(|error| error.to_string())?;
    }

    connection
        .execute_batch(
            "CREATE INDEX IF NOT EXISTS files_filename_idx ON files(filename);
             CREATE INDEX IF NOT EXISTS files_modified_idx ON files(modified_timestamp DESC);
             CREATE INDEX IF NOT EXISTS files_created_idx ON files(created_timestamp DESC);
             CREATE INDEX IF NOT EXISTS files_extension_idx ON files(extension);
             CREATE VIRTUAL TABLE IF NOT EXISTS files_fts USING fts5(
                 filename,
                 extension,
                 path,
                 tokenize='unicode61'
             );
             CREATE TRIGGER IF NOT EXISTS files_ai AFTER INSERT ON files BEGIN
                 INSERT INTO files_fts(rowid, filename, extension, path)
                 VALUES (new.id, new.filename, new.extension, new.path);
             END;
             CREATE TRIGGER IF NOT EXISTS files_ad AFTER DELETE ON files BEGIN
                 DELETE FROM files_fts WHERE rowid=old.id;
             END;
             CREATE TRIGGER IF NOT EXISTS files_au AFTER UPDATE ON files BEGIN
                 DELETE FROM files_fts WHERE rowid=old.id;
                 INSERT INTO files_fts(rowid, filename, extension, path)
                 VALUES (new.id, new.filename, new.extension, new.path);
             END;",
        )
        .map_err(|error| error.to_string())?;

    let fts_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM files_fts", [], |row| row.get(0))
        .unwrap_or(0);
    let file_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
        .unwrap_or(0);
    if file_count > 0 && fts_count == 0 {
        rebuild_fts(&connection)?;
    }
    Ok(connection)
}

pub fn rebuild_fts(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "DELETE FROM files_fts;
             INSERT INTO files_fts(rowid, filename, extension, path)
             SELECT id, filename, extension, path FROM files;
             INSERT INTO files_fts(files_fts) VALUES('optimize');",
        )
        .map_err(|error| error.to_string())
}

pub fn sanitize_fts_query(query: &str) -> String {
    query
        .split(|character: char| !character.is_alphanumeric() && character != '_' && character != '-')
        .filter(|token| !token.is_empty())
        .take(12)
        .map(|token| format!("\"{}\"*", token.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" AND ")
}

pub fn search_fts(connection: &Connection, query: &str) -> Result<Vec<FileEntry>, String> {
    let sanitized = sanitize_fts_query(query);
    if sanitized.is_empty() {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare_cached(
            "SELECT f.path, f.filename, f.extension, f.size, f.modified_timestamp, f.created_timestamp,
                    f.is_directory, snippet(files_fts, 0, '<b>', '</b>', '…', 32)
             FROM files_fts
             JOIN files f ON f.id=files_fts.rowid
             WHERE files_fts MATCH ?1
             ORDER BY rank
             LIMIT 20",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([sanitized], |row| {
            Ok(FileEntry {
                path: row.get(0)?,
                filename: row.get(1)?,
                extension: row.get(2)?,
                size: row.get(3)?,
                modified_timestamp: row.get(4)?,
                created_timestamp: row.get(5)?,
                is_directory: row.get::<_, i64>(6)? != 0,
                snippet: row.get(7)?,
                is_content_match: false,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn all_records(connection: &Connection) -> Result<Vec<FileRecord>, String> {
    let mut statement = connection
        .prepare_cached(
            "SELECT path, filename, extension, size, modified_timestamp, created_timestamp, is_directory, COALESCE(metadata, '{}')
             FROM files ORDER BY path",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(FileRecord {
                path: row.get(0)?,
                filename: row.get(1)?,
                extension: row.get(2)?,
                size: row.get(3)?,
                modified_timestamp: row.get(4)?,
                created_timestamp: row.get(5)?,
                is_directory: row.get::<_, i64>(6)? != 0,
                metadata: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn prepare_upsert<'a>(
    transaction: &'a rusqlite::Transaction<'_>,
) -> Result<rusqlite::CachedStatement<'a>, String> {
    transaction
        .prepare_cached(
            "INSERT INTO files(path, filename, extension, size, modified_timestamp, created_timestamp, is_directory, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(path) DO UPDATE SET
                 filename=excluded.filename,
                 extension=excluded.extension,
                 size=excluded.size,
                 modified_timestamp=excluded.modified_timestamp,
                 created_timestamp=excluded.created_timestamp,
                 is_directory=excluded.is_directory,
                 metadata=excluded.metadata
             WHERE files.filename<>excluded.filename
                OR files.extension<>excluded.extension
                OR files.size<>excluded.size
                OR files.modified_timestamp<>excluded.modified_timestamp
                OR files.created_timestamp<>excluded.created_timestamp
                OR files.is_directory<>excluded.is_directory
                OR files.metadata<>excluded.metadata",
        )
        .map_err(|error| error.to_string())
}

fn insert_records(transaction: &rusqlite::Transaction<'_>, records: &[FileRecord]) -> Result<(), String> {
    let mut statement = prepare_upsert(transaction)?;
    for record in records {
        statement
            .execute(params![
                record.path,
                record.filename,
                record.extension,
                record.size,
                record.modified_timestamp,
                record.created_timestamp,
                if record.is_directory { 1_i64 } else { 0_i64 },
                record.metadata
            ])
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn sync_all(connection: &mut Connection, records: &[FileRecord]) -> Result<IndexStats, String> {
    let existing = {
        let mut statement = connection
            .prepare("SELECT path FROM files")
            .map_err(|error| error.to_string())?;
        let paths = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .collect::<HashSet<_>>();
        paths
    };
    let incoming = records
        .iter()
        .map(|record| record.path.as_str())
        .collect::<HashSet<_>>();
    let added = records
        .iter()
        .filter(|record| !existing.contains(&record.path))
        .count() as u64;
    let removed = existing
        .iter()
        .filter(|path| !incoming.contains(path.as_str()))
        .count() as u64;
    let total_size = records
        .iter()
        .filter(|record| !record.is_directory)
        .map(|record| record.size.max(0) as u64)
        .sum();

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Exclusive)
        .map_err(|error| error.to_string())?;
    {
        let mut delete = transaction
            .prepare_cached("DELETE FROM files WHERE path=?1")
            .map_err(|error| error.to_string())?;
        for path in existing
            .iter()
            .filter(|path| !incoming.contains(path.as_str()))
        {
            delete.execute([path]).map_err(|error| error.to_string())?;
        }
    }
    insert_records(&transaction, records)?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(IndexStats {
        files_added: added,
        files_removed: removed,
        total_size,
    })
}

pub fn upsert_one(connection: &mut Connection, record: &FileRecord) -> Result<(), String> {
    let transaction = connection.transaction().map_err(|error| error.to_string())?;
    insert_records(&transaction, std::slice::from_ref(record))?;
    transaction.commit().map_err(|error| error.to_string())
}

pub fn upsert_many(connection: &mut Connection, records: &[FileRecord]) -> Result<IndexStats, String> {
    let mut existing = HashSet::new();
    {
        let mut statement = connection.prepare_cached("SELECT path FROM files WHERE path=?1")
            .map_err(|error| error.to_string())?;
        for record in records {
            if statement.exists([&record.path]).map_err(|error| error.to_string())? {
                existing.insert(record.path.as_str());
            }
        }
    }
    let files_added = records.iter().filter(|record| !existing.contains(record.path.as_str())).count() as u64;
    let total_size = records.iter().filter(|record| !record.is_directory).map(|record| record.size.max(0) as u64).sum();
    let transaction = connection.transaction().map_err(|error| error.to_string())?;
    insert_records(&transaction, records)?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(IndexStats { files_added, files_removed: 0, total_size })
}

pub fn delete_path(connection: &Connection, path: &Path) -> Result<(), String> {
    let path = path.to_string_lossy();
    let prefix = format!("{}\\%", path);
    connection
        .execute(
            "DELETE FROM files WHERE path=?1 OR path LIKE ?2",
            params![path.as_ref(), prefix],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub fn file_metadata(connection: &Connection, path: &str) -> Result<Option<FileMetadata>, String> {
    connection
        .query_row(
            "SELECT path, filename, extension, size, modified_timestamp, created_timestamp, is_directory FROM files WHERE path=?1",
            [path],
            |row| {
                Ok(FileMetadata {
                    path: row.get(0)?,
                    filename: row.get(1)?,
                    extension: row.get(2)?,
                    size: row.get(3)?,
                    modified_timestamp: row.get(4)?,
                    created_timestamp: row.get(5)?,
                    is_directory: row.get::<_, i64>(6)? != 0,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_legacy_source_schema() {
        let path =
            std::env::temp_dir().join(format!("eidos-db-test-{}.sqlite", uuid::Uuid::new_v4()));
        {
            let legacy = Connection::open(&path).expect("create legacy database");
            legacy
                .execute_batch(
                    "CREATE TABLE files (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        path TEXT UNIQUE NOT NULL,
                        filename TEXT NOT NULL,
                        extension TEXT NOT NULL DEFAULT '',
                        size INTEGER NOT NULL DEFAULT 0,
                        modified_timestamp INTEGER NOT NULL DEFAULT 0,
                        source TEXT NOT NULL CHECK(source IN ('local', 'github')),
                        metadata TEXT DEFAULT '{}'
                    );
                    INSERT INTO files(path, filename, extension, source) VALUES
                        ('C:\\notes.txt', 'notes.txt', 'txt', 'local');",
                )
                .expect("seed legacy database");
        }
        let database = initialize_database(&path).expect("migrate database");
        let schema: String = database
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name='files'",
                [],
                |row| row.get(0),
            )
            .expect("read schema");
        assert!(!schema.contains("source"));
        assert!(schema.contains("is_directory"));
        let results = search_fts(&database, "notes").expect("search migrated FTS");
        assert_eq!(results.len(), 1);
        drop(database);
        let _ = std::fs::remove_file(path);
    }
}
