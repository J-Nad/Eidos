use std::sync::LazyLock;

use chrono::{Datelike, Duration, Local, NaiveDate};
use regex::Regex;

static EXTENSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)(?:\*\.|\b(?:type|kind|ext|extension)\s*[:=]?\s*|\b)(pdf|docx?|xlsx?|csv|pptx?|txt|md|rtf|json|xml|ya?ml|toml|rs|go|py|jsx?|tsx?|html|css|png|jpe?g|gif|webp|bmp|tiff?|heic|mp4|mov|avi|mkv|webm|m4v|mp3|wav|flac|aac|ogg)\b",
    )
    .expect("extension regex")
});
static KIND_EXPLICIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:kind|type)\s*[:=]\s*(document|spreadsheet|presentation|image|picture|photo|video|music|audio)\b")
        .expect("kind regex")
});
static MODIFIED_PAST_DAYS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bmodified\s+(?:within\s+)?(?:the\s+)?past\s+(\d{1,4})\s+days?\b")
        .expect("modified past days regex")
});
static PAST_DAYS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:within\s+)?(?:the\s+)?past\s+(\d{1,4})\s+days?\b")
        .expect("past days regex")
});
static CREATED_RELATIVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bcreated\s+(today|yesterday|this\s+week|last\s+week|this\s+month|last\s+month)\b")
        .expect("created relative regex")
});
static CREATED_COMPARE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bcreated\s+(before|after|since)\s+([a-zA-Z]+\s+\d{1,2}(?:,\s*\d{4})?|\d{4}-\d{2}-\d{2}|\d{1,2}/\d{1,2}/\d{2,4})\b")
        .expect("created comparison regex")
});
static MODIFIED_COMPARE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bmodified\s+(before|after|since)\s+([a-zA-Z]+\s+\d{1,2}(?:,\s*\d{4})?|\d{4}-\d{2}-\d{2}|\d{1,2}/\d{1,2}/\d{2,4})\b")
        .expect("modified comparison regex")
});
static SIZE_GREATER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:larger|bigger|greater)\s+than\s+(\d+(?:\.\d+)?)\s*(kb|mb|gb|tb)\b|\bsize\s*(?:>|greater\s+than|over)\s*(\d+(?:\.\d+)?)\s*(kb|mb|gb|tb)\b|\bover\s+(\d+(?:\.\d+)?)\s*(kb|mb|gb|tb)\b")
        .expect("size greater regex")
});
static SIZE_LESS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:smaller|less)\s+than\s+(\d+(?:\.\d+)?)\s*(kb|mb|gb|tb)\b|\bsize\s*(?:<|less\s+than|under)\s*(\d+(?:\.\d+)?)\s*(kb|mb|gb|tb)\b|\bunder\s+(\d+(?:\.\d+)?)\s*(kb|mb|gb|tb)\b")
        .expect("size less regex")
});
static CONTENT_QUOTED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(?:containing|contains|with\s+(?:the\s+)?(?:word|phrase)|written\s+inside|inside)\s+["'“‘]([^"'”’]+)["'”’]"#)
        .expect("quoted content regex")
});
static CONTENT_UNQUOTED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(?:containing|contains|with\s+(?:the\s+)?word|written\s+inside)\s+([a-z0-9][\p{L}\p{N}_ -]{0,80}?)(?:\s+(?:modified|created|larger|bigger|smaller|less|size|from|today|yesterday|this|last|past|before|after)\b|$)"#)
        .expect("unquoted content regex")
});
static ABOUT_TOPIC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\babout\s+([a-z0-9][\p{L}\p{N}_ -]{0,80}?)(?:\s+(?:modified|created|larger|bigger|smaller|less|size|from|today|yesterday|this|last|past|before|after)\b|$)"#)
        .expect("about topic regex")
});
static FILENAME_QUOTED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(?:named|filename|file\s+name)\s*[:=]?\s*["'“‘]([^"'”’]+)["'”’]"#)
        .expect("quoted filename regex")
});
static FILENAME_UNQUOTED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(?:filename|file\s+name)\s*[:=]\s*([^\s]+)"#)
        .expect("unquoted filename regex")
});
static PHOTO_LOCATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:photos?|pictures?|images?)\s+(?:taken\s+)?in\s+([\p{L}][\p{L}\s-]{1,48})")
        .expect("photo location regex")
});

