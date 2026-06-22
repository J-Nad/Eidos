use std::sync::LazyLock;

use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone, Weekday};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MetadataFilters {
    #[serde(default)]
    pub extensions: Vec<String>,
    pub kind: Option<FileKind>,
    #[serde(default, alias = "size_min")]
    pub size_min: Option<u64>,
    #[serde(default, alias = "size_max")]
    pub size_max: Option<u64>,
    #[serde(default, alias = "modified_after", deserialize_with = "deserialize_optional_timestamp")]
    pub modified_after: Option<i64>,
    #[serde(default, alias = "modified_before", deserialize_with = "deserialize_optional_timestamp_end")]
    pub modified_before: Option<i64>,
    #[serde(default, alias = "created_after", deserialize_with = "deserialize_optional_timestamp")]
    pub created_after: Option<i64>,
    #[serde(default, alias = "created_before", deserialize_with = "deserialize_optional_timestamp_end")]
    pub created_before: Option<i64>,
    #[serde(default, alias = "name_query")]
    pub name_query: Option<String>,
    #[serde(default, alias = "has_content_intent")]
    pub has_content_intent: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Document,
    Spreadsheet,
    Image,
    Video,
    Audio,
    Code,
    Archive,
    Folder,
}

fn timestamp_from_date(value: &str, end_of_day: bool) -> Option<i64> {
    let date = NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").ok()?;
    let time = if end_of_day {
        date.and_hms_opt(23, 59, 59)?
    } else {
        date.and_hms_opt(0, 0, 0)?
    };
    Some(Local.from_local_datetime(&time).earliest()?.timestamp())
}

fn deserialize_optional_timestamp<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(None),
        Value::Number(number) => Ok(number.as_i64()),
        Value::String(text) => {
            if text.trim().is_empty() {
                Ok(None)
            } else if let Ok(timestamp) = text.parse::<i64>() {
                Ok(Some(timestamp))
            } else {
                Ok(timestamp_from_date(&text, false))
            }
        }
        _ => Ok(None),
    }
}

fn deserialize_optional_timestamp_end<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(None),
        Value::Number(number) => Ok(number.as_i64()),
        Value::String(text) => {
            if text.trim().is_empty() {
                Ok(None)
            } else if let Ok(timestamp) = text.parse::<i64>() {
                Ok(Some(timestamp))
            } else {
                Ok(timestamp_from_date(&text, true))
            }
        }
        _ => Ok(None),
    }
}

impl MetadataFilters {
    pub fn is_empty(&self) -> bool {
        self.extensions.is_empty()
            && self.kind.is_none()
            && self.size_min.is_none()
            && self.size_max.is_none()
            && self.modified_after.is_none()
            && self.modified_before.is_none()
            && self.created_after.is_none()
            && self.created_before.is_none()
            && self.name_query.as_deref().unwrap_or_default().trim().is_empty()
    }

