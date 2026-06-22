use futures_util::StreamExt;
use chrono::Datelike;
use serde_json::{json, Value};
use tauri::{Emitter, State, WebviewWindow};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::state::{AppState, GEMINI_MODEL, GEMINI_STREAM_ENDPOINT};
use crate::search::metadata::MetadataFilters;

const FILE_ASSISTANT_SYSTEM_PROMPT: &str = r#"You are a helpful assistant integrated into a Windows file manager.
You behave like a precise coding copilot: inspect the current file and attached context before answering.
Never invent file contents or claim that a change was saved.
In ASK mode, explain briefly and do not return a replacement file unless explicitly requested.
In EDIT or AGENT mode, when proposing a complete file replacement, put only the final file contents between
<eidos_file> and </eidos_file>. Put any short explanation outside those tags.
Preserve unrelated code, formatting, imports, and comments. Return complete, runnable content rather than fragments.
For a newly created empty file, generate a complete useful implementation appropriate to its extension."#;

const METADATA_FILTER_SYSTEM_PROMPT: &str = r#"You are a file search intent parser. Convert the user's query into a JSON object with the following optional fields:
- "extensions": array of lowercase extensions without dot (e.g., ["pdf","docx"])
- "kind": one of "document","spreadsheet","image","video","audio","code","archive","folder"
- "size_min": integer, bytes, minimum file size
- "size_max": integer, bytes, maximum file size
- "modified_after": ISO date string (YYYY-MM-DD) for the start of modified date range
- "modified_before": ISO date string for the end of modified date range
- "created_after": ISO date string
- "created_before": ISO date string
- "name_query": a string that is the likely filename or keyword to search for
- "has_content_intent": true only when the query explicitly asks for text inside files

Rules:
- Today is {{TODAY}}. Compute all relative dates (like "this week", "yesterday") accordingly.
- "pdf files" -> extensions: ["pdf"]
- "images from yesterday" -> kind: "image", modified_after: {{YESTERDAY}}, modified_before: {{TODAY}}
- "spreadsheets larger than 2 MB modified this week" -> kind: "spreadsheet", size_min: 2097152, modified_after: {{WEEK_START}}, modified_before: {{TODAY}}
- "find me the budget report" -> name_query: "budget report"
- If you can't determine any filter, just set name_query to the whole query.
- For "containing", "with the phrase", "inside", or a quoted phrase, set has_content_intent=true and do not put the content phrase in name_query. Content search is handled separately.

Return ONLY the JSON. No other text."#;

