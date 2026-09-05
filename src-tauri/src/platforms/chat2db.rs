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

            let tool_calls = parse_reasoning_tools(&message.id, message.reasoning_content.as_ref());
            let thinking = reasoning_as_thinking(message.reasoning_content.as_ref());
            if let Some(thinking_text) = thinking {
                blocks.push(TimelineBlock {
                    id: format!("{}:thinking", message.id),
                    role: "thinking".to_string(),
                    content: thinking_text,
                    editable: false,
                    edit_target: String::new(),
                    source_meta: json!({
                        "messageId": message.id,
                        "sessionId": session_key,
                        "flavor": self.flavor.platform_id(),
                        "field": "reasoningContent",
                    }),
                    tool_calls: Vec::new(),
                });
            }

            if message.content.trim().is_empty() && tool_calls.is_empty() {
                continue;
            }

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
                tool_calls,
            });
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
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn reasoning_as_thinking(value: Option<&Value>) -> Option<String> {
    let value = value?;
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() || trimmed.starts_with('[') {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Null => None,
        other => {
            // Non-array structured values are uncommon; skip tool arrays.
            if other.as_array().is_some() {
                None
            } else {
                Some(other.to_string())
            }
        }
    }
}

fn parse_reasoning_tools(message_id: &str, value: Option<&Value>) -> Vec<ToolCallBlock> {
    let Some(value) = value else {
        return Vec::new();
    };

    let items: Vec<Value> = match value {
        Value::Array(arr) => arr.clone(),
        Value::String(text) => {
            let trimmed = text.trim();
            if !trimmed.starts_with('[') {
                return Vec::new();
            }
            serde_json::from_str(trimmed).unwrap_or_default()
        }
        _ => return Vec::new(),
    };

    let mut tools = Vec::new();
    for (index, item) in items.into_iter().enumerate() {
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
        tools.push(ToolCallBlock {
            id: format!("{message_id}:tool:{index}"),
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
                "toolIndex": index,
                "source": "reasoningContent",
            }),
        });
    }
    tools
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
        let tools = parse_reasoning_tools("m1", Some(&raw));
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "list_all_tables");
        assert_eq!(tools[0].output.as_deref(), Some("a [TABLE]"));
    }

    #[test]
    fn plain_reasoning_text_becomes_thinking() {
        let value = json!("planning next SQL");
        assert_eq!(
            reasoning_as_thinking(Some(&value)).as_deref(),
            Some("planning next SQL")
        );
        assert!(parse_reasoning_tools("m1", Some(&value)).is_empty());
    }
}