    pub fn normalized(mut self) -> Self {
        self.extensions = self
            .extensions
            .into_iter()
            .map(|value| value.trim().trim_start_matches('.').to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .collect();
        self.extensions.sort();
        self.extensions.dedup();
        self.name_query = self
            .name_query
            .map(|value| value.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|value| !value.is_empty());
        self
    }

}

static EXTENSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:\*\.|\b(?:type|kind|ext|extension)\s*[:=]?\s*|\b)(pdfs?|docx?|xlsx?|csv|pptx?|txt|md|rtf|json|xml|ya?ml|toml|rs|go|py|java|jsx?|tsx?|html|css|png|jpe?g|gif|webp|bmp|tiff?|heic|mp4|mov|avi|mkv|webm|m4v|mp3|wav|flac|aac|ogg|zip|rar|7z)\b")
        .expect("metadata extension regex")
});
static KIND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(documents?|spreadsheets?|presentations?|powerpoints?|images?|pictures?|photos?|videos?|movies?|audio|music|code|source|archives?|folders?)\b")
        .expect("metadata kind regex")
});
static OFFICE_KIND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(word\s+docs?|excel\s+files?|powerpoints?)\b")
        .expect("metadata office type regex")
});
static SIZE_GREATER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:larger|bigger|greater|more)\s+than\s+(\d+(?:\.\d+)?)\s*(bytes?|kb|mb|gb)\b|\bsize\s*(?:>|greater\s+than|over|at\s+least)\s*(\d+(?:\.\d+)?)\s*(bytes?|kb|mb|gb)\b|\b(?:over|at\s+least)\s+(\d+(?:\.\d+)?)\s*(bytes?|kb|mb|gb)\b")
        .expect("metadata size min regex")
});
static SIZE_LESS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:smaller|less)\s+than\s+(\d+(?:\.\d+)?)\s*(bytes?|kb|mb|gb)\b|\bsize\s*(?:<|less\s+than|under|at\s+most)\s*(\d+(?:\.\d+)?)\s*(bytes?|kb|mb|gb)\b|\b(?:under|at\s+most)\s+(\d+(?:\.\d+)?)\s*(bytes?|kb|mb|gb)\b")
        .expect("metadata size max regex")
});
static SIZE_BETWEEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:size\s+)?between\s+(\d+(?:\.\d+)?)\s*(bytes?|kb|mb|gb)?\s+and\s+(\d+(?:\.\d+)?)\s*(bytes?|kb|mb|gb)\b")
        .expect("metadata size range regex")
});
static LARGE_SIZE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:large|big|huge)\s+(?:files?|videos?|documents?|images?|spreadsheets?)\b|\b(?:large|big|huge)\b")
        .expect("metadata large size regex")
});
static MODIFIED_RELATIVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bmodified\s+(?:(?:within|in)\s+(?:the\s+)?)?(today|yesterday|this\s+(?:morning|evening|week|month|year)|last\s+(?:week|month|year)|(?:past|last)\s+(\d{1,4})\s+days?|(?:the\s+)?(?:past|last)\s+(week|month|year))\b")
        .expect("metadata modified regex")
});
static FROM_RELATIVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bfrom\s+(today|yesterday|this\s+(?:morning|evening|week|month|year)|last\s+(?:week|month|year)|(?:past|last)\s+(\d{1,4})\s+days?)\b")
        .expect("metadata generic date regex")
});
static CREATED_RELATIVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bcreated\s+(?:(?:within|in)\s+(?:the\s+)?)?(today|yesterday|this\s+(?:morning|evening|week|month|year)|last\s+(?:week|month|year)|(?:past|last)\s+(\d{1,4})\s+days?|(?:the\s+)?(?:past|last)\s+(week|month|year))\b")
        .expect("metadata created regex")
});
static FROM_WEEKDAY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bfrom\s+last\s+(monday|tuesday|wednesday|thursday|friday|saturday|sunday)\b")
        .expect("metadata weekday regex")
});
static CONTENT_INTENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:\bcontaining\b|\bwith\s+the\s+phrase\b|\binside\b|"[^"]+")"#)
        .expect("content intent regex")
});
static CONTENT_CLAUSE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(?:containing|with\s+the\s+phrase|inside)\b\s*(?:["'][^"']+["']|[^,;]+)?|"[^"]+""#)
        .expect("content clause regex")
});
static MODIFIED_BETWEEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bmodified\s+between\s+(\d{4}-\d{2}-\d{2}|\d{1,2}/\d{1,2}/\d{2,4})\s+and\s+(\d{4}-\d{2}-\d{2}|\d{1,2}/\d{1,2}/\d{2,4})")
        .expect("metadata modified between regex")
});
static CREATED_BETWEEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bcreated\s+between\s+(\d{4}-\d{2}-\d{2}|\d{1,2}/\d{1,2}/\d{2,4})\s+and\s+(\d{4}-\d{2}-\d{2}|\d{1,2}/\d{1,2}/\d{2,4})")
        .expect("metadata created between regex")
});
static NAMED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(?:named|filename|file\s+name)\s*[:=]?\s*["'“‘]?([^"'”’]+?)["'”’]?(?:\s+(?:modified|created|larger|bigger|smaller|less|size|from|today|yesterday|this|last|past|before|after)\b|$)"#)
        .expect("metadata name regex")
});
static STOP_WORDS: &[&str] = &[
    "find", "me", "a", "an", "the", "file", "files", "folder", "folders", "all", "show",
    "search", "for", "with", "from", "that", "are", "is", "were", "was", "i", "my",
];

