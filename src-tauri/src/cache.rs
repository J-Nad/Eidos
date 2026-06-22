use std::{collections::HashMap, path::Path};

use crate::db::{FileEntry, FileRecord};

#[derive(Clone, Debug, Default)]
pub struct FileCache {
    records_by_drive: HashMap<char, Vec<FileEntry>>,
    total_records: usize,
}

impl FileCache {
    pub fn is_empty(&self) -> bool {
        self.total_records == 0
    }

    pub fn len(&self) -> usize {
        self.total_records
    }

    pub fn replace(&mut self, records: &[FileRecord]) {
        let mut next: HashMap<char, Vec<FileEntry>> = HashMap::new();
        for record in records {
            let drive = drive_key(&record.path);
            next.entry(drive).or_default().push(record.clone().into());
        }
        for records in next.values_mut() {
            records.sort_by(|left, right| {
                right
                    .is_directory
                    .cmp(&left.is_directory)
                    .then_with(|| left.filename.len().cmp(&right.filename.len()))
                    .then_with(|| left.path.cmp(&right.path))
            });
        }
        self.total_records = next.values().map(Vec::len).sum();
        self.records_by_drive = next;
    }

    pub fn upsert(&mut self, record: FileRecord) {
        self.remove(&record.path);
        let drive = drive_key(&record.path);
        self.records_by_drive
            .entry(drive)
            .or_default()
            .push(record.into());
        self.total_records += 1;
    }

    pub fn remove(&mut self, path: &str) {
        let drive = drive_key(path);
        if let Some(records) = self.records_by_drive.get_mut(&drive) {
            let before = records.len();
            let lower_path = path.to_ascii_lowercase();
            records.retain(|record| {
                let current = record.path.to_ascii_lowercase();
                current != lower_path && !current.starts_with(&(lower_path.clone() + "\\"))
            });
            self.total_records = self.total_records.saturating_sub(before - records.len());
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<FileEntry> {
        let normalized = normalize_query(query);
        if normalized.is_empty() {
            return Vec::new();
        }
        let preferred_drive = query
            .as_bytes()
            .first()
            .copied()
            .filter(|first| first.is_ascii_alphabetic())
            .map(|first| (first as char).to_ascii_uppercase());
        let mut buckets = Vec::new();
        if let Some(drive) = preferred_drive {
            if let Some(records) = self.records_by_drive.get(&drive) {
                buckets.push(records);
            }
        }
        for (drive, records) in &self.records_by_drive {
            if Some(*drive) != preferred_drive {
                buckets.push(records);
            }
        }

        let mut ranked = Vec::with_capacity(limit);
        for records in buckets {
            for record in records {
                if let Some(score) = score_record(record, &normalized) {
                    ranked.push((score, record.clone()));
                    if ranked.len() >= limit * 8 {
                        break;
                    }
                }
            }
            if ranked.len() >= limit * 8 {
                break;
            }
        }
        ranked.sort_by(|(left_score, left), (right_score, right)| {
            left_score
                .cmp(right_score)
                .then_with(|| right.is_directory.cmp(&left.is_directory))
                .then_with(|| left.filename.len().cmp(&right.filename.len()))
                .then_with(|| left.path.cmp(&right.path))
        });
        ranked
            .into_iter()
            .take(limit)
            .map(|(_, record)| record)
            .collect()
    }
}

fn drive_key(path: &str) -> char {
    path.as_bytes()
        .first()
        .copied()
        .filter(|byte| byte.is_ascii_alphabetic())
        .map(|byte| (byte as char).to_ascii_uppercase())
        .unwrap_or('#')
}

fn normalize_query(query: &str) -> String {
    query
        .trim()
        .trim_matches('"')
        .replace('/', "\\")
        .to_ascii_lowercase()
}

fn score_record(record: &FileEntry, query: &str) -> Option<u8> {
    let filename = record.filename.to_ascii_lowercase();
    if filename == query {
        return Some(0);
    }
    if filename.starts_with(query) {
        return Some(1);
    }
    if filename.contains(query) {
        return Some(2);
    }
    let path = record.path.to_ascii_lowercase();
    if path
        .split('\\')
        .any(|part| part == query || part.starts_with(query))
    {
        return Some(3);
    }
    if path.contains(query) {
        return Some(4);
    }
    None
}

impl From<FileRecord> for FileEntry {
    fn from(record: FileRecord) -> Self {
        Self {
            path: record.path,
            filename: record.filename,
            extension: record.extension,
            size: record.size,
            modified_timestamp: record.modified_timestamp,
            created_timestamp: record.created_timestamp,
            is_directory: record.is_directory,
            snippet: String::new(),
            is_content_match: false,
        }
    }
}

pub fn entry_from_path(path: &Path) -> Option<FileEntry> {
    crate::indexer::local_record(path).map(Into::into)
}