pub async fn translate_metadata_filters(
    state: &AppState,
    query: &str,
    today: &str,
) -> Result<MetadataFilters, String> {
    let api_key = state
        .gemini_api_key
        .read()
        .await
        .clone()
        .ok_or_else(|| "AI not configured".to_string())?;
    let today_date = chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d")
        .map_err(|error| format!("Invalid prompt date: {error}"))?;
    let yesterday = today_date - chrono::Duration::days(1);
    let week_start = today_date - chrono::Duration::days(today_date.weekday().num_days_from_monday() as i64);
    let system_prompt = METADATA_FILTER_SYSTEM_PROMPT
        .replace("{{TODAY}}", today)
        .replace("{{YESTERDAY}}", &yesterday.format("%Y-%m-%d").to_string())
        .replace("{{WEEK_START}}", &week_start.format("%Y-%m-%d").to_string());
    let endpoint = GEMINI_STREAM_ENDPOINT.replace(":streamGenerateContent", ":generateContent");
    let response = state
        .http
        .post(endpoint)
        .query(&[("key", api_key.as_str())])
        .json(&json!({
            "contents": [{
                "parts": [
                    { "text": system_prompt },
                    { "text": format!("User: {query}") }
                ]
            }],
            "generationConfig": {
                "temperature": 0.0,
                "maxOutputTokens": 512,
                "responseMimeType": "application/json"
            },
            "safetySettings": []
        }))
        .send()
        .await
        .map_err(|error| format!("Could not reach Gemini for metadata parsing: {error}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(format!("Gemini metadata parsing returned {status}: {detail}"));
    }
    let payload: Value = response
        .json()
        .await
        .map_err(|error| format!("Gemini returned invalid metadata parsing JSON: {error}"))?;
    let text = payload
        .pointer("/candidates/0/content/parts/0/text")
        .and_then(Value::as_str)
        .ok_or_else(|| "Gemini returned no metadata parsing result.".to_string())?
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str::<MetadataFilters>(text)
        .map(MetadataFilters::normalized)
        .map_err(|error| format!("Gemini metadata response did not match the filter schema: {error}"))
}

fn consume_gemini_event(
    line: &[u8],
    window: &WebviewWindow,
    output: &mut String,
) -> Result<(), String> {
    let line = String::from_utf8_lossy(line);
    let Some(data) = line.trim().strip_prefix("data:") else {
        return Ok(());
    };
    let data = data.trim();
    if data.is_empty() {
        return Ok(());
    }
    let value: Value = serde_json::from_str(data)
        .map_err(|error| format!("Gemini returned an invalid stream event: {error}"))?;
    if let Some(message) = value.pointer("/error/message").and_then(Value::as_str) {
        return Err(format!("Gemini rejected the request: {message}"));
    }
    if let Some(reason) = value
        .pointer("/promptFeedback/blockReason")
        .and_then(Value::as_str)
    {
        return Err(format!("Gemini blocked the prompt: {reason}"));
    }
    let Some(token) = value
        .pointer("/candidates/0/content/parts/0/text")
        .and_then(Value::as_str)
    else {
        return Ok(());
    };
    output.push_str(token);
    window
        .emit("ai-token", token.to_string())
        .map_err(|error| error.to_string())
}

pub async fn stream_chat(
    state: State<'_, AppState>,
    window: WebviewWindow,
    file_path: Option<String>,
    file_content: Option<String>,
    message: String,
    mode: Option<String>,
    context: Option<String>,
) -> Result<(), String> {
    if state.gemini_api_key.read().await.is_none() {
        return Err("AI not configured. Set GEMINI_API_KEY in Settings or as an environment variable.".to_string());
    }
    let path = file_path.unwrap_or_else(|| "New unsaved file".to_string());
    let mode = mode.unwrap_or_else(|| "agent".to_string()).to_ascii_uppercase();
    let current = file_content.unwrap_or_else(|| "(The file is empty or unreadable.)".to_string());
    let context = context.unwrap_or_default();
    let prompt = format!(
        "{FILE_ASSISTANT_SYSTEM_PROMPT}\n\nMODE: {mode}\nTARGET PATH: {path}\n\nCURRENT FILE:\n{current}\n\nATTACHED CONTEXT:\n{}\n\nUSER INSTRUCTION:\n{message}",
        if context.trim().is_empty() { "(None)" } else { context.as_str() }
    );
    stream_prompt(state, window, prompt, 0.15, 4096).await
}

async fn stream_prompt(
    state: State<'_, AppState>,
    window: WebviewWindow,
    prompt: String,
    temperature: f32,
    max_output_tokens: u32,
) -> Result<(), String> {
    let api_key = state
        .gemini_api_key
        .read()
        .await
        .clone()
        .ok_or_else(|| "AI not configured. Set GEMINI_API_KEY in Settings or as an environment variable.".to_string())?;
    let stream_id = Uuid::new_v4().to_string();
    let (abort_sender, mut abort_receiver) = oneshot::channel();
    state
        .abort_handles
        .lock()
        .await
        .insert(stream_id.clone(), abort_sender);
    window
        .emit("ai-stream-start", json!({ "streamId": stream_id, "model": GEMINI_MODEL }))
        .map_err(|error| error.to_string())?;

    let request = state
        .http
        .post(GEMINI_STREAM_ENDPOINT)
        .query(&[("alt", "sse"), ("key", api_key.as_str())])
        .json(&json!({
            "contents": [{
                "parts": [{ "text": prompt }]
            }],
            "generationConfig": {
                "temperature": temperature,
                "maxOutputTokens": max_output_tokens
            },
            "safetySettings": []
        }));

    let response_result = tokio::select! {
        _ = &mut abort_receiver => {
            state.abort_handles.lock().await.remove(&stream_id);
            window.emit("ai-stream-end", json!({ "streamId": stream_id, "content": "", "cancelled": true })).map_err(|error| error.to_string())?;
            return Ok(());
        }
        response = request.send() => response.map_err(|error| format!("Could not reach Gemini: {error}")),
    };
    let response = match response_result {
        Ok(response) => response,
        Err(error) => {
            state.abort_handles.lock().await.remove(&stream_id);
            return Err(error);
        }
    };
    if !response.status().is_success() {
        let status = response.status();
        let detail = response
            .text()
            .await
            .unwrap_or_else(|_| "No response body".to_string());
        state.abort_handles.lock().await.remove(&stream_id);
        return Err(format!("Gemini {GEMINI_MODEL} returned {status}: {detail}"));
    }

    let mut stream = response.bytes_stream();
    let mut pending = Vec::<u8>::new();
    let mut output = String::new();
    let mut cancelled = false;
    let stream_result = 'streaming: loop {
        tokio::select! {
            _ = &mut abort_receiver => {
                cancelled = true;
                break Ok(());
            }
            next = stream.next() => {
                let Some(next) = next else { break Ok(()) };
                match next {
                    Ok(bytes) => {
                        pending.extend_from_slice(&bytes);
                        while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
                            let mut line = pending.drain(..=newline).collect::<Vec<_>>();
                            while matches!(line.last(), Some(b'\n' | b'\r')) { line.pop(); }
                            if let Err(error) = consume_gemini_event(&line, &window, &mut output) {
                                break 'streaming Err(error);
                            }
                        }
                    }
                    Err(error) => break 'streaming Err(format!("Gemini stream interrupted: {error}")),
                }
            }
        }
    };
    if stream_result.is_ok() && !pending.is_empty() && !cancelled {
        consume_gemini_event(&pending, &window, &mut output)?;
    }
    state.abort_handles.lock().await.remove(&stream_id);
    stream_result?;
    window
        .emit("ai-stream-end", json!({ "streamId": stream_id, "content": output, "cancelled": cancelled }))
        .map_err(|error| error.to_string())
}
