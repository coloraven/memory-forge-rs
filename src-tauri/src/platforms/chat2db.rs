use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    build_commands, extract_snippet, tool_text_from_value, ContentMatch, PlatformAdapter,
    SessionDetail, SessionListItem, SessionListResult, TimelineBlock, ToolCallBlock,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chat2DbFlavor {
    Local,
    Community,
    Pro,
}

impl Chat2DbFlavor {
    pub fn platform_id(self) -> &'static str {
        match self {
            Self::Local => "chat2db-local",
            Self::Community => "chat2db-community",
            Self::Pro => "chat2db-pro",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "Chat2DB Local",
            Self::Community => "Chat2DB Community",
            Self::Pro => "Chat2DB Pro",
        }
    }
}

pub struct Chat2DbPlatform {
    flavor: Chat2DbFlavor,
    home: PathBuf,
}

#[derive(Debug, Deserialize)]
struct SessionsFile {
    #[serde(default)]
    sessions: Vec<SessionIndexEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionIndexEntry {
    id: String,
    #[serde(default)]
    user_id: i64,
    #[serde(default)]
    title: String,
    gmt_create: Option<Value>,
    gmt_modified: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct MessagesFile {
    #[serde(default)]
    messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatMessage {
    id: String,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    content: String,
    reasoning_content: Option<Value>,
    gmt_create: Option<Value>,
}

impl Chat2DbPlatform {
    pub fn new(flavor: Chat2DbFlavor, home: PathBuf) -> Self {
        Self { flavor, home }
    }

    fn history_dir(&self) -> PathBuf {
        self.home.join("ai-chat-history")
    }

    fn session_path(&self, session_id: &str) -> PathBuf {
        self.history_dir().join(format!("{session_id}.json"))
    }

    fn list_index_entries(&self) -> Vec<SessionIndexEntry> {
        let dir = self.history_dir();
        let Ok(entries) = fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.starts_with("sessions-") || !name.ends_with(".json") {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(file) = serde_json::from_str::<SessionsFile>(&raw) else {
                continue;
            };
            out.extend(file.sessions);
        }

        // Orphan session bodies missing from index
        let indexed: HashMap<String, ()> = out.iter().map(|s| (s.id.clone(), ())).collect();
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(stem) = path.file_stem().and_then(|n| n.to_str()) else {
                    continue;
                };
                if stem.starts_with("sessions-") || path.extension().and_then(|e| e.to_str()) != Some("json")
                {
                    continue;
                }
                if indexed.contains_key(stem) {
                    continue;
                }
                let Ok(raw) = fs::read_to_string(&path) else {
                    continue;
                };
                let Ok(body) = serde_json::from_str::<MessagesFile>(&raw) else {
                    continue;
                };
                let title = body
                    .messages
                    .iter()
                    .find(|m| m.role == "user" && !m.content.trim().is_empty())
                    .map(|m| truncate_title(&m.content))
                    .unwrap_or_else(|| stem.to_string());
                let gmt = body.messages.last().and_then(|m| m.gmt_create.clone());
                out.push(SessionIndexEntry {
                    id: stem.to_string(),
                    user_id: 0,
                    title,
                    gmt_create: gmt.clone(),
                    gmt_modified: gmt,
                });
            }
        }

        out.sort_by_key(|item| std::cmp::Reverse(gmt_to_millis(item.gmt_modified.as_ref())));
        out
    }

    fn read_messages(&self, session_id: &str) -> Result<MessagesFile, String> {
        let path = self.session_path(session_id);
        let raw = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read Chat2DB session '{}': {e}", path.display()))?;
        serde_json::from_str(&raw)
            .map_err(|e| format!("Failed to parse Chat2DB session '{session_id}': {e}"))
    }

    fn write_json_atomic(path: &Path, value: &Value) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create Chat2DB directory '{}': {e}", parent.display()))?;
        }
        let tmp = path.with_extension("json.tmp");
        let serialized = serde_json::to_string_pretty(value)
            .map_err(|e| format!("Failed to serialize Chat2DB JSON: {e}"))?;
        {
            let mut file = fs::File::create(&tmp)
                .map_err(|e| format!("Failed to create temp file '{}': {e}", tmp.display()))?;
            file.write_all(serialized.as_bytes())
                .map_err(|e| format!("Failed to write temp file '{}': {e}", tmp.display()))?;
            file.sync_all().ok();
        }
        fs::rename(&tmp, path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!(
                "Failed to replace Chat2DB file '{}': {e}. Close Chat2DB and try again.",
                path.display()
            )
        })?;
        Ok(())
    }

    fn update_index_title(&self, session_id: &str, title: &str) -> Result<(), String> {
        let dir = self.history_dir();
        let Ok(entries) = fs::read_dir(&dir) else {
            return Ok(());
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.starts_with("sessions-") || !name.ends_with(".json") {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(mut root) = serde_json::from_str::<Value>(&raw) else {
                continue;
            };
            let Some(sessions) = root.get_mut("sessions").and_then(Value::as_array_mut) else {
                continue;
            };
            let mut touched = false;
            for session in sessions.iter_mut() {
                if session.get("id").and_then(Value::as_str) == Some(session_id) {
                    session["title"] = Value::String(title.to_string());
                    session["gmtModified"] = now_gmt_array();
                    touched = true;
                    break;
                }
            }
            if touched {
                return Self::write_json_atomic(&path, &root);
            }
        }
        Ok(())
    }
}