fn escape_aqs(value: &str) -> String {
    value.trim().replace('\\', "\\\\").replace('"', "\\\"")
}

fn quoted(value: &str) -> String {
    format!("\"{}\"", escape_aqs(value))
}

fn push_unique(clauses: &mut Vec<String>, clause: impl Into<String>) {
    let clause = clause.into();
    if !clause.trim().is_empty() && !clauses.iter().any(|existing| existing == &clause) {
        clauses.push(clause);
    }
}

fn date_range(property: &str, start: NaiveDate, end_inclusive: NaiveDate) -> String {
    format!(
        "{property}:>={} AND {property}:<={}",
        start.format("%Y-%m-%d"),
        end_inclusive.format("%Y-%m-%d")
    )
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

fn relative_date_clause(property: &str, phrase: &str, today: NaiveDate) -> Option<String> {
    match phrase.to_ascii_lowercase().as_str() {
        "today" => Some(date_range(property, today, today)),
        "yesterday" => {
            let day = today - Duration::days(1);
            Some(date_range(property, day, day))
        }
        "this week" => Some(date_range(property, week_start(today), today)),
        "last week" => {
            let this_monday = week_start(today);
            Some(date_range(
                property,
                this_monday - Duration::days(7),
                this_monday - Duration::days(1),
            ))
        }
        "this month" => Some(date_range(property, month_start(today)?, today)),
        "last month" => Some(date_range(
            property,
            previous_month_start(today)?,
            month_start(today)? - Duration::days(1),
        )),
        _ => None,
    }
}

fn parse_date(input: &str, today: NaiveDate) -> Option<NaiveDate> {
    let trimmed = input.trim().trim_end_matches(',');
    NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
        .ok()
        .or_else(|| NaiveDate::parse_from_str(trimmed, "%m/%d/%Y").ok())
        .or_else(|| NaiveDate::parse_from_str(trimmed, "%m/%d/%y").ok())
        .or_else(|| NaiveDate::parse_from_str(&format!("{trimmed}, {}", today.year()), "%B %d, %Y").ok())
        .or_else(|| NaiveDate::parse_from_str(&format!("{trimmed}, {}", today.year()), "%b %d, %Y").ok())
        .or_else(|| NaiveDate::parse_from_str(trimmed, "%B %d, %Y").ok())
        .or_else(|| NaiveDate::parse_from_str(trimmed, "%b %d, %Y").ok())
}

fn comparison_date_clause(property: &str, op: &str, date: NaiveDate) -> String {
    let operator = match op.to_ascii_lowercase().as_str() {
        "before" => "<",
        "after" | "since" => ">=",
        _ => ">=",
    };
    format!("{property}:{operator}{}", date.format("%Y-%m-%d"))
}

fn modified_date_clause(lower: &str, today: NaiveDate) -> Option<String> {
    if let Some(capture) = MODIFIED_PAST_DAYS.captures(lower).or_else(|| PAST_DAYS.captures(lower)) {
        let days = capture.get(1)?.as_str().parse::<i64>().ok()?.clamp(0, 3650);
        return Some(date_range("System.DateModified", today - Duration::days(days), today));
    }
    for phrase in ["today", "yesterday", "this week", "last week", "this month", "last month"] {
        if lower.contains(&format!("modified {phrase}"))
            || lower.contains(&format!("from {phrase}"))
            || lower.contains(&format!("recorded {phrase}"))
            || lower.contains(&format!("taken {phrase}"))
        {
            return relative_date_clause("System.DateModified", phrase, today);
        }
    }
    None
}

fn explicit_kind_clause(kind: &str) -> &'static str {
    match kind.to_ascii_lowercase().as_str() {
        "spreadsheet" => "System.Kind:=spreadsheet",
        "presentation" => "System.Kind:=document",
        "image" | "picture" | "photo" => "System.Kind:=picture",
        "video" => "System.Kind:=video",
        "music" | "audio" => "System.Kind:=music",
        _ => "System.Kind:=document",
    }
}