fn date_bounds(start: NaiveDate, end: NaiveDate) -> (i64, i64) {
    let start = Local
        .from_local_datetime(&start.and_hms_opt(0, 0, 0).expect("valid start"))
        .earliest()
        .expect("local start")
        .timestamp();
    let end = Local
        .from_local_datetime(&end.and_hms_opt(23, 59, 59).expect("valid end"))
        .latest()
        .expect("local end")
        .timestamp();
    (start, end)
}

fn week_start(today: NaiveDate) -> NaiveDate {
    today - Duration::days(today.weekday().num_days_from_monday() as i64)
}

fn month_start(today: NaiveDate) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
}

fn previous_month_start(today: NaiveDate) -> Option<NaiveDate> {
    let (year, month) = if today.month() == 1 {
        (today.year() - 1, 12)
    } else {
        (today.year(), today.month() - 1)
    };
    NaiveDate::from_ymd_opt(year, month, 1)
}

fn relative_bounds(phrase: &str, days_capture: Option<&str>, today: NaiveDate) -> Option<(i64, i64)> {
    let lower = phrase.to_ascii_lowercase();
    let lower = lower.trim_start_matches("the ");
    if lower == "today" || lower == "this morning" || lower == "this evening" {
        Some(date_bounds(today, today))
    } else if lower == "yesterday" {
        let day = today - Duration::days(1);
        Some(date_bounds(day, day))
    } else if lower == "this week" {
        Some(date_bounds(week_start(today), today))
    } else if lower == "last week" || lower == "past week" {
        let this_monday = week_start(today);
        Some(date_bounds(
            this_monday - Duration::days(7),
            this_monday - Duration::days(1),
        ))
    } else if lower == "this month" {
        Some(date_bounds(month_start(today)?, today))
    } else if lower == "last month" || lower == "past month" {
        Some(date_bounds(
            previous_month_start(today)?,
            month_start(today)? - Duration::days(1),
        ))
    } else if lower == "this year" {
        Some(date_bounds(NaiveDate::from_ymd_opt(today.year(), 1, 1)?, today))
    } else if lower == "last year" || lower == "past year" {
        Some(date_bounds(
            NaiveDate::from_ymd_opt(today.year() - 1, 1, 1)?,
            NaiveDate::from_ymd_opt(today.year() - 1, 12, 31)?,
        ))
    } else if lower.starts_with("past ") || lower.starts_with("last ") {
        let days = days_capture?.parse::<i64>().ok()?.clamp(0, 3650);
        Some(date_bounds(today - Duration::days(days), today))
    } else {
        None
    }
}

fn parse_weekday(value: &str) -> Option<Weekday> {
    match value.to_ascii_lowercase().as_str() {
        "monday" => Some(Weekday::Mon),
        "tuesday" => Some(Weekday::Tue),
        "wednesday" => Some(Weekday::Wed),
        "thursday" => Some(Weekday::Thu),
        "friday" => Some(Weekday::Fri),
        "saturday" => Some(Weekday::Sat),
        "sunday" => Some(Weekday::Sun),
        _ => None,
    }
}

fn previous_weekday(today: NaiveDate, target: Weekday) -> NaiveDate {
    let mut days = (today.weekday().num_days_from_monday() as i64
        - target.num_days_from_monday() as i64)
        .rem_euclid(7);
    if days == 0 { days = 7; }
    today - Duration::days(days)
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .or_else(|| NaiveDate::parse_from_str(value, "%m/%d/%Y").ok())
        .or_else(|| NaiveDate::parse_from_str(value, "%m/%d/%y").ok())
}