impl PlatformAdapter for Chat2DbPlatform {
    fn list_sessions(
        &self,
        alias_map: &HashMap<String, String>,
        limit: Option<usize>,
        offset: usize,
    ) -> SessionListResult {
        if !self.history_dir().is_dir() {
            return SessionListResult {
                total: 0,
                items: Vec::new(),
            };
        }

        let entries = self.list_index_entries();
        let total = entries.len();
        let items = entries
            .into_iter()
            .skip(offset)
            .take(limit.unwrap_or(usize::MAX))
            .map(|entry| {
                let alias = alias_map.get(&entry.id).cloned().unwrap_or_default();
                let display_title = if alias.is_empty() {
                    if entry.title.trim().is_empty() {
                        entry.id.clone()
                    } else {
                        entry.title.clone()
                    }
                } else {
                    alias.clone()
                };
                SessionListItem {
                    platform: self.flavor.platform_id().to_string(),
                    session_key: entry.id.clone(),
                    session_id: entry.id.clone(),
                    display_title,
                    alias_title: alias,
                    preview: entry.title,
                    updated_at: gmt_to_millis(entry.gmt_modified.as_ref()).to_string(),
                    cwd: self.home.display().to_string(),
                    editable: true,
                    content_matches: Vec::new(),
                    total_content_matches: 0,
                    favorite: false,
                    agent_group: None,
                }
            })
            .collect();

        SessionListResult { total, items }
    }

    fn get_session_detail(
        &self,
        session_key: &str,
        alias_map: &HashMap<String, String>,
    ) -> Result<SessionDetail, String> {
        let body = self.read_messages(session_key)?;
        let index_title = self
            .list_index_entries()
            .into_iter()
            .find(|entry| entry.id == session_key)
            .map(|entry| entry.title)
            .unwrap_or_default();

        let mut blocks = Vec::new();
        for message in &body.messages {
            let role = match message.role.as_str() {
                "user" => "user",
                "assistant" => "assistant",
                other if !other.is_empty() => other,
                _ => continue,
            };

            let reasoning_parts =
                parse_reasoning_timeline(&message.id, message.reasoning_content.as_ref());
            let mut thinking_index = 0usize;
            let mut tool_group_index = 0usize;
            let mut emitted_from_reasoning = false;

            for part in reasoning_parts {
                match part {
                    ReasoningPart::Thinking(text) => {
                        thinking_index += 1;
                        emitted_from_reasoning = true;
                        blocks.push(TimelineBlock {
                            id: format!("{}:thinking:{}", message.id, thinking_index),
                            role: "thinking".to_string(),
                            content: text,
                            editable: false,
                            edit_target: String::new(),
                            source_meta: json!({
                                "messageId": message.id,
                                "sessionId": session_key,
                                "flavor": self.flavor.platform_id(),
                                "field": "reasoningContent",
                                "part": "reasoning",
                                "index": thinking_index,
                            }),
                            tool_calls: Vec::new(),
                        });
                    }
                    ReasoningPart::Tools(tools) => {
                        if tools.is_empty() {
                            continue;
                        }
                        tool_group_index += 1;
                        emitted_from_reasoning = true;
                        blocks.push(TimelineBlock {
                            id: format!("{}:tools:{}", message.id, tool_group_index),
                            role: "assistant".to_string(),
                            content: String::new(),
                            editable: false,
                            edit_target: String::new(),
                            source_meta: json!({
                                "messageId": message.id,
                                "sessionId": session_key,
                                "flavor": self.flavor.platform_id(),
                                "field": "reasoningContent",
                                "part": "tool_result",
                                "index": tool_group_index,
                            }),
                            tool_calls: tools,
                        });
                    }
                }
            }

            if message.content.trim().is_empty() {
                if !emitted_from_reasoning {
                    continue;
                }
            } else {
                blocks.push(TimelineBlock {
                    id: message.id.clone(),
                    role: role.to_string(),
                    content: message.content.clone(),
                    editable: true,
                    edit_target: format!("{session_key}::{}::content", message.id),
                    source_meta: json!({
                        "messageId": message.id,
                        "sessionId": session_key,
                        "flavor": self.flavor.platform_id(),
                        "gmtCreate": message.gmt_create,
                        "userId": message.session_id,
                    }),
                    tool_calls: Vec::new(),
                });
            }
        }

        let alias = alias_map.get(session_key).cloned().unwrap_or_default();
        let title = if alias.is_empty() {
            if index_title.trim().is_empty() {
                session_key.to_string()
            } else {
                index_title
            }
        } else {
            alias.clone()
        };

        Ok(SessionDetail {
            platform: self.flavor.platform_id().to_string(),
            session_key: session_key.to_string(),
            session_id: session_key.to_string(),
            title,
            alias_title: alias,
            cwd: self.home.display().to_string(),
            commands: build_commands(self.flavor.platform_id(), session_key),
            blocks,
        })
    }