fn inferred_kind_clause(lower: &str) -> Option<&'static str> {
    if lower.contains("spreadsheet") || lower.contains("spreadsheets") {
        Some("System.Kind:=spreadsheet")
    } else if lower.contains("presentation") || lower.contains("presentations") || lower.contains("powerpoint") {
        Some("(System.FileExtension:.ppt OR System.FileExtension:.pptx)")
    } else if lower.contains("photo") || lower.contains("photos") || lower.contains("picture") || lower.contains("image") || lower.contains("images") {
        Some("System.Kind:=picture")
    } else if lower.contains("video") || lower.contains("videos") || lower.contains("movie") || lower.contains("recorded") {
        Some("System.Kind:=video")
    } else if lower.contains("music") || lower.contains("audio") || lower.contains("song") {
        Some("System.Kind:=music")
    } else if lower.contains("document") || lower.contains("documents") {
        Some("System.Kind:=document")
    } else {
        None
    }
}

fn normalized_extension(raw: &str) -> String {
    raw.to_ascii_lowercase()
}

fn size_bytes(amount: &str, unit: &str) -> Option<u64> {
    let amount = amount.parse::<f64>().ok()?;
    let multiplier = match unit.to_ascii_lowercase().as_str() {
        "kb" => 1024_f64,
        "mb" => 1024_f64.powi(2),
        "gb" => 1024_f64.powi(3),
        "tb" => 1024_f64.powi(4),
        _ => return None,
    };
    Some((amount * multiplier) as u64)
}

fn capture_amount_unit<'a>(capture: &'a regex::Captures<'a>) -> Option<(&'a str, &'a str)> {
    [(1, 2), (3, 4), (5, 6)]
        .into_iter()
        .find_map(|(amount, unit)| Some((capture.get(amount)?.as_str(), capture.get(unit)?.as_str())))
}

pub fn extension_clause_for_query(query: &str) -> Option<String> {
    let extension = EXTENSION
        .captures(query)
        .and_then(|capture| capture.get(1))
        .map(|value| normalized_extension(value.as_str()))?;
    Some(format!("System.FileExtension:.{extension}"))
}

pub fn fallback_content_aqs(query: &str) -> String {
    let mut clauses = vec![format!("System.Search.Contents:{}", quoted(query))];
    if let Some(extension) = extension_clause_for_query(query) {
        clauses.push(extension);
    }
    clauses.join(" AND ")
}

pub fn classify_query_for_aqs(query: &str) -> Option<String> {
    let query = query.trim();
    if query.is_empty() {
        return None;
    }

    let lower = query.to_lowercase();
    let today = Local::now().date_naive();
    let mut clauses = Vec::<String>::new();

    if let Some(extension) = extension_clause_for_query(query) {
        push_unique(&mut clauses, extension);
    }

    if let Some(capture) = KIND_EXPLICIT.captures(query) {
        if let Some(kind) = capture.get(1) {
            push_unique(&mut clauses, explicit_kind_clause(kind.as_str()));
        }
    } else if let Some(kind) = inferred_kind_clause(&lower) {
        if !clauses.iter().any(|clause| clause.contains("FileExtension")) {
            push_unique(&mut clauses, kind);
        }
    }

    if let Some(date) = modified_date_clause(&lower, today) {
        push_unique(&mut clauses, date);
    }
    if let Some(capture) = CREATED_RELATIVE.captures(query) {
        if let Some(clause) = capture
            .get(1)
            .and_then(|value| relative_date_clause("System.DateCreated", value.as_str(), today))
        {
            push_unique(&mut clauses, clause);
        }
    }
    if let Some(capture) = CREATED_COMPARE.captures(query) {
        if let (Some(op), Some(date)) = (
            capture.get(1),
            capture.get(2).and_then(|value| parse_date(value.as_str(), today)),
        ) {
            push_unique(&mut clauses, comparison_date_clause("System.DateCreated", op.as_str(), date));
        }
    }
    if let Some(capture) = MODIFIED_COMPARE.captures(query) {
        if let (Some(op), Some(date)) = (
            capture.get(1),
            capture.get(2).and_then(|value| parse_date(value.as_str(), today)),
        ) {
            push_unique(&mut clauses, comparison_date_clause("System.DateModified", op.as_str(), date));
        }
    }

    if let Some(capture) = SIZE_GREATER.captures(query) {
        if let Some(bytes) = capture_amount_unit(&capture).and_then(|(amount, unit)| size_bytes(amount, unit)) {
            push_unique(&mut clauses, format!("System.Size:>{bytes}"));
        }
    }
    if let Some(capture) = SIZE_LESS.captures(query) {
        if let Some(bytes) = capture_amount_unit(&capture).and_then(|(amount, unit)| size_bytes(amount, unit)) {
            push_unique(&mut clauses, format!("System.Size:<{bytes}"));
        }
    }

    if let Some(capture) = CONTENT_QUOTED
        .captures(query)
        .or_else(|| CONTENT_UNQUOTED.captures(query))
        .or_else(|| ABOUT_TOPIC.captures(query))
    {
        if let Some(phrase) = capture
            .get(1)
            .map(|value| value.as_str().trim())
            .filter(|value| !value.is_empty())
        {
            push_unique(&mut clauses, format!("System.Search.Contents:{}", quoted(phrase)));
        }
    }

    if let Some(capture) = FILENAME_QUOTED.captures(query).or_else(|| FILENAME_UNQUOTED.captures(query)) {
        if let Some(name) = capture
            .get(1)
            .map(|value| value.as_str().trim())
            .filter(|value| !value.is_empty())
        {
            push_unique(&mut clauses, format!("System.FileName:{}", quoted(name)));
        }
    }

    if let Some(capture) = PHOTO_LOCATION.captures(query) {
        if let Some(location) = capture
            .get(1)
            .map(|value| value.as_str().trim())
            .filter(|value| !value.is_empty())
        {
            push_unique(&mut clauses, format!("System.Photo.City:{}", quoted(location)));
        }
    }

    (!clauses.is_empty()).then(|| clauses.join(" AND "))
}