fn parse_size(amount: &str, unit: &str) -> Option<u64> {
    let amount = amount.parse::<f64>().ok()?;
    let multiplier = match unit.to_ascii_lowercase().as_str() {
        "byte" | "bytes" => 1_f64,
        "kb" => 1024_f64,
        "mb" => 1024_f64.powi(2),
        "gb" => 1024_f64.powi(3),
        _ => return None,
    };
    Some((amount * multiplier) as u64)
}

fn amount_unit<'a>(capture: &'a regex::Captures<'a>) -> Option<(&'a str, &'a str)> {
    [(1, 2), (3, 4), (5, 6)]
        .into_iter()
        .find_map(|(amount, unit)| Some((capture.get(amount)?.as_str(), capture.get(unit)?.as_str())))
}

fn extension_kind(extension: &str) -> Option<FileKind> {
    match extension {
        "pdf" | "doc" | "docx" | "txt" | "md" | "rtf" => Some(FileKind::Document),
        "xls" | "xlsx" | "csv" => Some(FileKind::Spreadsheet),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "heic" => Some(FileKind::Image),
        "mp4" | "mov" | "avi" | "mkv" | "webm" | "m4v" => Some(FileKind::Video),
        "mp3" | "wav" | "flac" | "aac" | "ogg" => Some(FileKind::Audio),
        "py" | "js" | "ts" | "rs" | "go" | "java" | "html" | "css" | "json" | "xml" | "yaml" | "yml" | "toml" => Some(FileKind::Code),
        "zip" | "rar" | "7z" => Some(FileKind::Archive),
        _ => None,
    }
}

fn kind_from_word(word: &str) -> Option<FileKind> {
    match word.to_ascii_lowercase().as_str() {
        "document" | "documents" => Some(FileKind::Document),
        "spreadsheet" | "spreadsheets" => Some(FileKind::Spreadsheet),
        "presentation" | "presentations" | "powerpoint" | "powerpoints" => Some(FileKind::Document),
        "image" | "images" | "picture" | "pictures" | "photo" | "photos" => Some(FileKind::Image),
        "video" | "videos" | "movie" | "movies" => Some(FileKind::Video),
        "audio" | "music" => Some(FileKind::Audio),
        "code" | "source" => Some(FileKind::Code),
        "archive" | "archives" => Some(FileKind::Archive),
        "folder" | "folders" => Some(FileKind::Folder),
        _ => None,
    }
}