    fn update_message(&self, edit_target: &str, new_content: &str) -> Result<String, String> {
        let parts: Vec<&str> = edit_target.splitn(3, "::").collect();
        if parts.len() != 3 {
            return Err(format!("Invalid Chat2DB edit target: {edit_target}"));
        }
        let (session_id, message_id, field) = (parts[0], parts[1], parts[2]);
        if field != "content" {
            return Err(format!("Unsupported Chat2DB edit field: {field}"));
        }

        let path = self.session_path(session_id);
        let raw = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read Chat2DB session for edit: {e}"))?;
        let mut root: Value = serde_json::from_str(&raw)
            .map_err(|e| format!("Failed to parse Chat2DB session for edit: {e}"))?;
        let messages = root
            .get_mut("messages")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| "Chat2DB session is missing messages[]".to_string())?;

        let mut old_content = String::new();
        let mut found = false;
        for message in messages.iter_mut() {
            if message.get("id").and_then(Value::as_str) != Some(message_id) {
                continue;
            }
            old_content = message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            message["content"] = Value::String(new_content.to_string());
            found = true;
            break;
        }
        if !found {
            return Err(format!("Chat2DB message not found: {message_id}"));
        }

        Self::write_json_atomic(&path, &root)?;

        // Keep sidebar title in sync when editing the first user message.
        if let Ok(body) = self.read_messages(session_id) {
            if body
                .messages
                .iter()
                .find(|m| m.role == "user")
                .map(|m| m.id.as_str())
                == Some(message_id)
            {
                let _ = self.update_index_title(session_id, &truncate_title(new_content));
            }
        }

        Ok(old_content)
    }

    fn matches_query(&self, session_key: &str, query: &str) -> bool {
        !self.content_search(session_key, query).is_empty()
    }

    fn content_search(&self, session_key: &str, query: &str) -> Vec<ContentMatch> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        let Ok(body) = self.read_messages(session_key) else {
            return Vec::new();
        };

        let mut matches = Vec::new();
        for (index, message) in body.messages.iter().enumerate() {
            let mut haystacks = vec![message.content.clone()];
            if let Some(rc) = message.reasoning_content.as_ref() {
                haystacks.push(reasoning_search_text(rc));
            }
            for text in haystacks {
                if text.to_lowercase().contains(&needle) {
                    matches.push(ContentMatch {
                        snippet: extract_snippet(&text, &needle),
                        match_index: index,
                        role: message.role.clone(),
                    });
                    break;
                }
            }
        }
        matches
    }
}

pub fn default_chat2db_local_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".chat2db_local_edition")
}

