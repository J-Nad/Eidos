use std::sync::LazyLock;

use chrono::{Datelike, Duration, Local, NaiveDate, Weekday};
use regex::Regex;

static EXTENSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:\*\.|\btype\s*:\s*|\b)(pdf|docx?|xlsx?|csv|pptx?|txt|md|png|jpe?g|gif|webp|mp4|mov|avi|mkv|mp3|wav)\b")
        .expect("extension regex")
});
static SIZE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:larger\s+than|bigger\s+than|size\s*>|over)\s*(\d+(?:\.\d+)?)\s*(kb|mb|gb|tb)\b")
        .expect("size regex")
});
static CONTENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:containing|contains|with\s+(?:the\s+)?(?:word|phrase)|about)\s+[\"'“‘]?([^\"'”’]+?)[\"'”’]?(?:\s+written\s+inside)?(?:\s+(?:modified|from|larger|bigger|size)\b|$)"#)
        .expect("content regex")
});
static PHOTO_LOCATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:photos?|pictures?|images?)\s+(?:taken\s+)?in\s+([\p{L}][\p{L}\s-]{1,48})")
        .expect("photo location regex")
});

fn quoted(value: &str) -> String {
    format!("\"{}\"", value.trim().replace('"', "\""))
}

fn date_range(property: &str, start: NaiveDate, end: NaiveDate) -> String {
    format!(
        "{property}:>={} AND {property}:<{}",
        start.format("%Y-%m-%d"),
        end.format("%Y-%m-%d")
    )
}

fn date_clause(lower: &str, today: NaiveDate) -> Option<String> {
    if lower.contains("yesterday") {
        let day = today - Duration::days(1);
        return Some(date_range("System.DateModified", day, today));
    }
    if lower.contains("today") {
        return Some(date_range(
            "System.DateModified",
            today,
            today + Duration::days(1),
        ));
    }
    if lower.contains("last week") {
        let days_from_monday = match today.weekday() {
            Weekday::Mon => 0,
            Weekday::Tue => 1,
            Weekday::Wed => 2,
            Weekday::Thu => 3,
            Weekday::Fri => 4,
            Weekday::Sat => 5,
            Weekday::Sun => 6,
        };
        let this_monday = today - Duration::days(days_from_monday);
        let start = this_monday - Duration::days(7);
        return Some(date_range("System.DateModified", start, this_monday));
    }
    if lower.contains("this week") {
        let days_from_monday = today.weekday().num_days_from_monday() as i64;
        let start = today - Duration::days(days_from_monday);
        return Some(date_range(
            "System.DateModified",
            start,
            today + Duration::days(1),
        ));
    }
    if lower.contains("this month") {
        let start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1)?;
        let (year, month) = if today.month() == 12 {
            (today.year() + 1, 1)
        } else {
            (today.year(), today.month() + 1)
        };
        let end = NaiveDate::from_ymd_opt(year, month, 1)?;
        return Some(date_range("System.DateModified", start, end));
    }
    None
}

fn kind_clause(lower: &str) -> Option<&'static str> {
    if lower.contains("spreadsheet") || lower.contains("spreadsheets") {
        Some("System.Kind:=spreadsheet")
    } else if lower.contains("presentation") || lower.contains("presentations") {
        Some("(System.FileExtension:.ppt OR System.FileExtension:.pptx)")
    } else if lower.contains("photo") || lower.contains("picture") || lower.contains("image") {
        Some("System.Kind:=picture")
    } else if lower.contains("video") || lower.contains("movie") {
        Some("System.Kind:=video")
    } else if lower.contains("music") || lower.contains("audio") || lower.contains("song") {
        Some("System.Kind:=music")
    } else if lower.contains("document") || lower.contains("documents") {
        Some("System.Kind:=document")
    } else {
        None
    }
}

pub fn classify_query_for_aqs(query: &str) -> Option<String> {
    let query = query.trim();
    if query.is_empty() {
        return None;
    }
    let lower = query.to_lowercase();
    let mut clauses = Vec::<String>::new();

    if let Some(capture) = EXTENSION.captures(query) {
        let extension = capture.get(1)?.as_str().to_ascii_lowercase();
        clauses.push(format!("System.FileExtension:.{extension}"));
    }
    if let Some(kind) = kind_clause(&lower) {
        if !clauses.iter().any(|clause| clause.contains("FileExtension")) {
            clauses.push(kind.to_string());
        }
    }
    if let Some(date) = date_clause(&lower, Local::now().date_naive()) {
        clauses.push(date);
    }
    if let Some(capture) = SIZE.captures(query) {
        let amount = capture.get(1)?.as_str().parse::<f64>().ok()?;
        let multiplier = match capture.get(2)?.as_str().to_ascii_lowercase().as_str() {
            "kb" => 1024_f64,
            "mb" => 1024_f64.powi(2),
            "gb" => 1024_f64.powi(3),
            "tb" => 1024_f64.powi(4),
            _ => return None,
        };
        clauses.push(format!("System.Size:>{}", (amount * multiplier) as u64));
    }
    if let Some(capture) = CONTENT.captures(query) {
        let phrase = capture.get(1)?.as_str().trim();
        if !phrase.is_empty() {
            clauses.push(format!("System.Search.Contents:{}", quoted(phrase)));
        }
    }
    if let Some(capture) = PHOTO_LOCATION.captures(query) {
        let location = capture.get(1)?.as_str().trim();
        if !location.is_empty() {
            clauses.push(format!("System.Photo.City:{}", quoted(location)));
        }
    }

    (!clauses.is_empty()).then(|| clauses.join(" AND "))
}

#[cfg(test)]
mod tests {
    use super::classify_query_for_aqs;

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
        assert!(aqs.contains("System.FileExtension:.pptx"));
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
}