fn stripped_query(query: &str) -> String {
    let mut text = query.to_string();
    for regex in [
        &*EXTENSION,
        &*OFFICE_KIND,
        &*KIND,
        &*SIZE_GREATER,
        &*SIZE_LESS,
        &*SIZE_BETWEEN,
        &*LARGE_SIZE,
        &*MODIFIED_RELATIVE,
        &*CREATED_RELATIVE,
        &*FROM_RELATIVE,
        &*FROM_WEEKDAY,
        &*MODIFIED_BETWEEN,
        &*CREATED_BETWEEN,
        &*NAMED,
        &*CONTENT_CLAUSE,
    ] {
        text = regex.replace_all(&text, " ").to_string();
    }
    text.split(|character: char| !character.is_alphanumeric() && character != '_' && character != '-')
        .filter(|token| !token.is_empty())
        .filter(|token| !STOP_WORDS.iter().any(|word| token.eq_ignore_ascii_case(word)))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn kind_matches(kind: &FileKind, extension: &str, is_directory: bool) -> bool {
    if matches!(kind, FileKind::Folder) {
        return is_directory;
    }
    if is_directory {
        return false;
    }
    extension_kind(&extension.to_ascii_lowercase()).as_ref() == Some(kind)
}

pub fn parse_metadata_from_query(query: &str) -> Option<MetadataFilters> {
    let query = query.trim();
    if query.is_empty() {
        return None;
    }
    let today = Local::now().date_naive();
    let mut filters = MetadataFilters {
        extensions: Vec::new(),
        kind: None,
        size_min: None,
        size_max: None,
        modified_after: None,
        modified_before: None,
        created_after: None,
        created_before: None,
        name_query: None,
        has_content_intent: CONTENT_INTENT.is_match(query),
    };
    let mut matched = false;

    for capture in EXTENSION.captures_iter(query) {
        if let Some(extension) = capture.get(1) {
            let extension = extension.as_str().trim_start_matches('.').to_ascii_lowercase();
            filters
                .extensions
                .push(if extension == "pdfs" { "pdf".to_string() } else { extension });
            matched = true;
        }
    }
    for capture in OFFICE_KIND.captures_iter(query) {
        let phrase = capture.get(1).map(|value| value.as_str().to_ascii_lowercase()).unwrap_or_default();
        if phrase.starts_with("word") {
            filters.extensions.extend(["doc".to_string(), "docx".to_string()]);
        } else if phrase.starts_with("excel") {
            filters.extensions.extend(["xls".to_string(), "xlsx".to_string()]);
        } else {
            filters.extensions.extend(["ppt".to_string(), "pptx".to_string()]);
        }
        matched = true;
    }
    if let Some(capture) = KIND.captures(query) {
        if let Some(kind) = capture.get(1).and_then(|value| kind_from_word(value.as_str())) {
            filters.kind = Some(kind);
            matched = true;
        }
    }
    if let Some(capture) = SIZE_GREATER.captures(query) {
        if let Some(bytes) = amount_unit(&capture).and_then(|(amount, unit)| parse_size(amount, unit)) {
            filters.size_min = Some(bytes);
            matched = true;
        }
    }
    if let Some(capture) = SIZE_LESS.captures(query) {
        if let Some(bytes) = amount_unit(&capture).and_then(|(amount, unit)| parse_size(amount, unit)) {
            filters.size_max = Some(bytes);
            matched = true;
        }
    }
    if let Some(capture) = SIZE_BETWEEN.captures(query) {
        let first_unit = capture.get(2).map(|value| value.as_str());
        let second_unit = capture.get(4).map(|value| value.as_str());
        if let (Some(minimum), Some(maximum)) = (
            capture.get(1).and_then(|value| parse_size(value.as_str(), first_unit.or(second_unit).unwrap_or("bytes"))),
            capture.get(3).and_then(|value| parse_size(value.as_str(), second_unit.or(first_unit).unwrap_or("bytes"))),
        ) {
            filters.size_min = Some(minimum.min(maximum));
            filters.size_max = Some(minimum.max(maximum));
            matched = true;
        }
    }
    if filters.size_min.is_none() && LARGE_SIZE.is_match(query) {
        filters.size_min = Some(100 * 1024 * 1024);
        matched = true;
    }
    if let Some(capture) = MODIFIED_RELATIVE.captures(query) {
        if let Some((after, before)) = relative_bounds(
            capture.get(1)?.as_str(),
            capture.get(2).map(|value| value.as_str()),
            today,
        ) {
            filters.modified_after = Some(after);
            filters.modified_before = Some(before);
            matched = true;
        }
    }
    if let Some(capture) = CREATED_RELATIVE.captures(query) {
        if let Some((after, before)) = relative_bounds(
            capture.get(1)?.as_str(),
            capture.get(2).map(|value| value.as_str()),
            today,
        ) {
            filters.created_after = Some(after);
            filters.created_before = Some(before);
            matched = true;
        }
    }
    if filters.modified_after.is_none() && filters.modified_before.is_none() {
        if let Some(capture) = FROM_RELATIVE.captures(query) {
            if let Some((after, before)) = relative_bounds(
                capture.get(1)?.as_str(),
                capture.get(2).map(|value| value.as_str()),
                today,
            ) {
                filters.modified_after = Some(after);
                filters.modified_before = Some(before);
                matched = true;
            }
        }
    }
    if let Some(capture) = FROM_WEEKDAY.captures(query) {
        if let Some(day) = capture.get(1).and_then(|value| parse_weekday(value.as_str())) {
            let date = previous_weekday(today, day);
            let (after, before) = date_bounds(date, date);
            filters.modified_after = Some(after);
            filters.modified_before = Some(before);
            matched = true;
        }
    }
    if let Some(capture) = MODIFIED_BETWEEN.captures(query) {
        if let (Some(start), Some(end)) = (
            capture.get(1).and_then(|value| parse_date(value.as_str())),
            capture.get(2).and_then(|value| parse_date(value.as_str())),
        ) {
            let (after, before) = date_bounds(start, end);
            filters.modified_after = Some(after);
            filters.modified_before = Some(before);
            matched = true;
        }
    }
    if let Some(capture) = CREATED_BETWEEN.captures(query) {
        if let (Some(start), Some(end)) = (
            capture.get(1).and_then(|value| parse_date(value.as_str())),
            capture.get(2).and_then(|value| parse_date(value.as_str())),
        ) {
            let (after, before) = date_bounds(start, end);
            filters.created_after = Some(after);
            filters.created_before = Some(before);
            matched = true;
        }
    }
    if let Some(capture) = NAMED.captures(query) {
        if let Some(name) = capture.get(1).map(|value| value.as_str().trim()).filter(|value| !value.is_empty()) {
            filters.name_query = Some(name.to_string());
            matched = true;
        }
    }
    let remaining = stripped_query(query);
    if filters.name_query.is_none() && !remaining.is_empty() {
        filters.name_query = Some(remaining);
    }

    (matched || filters.has_content_intent).then(|| filters.normalized())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pdf_size_and_modified() {
        let filters = parse_metadata_from_query("PDF files modified this week larger than 2 MB")
            .expect("metadata filters");
        assert_eq!(filters.extensions, vec!["pdf"]);
        assert_eq!(filters.size_min, Some(2 * 1024 * 1024));
        assert!(filters.modified_after.is_some());
        assert!(filters.modified_before.is_some());
    }

    #[test]
    fn parses_spreadsheets_created_today() {
        let filters = parse_metadata_from_query("spreadsheets created today").expect("metadata filters");
        assert_eq!(filters.kind, Some(FileKind::Spreadsheet));
        assert!(filters.created_after.is_some());
        assert!(filters.created_before.is_some());
    }

    #[test]
    fn keeps_remaining_words_as_name_query() {
        let filters = parse_metadata_from_query("report pdf larger than 1 MB").expect("metadata filters");
        assert_eq!(filters.extensions, vec!["pdf"]);
        assert_eq!(filters.name_query.as_deref(), Some("report"));
    }

    #[test]
    fn separates_content_intent_from_filename_metadata() {
        let filters = parse_metadata_from_query("pdf files from yesterday containing invoice")
            .expect("metadata filters");
        assert_eq!(filters.extensions, vec!["pdf"]);
        assert_eq!(filters.name_query, None);
        assert!(filters.has_content_intent);
        assert!(filters.modified_after.is_some());
    }

    #[test]
    fn parses_within_past_days_and_size_range() {
        let filters = parse_metadata_from_query("pdfs modified within the past 2 days between 2 and 8 MB")
            .expect("metadata filters");
        assert_eq!(filters.extensions, vec!["pdf"]);
        assert_eq!(filters.size_min, Some(2 * 1024 * 1024));
        assert_eq!(filters.size_max, Some(8 * 1024 * 1024));
        assert!(filters.modified_after.is_some());
    }

    #[test]
    fn parses_large_video_metadata() {
        let filters = parse_metadata_from_query("large video files modified this month")
            .expect("metadata filters");
        assert_eq!(filters.kind, Some(FileKind::Video));
        assert_eq!(filters.size_min, Some(100 * 1024 * 1024));
        assert!(filters.modified_after.is_some());
    }
}