pub fn default_chat2db_community_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".chat2db-community")
}

pub fn default_chat2db_pro_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".chat2db")
}

fn truncate_title(content: &str) -> String {
    let trimmed = content.trim().replace('\n', " ");
    let mut chars = trimmed.chars();
    let short: String = chars.by_ref().take(48).collect();
    if chars.next().is_some() {
        format!("{short}…")
    } else {
        short
    }
}

fn gmt_to_millis(value: Option<&Value>) -> i64 {
    let Some(arr) = value.and_then(Value::as_array) else {
        return 0;
    };
    let year = arr.first().and_then(Value::as_i64).unwrap_or(1970) as i32;
    let month = arr.get(1).and_then(Value::as_i64).unwrap_or(1) as u32;
    let day = arr.get(2).and_then(Value::as_i64).unwrap_or(1) as u32;
    let hour = arr.get(3).and_then(Value::as_i64).unwrap_or(0) as u32;
    let minute = arr.get(4).and_then(Value::as_i64).unwrap_or(0) as u32;
    let second = arr.get(5).and_then(Value::as_i64).unwrap_or(0) as u32;
    let nano = arr.get(6).and_then(Value::as_i64).unwrap_or(0).clamp(0, 999_999_999) as u32;

    let Some(date) = NaiveDate::from_ymd_opt(year, month, day) else {
        return 0;
    };
    let Some(time) = NaiveTime::from_hms_nano_opt(hour, minute, second, nano) else {
        return 0;
    };
    let naive = NaiveDateTime::new(date, time);
    Local
        .from_local_datetime(&naive)
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

fn now_gmt_array() -> Value {
    let now = Local::now();
    json!([
        now.year(),
        now.month(),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.nanosecond()
    ])
}

fn reasoning_search_text(value: &Value) -> String {
    match value {
        Value::String(text) => {
            if let Ok(items) = serde_json::from_str::<Vec<Value>>(text.trim()) {
                items
                    .iter()
                    .filter_map(|item| {
                        item.get("content")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                text.clone()
            }
        }
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                item.get("content")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.to_string(),
    }
}

enum ReasoningPart {
    Thinking(String),
    Tools(Vec<ToolCallBlock>),
}

fn parse_reasoning_items(value: Option<&Value>) -> Vec<Value> {
    let Some(value) = value else {
        return Vec::new();
    };
    match value {
        Value::Array(arr) => arr.clone(),
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Vec::new();
            }
            if trimmed.starts_with('[') {
                serde_json::from_str(trimmed).unwrap_or_else(|_| {
                    // Plain text that happens to start with '[' — treat as single reasoning blob.
                    vec![json!({
                        "type": "reasoning",
                        "messageType": "reasoning",
                        "content": trimmed,
                    })]
                })
            } else {
                vec![json!({
                    "type": "reasoning",
                    "messageType": "reasoning",
                    "content": trimmed,
                })]
            }
        }
        Value::Null => Vec::new(),
        other => vec![json!({
            "type": "reasoning",
            "messageType": "reasoning",
            "content": other.to_string(),
        })],
    }
}

fn item_is_reasoning(item: &Value) -> bool {
    let kind = item
        .get("type")
        .or_else(|| item.get("messageType"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    if kind == "reasoning" || kind == "thinking" {
        return true;
    }
    // Tool results always carry a tool name; reasoning items usually do not.
    if item.get("name").and_then(Value::as_str).filter(|n| !n.is_empty()).is_some() {
        return false;
    }
    kind.is_empty() && item.get("content").and_then(Value::as_str).is_some()
}

fn parse_reasoning_timeline(message_id: &str, value: Option<&Value>) -> Vec<ReasoningPart> {
    let items = parse_reasoning_items(value);
    if items.is_empty() {
        return Vec::new();
    }

    let mut parts: Vec<ReasoningPart> = Vec::new();
    let mut thinking_buf = String::new();
    let mut tools_buf: Vec<ToolCallBlock> = Vec::new();
    let mut tool_index = 0usize;

    let flush_thinking = |buf: &mut String, parts: &mut Vec<ReasoningPart>| {
        let text = std::mem::take(buf).trim().to_string();
        if !text.is_empty() {
            parts.push(ReasoningPart::Thinking(text));
        }
    };
    let flush_tools = |buf: &mut Vec<ToolCallBlock>, parts: &mut Vec<ReasoningPart>| {
        if !buf.is_empty() {
            parts.push(ReasoningPart::Tools(std::mem::take(buf)));
        }
    };

    for item in items {
        if item_is_reasoning(&item) {
            flush_tools(&mut tools_buf, &mut parts);
            let text = item
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if text.is_empty() {
                continue;
            }
            if !thinking_buf.is_empty() {
                thinking_buf.push_str("\n\n");
            }
            thinking_buf.push_str(text);
            continue;
        }

        flush_thinking(&mut thinking_buf, &mut parts);
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("tool")
            .to_string();
        let output = item
            .get("content")
            .and_then(|v| tool_text_from_value(v, 32768));
        let kind = item
            .get("type")
            .or_else(|| item.get("messageType"))
            .and_then(Value::as_str)
            .unwrap_or("tool_result")
            .to_string();
        tools_buf.push(ToolCallBlock {
            id: format!("{message_id}:tool:{tool_index}"),
            name,
            kind,
            status: "completed".to_string(),
            input: None,
            output,
            error: None,
            started_at: None,
            ended_at: None,
            source_meta: json!({
                "messageId": message_id,
                "toolIndex": tool_index,
                "source": "reasoningContent",
            }),
        });
        tool_index += 1;
    }

    flush_thinking(&mut thinking_buf, &mut parts);
    flush_tools(&mut tools_buf, &mut parts);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gmt_array_converts_to_millis() {
        let value = json!([2026, 9, 5, 11, 30, 51, 0]);
        assert!(gmt_to_millis(Some(&value)) > 0);
    }

    #[test]
    fn reasoning_tools_parse_from_string_array() {
        let raw = json!(
            "[{\"type\":\"tool_result\",\"name\":\"list_all_tables\",\"content\":\"a [TABLE]\"}]"
        );
        let parts = parse_reasoning_timeline("m1", Some(&raw));
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            ReasoningPart::Tools(tools) => {
                assert_eq!(tools.len(), 1);
                assert_eq!(tools[0].name, "list_all_tables");
                assert_eq!(tools[0].output.as_deref(), Some("a [TABLE]"));
            }
            ReasoningPart::Thinking(_) => panic!("expected tools"),
        }
    }

    #[test]
    fn plain_reasoning_text_becomes_thinking() {
        let value = json!("planning next SQL");
        let parts = parse_reasoning_timeline("m1", Some(&value));
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            ReasoningPart::Thinking(text) => assert_eq!(text, "planning next SQL"),
            ReasoningPart::Tools(_) => panic!("expected thinking"),
        }
    }

    #[test]
    fn pro_interleaved_reasoning_and_tools() {
        let raw = json!([
            {
                "type": "reasoning",
                "messageType": "reasoning",
                "content": "first thought"
            },
            {
                "type": "tool_result",
                "messageType": "tool_result",
                "name": "execute_sql",
                "content": "CORRUPT"
            },
            {
                "type": "reasoning",
                "messageType": "reasoning",
                "content": "second thought"
            },
            {
                "type": "tool_result",
                "name": "list_all_datasources",
                "content": "id=1"
            }
        ]);
        let parts = parse_reasoning_timeline("m1", Some(&raw));
        assert_eq!(parts.len(), 4);
        assert!(matches!(&parts[0], ReasoningPart::Thinking(t) if t == "first thought"));
        assert!(matches!(&parts[1], ReasoningPart::Tools(t) if t.len() == 1 && t[0].name == "execute_sql"));
        assert!(matches!(&parts[2], ReasoningPart::Thinking(t) if t == "second thought"));
        assert!(matches!(&parts[3], ReasoningPart::Tools(t) if t.len() == 1 && t[0].name == "list_all_datasources"));
    }

    #[test]
    fn consecutive_reasoning_merged() {
        let raw = json!([
            {"type":"reasoning","content":"a"},
            {"type":"reasoning","content":"b"},
            {"type":"tool_result","name":"t","content":"ok"}
        ]);
        let parts = parse_reasoning_timeline("m1", Some(&raw));
        assert_eq!(parts.len(), 2);
        assert!(matches!(&parts[0], ReasoningPart::Thinking(t) if t == "a\n\nb"));
        assert!(matches!(&parts[1], ReasoningPart::Tools(t) if t.len() == 1));
    }
}