#[cfg(test)]
mod tests {
    use super::{classify_query_for_aqs, fallback_content_aqs};

    #[test]
    fn parses_modified_within_past_days() {
        let aqs = classify_query_for_aqs("find me a file modified within the past 2 days")
            .expect("classified");
        assert!(aqs.contains("System.DateModified:>="));
        assert!(aqs.contains("System.DateModified:<="));
    }

    #[test]
    fn combines_pdf_date_and_content() {
        let aqs = classify_query_for_aqs("pdf documents modified last week containing intrinsic")
            .expect("classified");
        assert!(aqs.contains("System.FileExtension:.pdf"));
        assert!(aqs.contains("System.DateModified"));
        assert!(aqs.contains("System.Search.Contents:\"intrinsic\""));
    }

    #[test]
    fn parses_size() {
        let aqs = classify_query_for_aqs("spreadsheets larger than 5 MB this month")
            .expect("classified");
        assert!(aqs.contains("System.Kind:=spreadsheet"));
        assert!(aqs.contains("System.Size:>5242880"));
    }

    #[test]
    fn parses_smaller_size() {
        let aqs = classify_query_for_aqs("documents smaller than 500 KB").expect("classified");
        assert!(aqs.contains("System.Kind:=document"));
        assert!(aqs.contains("System.Size:<512000"));
    }

    #[test]
    fn parses_curly_quoted_content_phrase() {
        let aqs = classify_query_for_aqs("all PDF documents with the word ‘intrinsic’ written inside")
            .expect("classified");
        assert!(aqs.contains("System.FileExtension:.pdf"));
        assert!(aqs.contains("System.Search.Contents:\"intrinsic\""));
    }

    #[test]
    fn parses_presentation_date_and_topic() {
        let aqs = classify_query_for_aqs("presentation from last week about sales")
            .expect("classified");
        assert!(aqs.contains("System.FileExtension:.ppt"));
        assert!(aqs.contains("System.DateModified"));
        assert!(aqs.contains("System.Search.Contents:\"sales\""));
    }

    #[test]
    fn parses_photo_location() {
        let aqs = classify_query_for_aqs("photos taken in Paris").expect("classified");
        assert!(aqs.contains("System.Kind:=picture"));
        assert!(aqs.contains("System.Photo.City:\"Paris\""));
    }

    #[test]
    fn parses_video_date() {
        let aqs = classify_query_for_aqs("videos I recorded yesterday").expect("classified");
        assert!(aqs.contains("System.Kind:=video"));
        assert!(aqs.contains("System.DateModified"));
    }

    #[test]
    fn parses_created_after_date() {
        let aqs = classify_query_for_aqs("created after 2026-06-01").expect("classified");
        assert!(aqs.contains("System.DateCreated:>=2026-06-01"));
    }

    #[test]
    fn fallback_keeps_extension_hint() {
        let aqs = fallback_content_aqs("pdf budget planning");
        assert!(aqs.contains("System.Search.Contents:\"pdf budget planning\""));
        assert!(aqs.contains("System.FileExtension:.pdf"));
    }
}
