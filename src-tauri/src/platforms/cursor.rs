use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, Row};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    build_commands, content_entries_to_matches, push_bounded_index_entry, ContentMatch,
    PlatformAdapter, SessionDetail, SessionKey, SessionListItem, SessionListResult, TimelineBlock,
    ToolCallBlock,
};
use crate::app_log;
use crate::database::{
    SessionContentEntry, SessionContentIndex, SessionSummaryCache, SessionSummaryFingerprint,
};

const COMPOSER_HEADERS_KEY: &str = "composer.composerHeaders";
const WORKSPACE_COMPOSER_DATA_KEY: &str = "composer.composerData";
const TRANSCRIPT_SESSION_PREFIX: &str = "transcript:";

pub struct CursorPlatform {
    cursor_home: PathBuf,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CursorComposerHeaders {
    #[serde(default)]
    all_composers: Vec<CursorComposerHeader>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CursorComposerHeader {
    #[serde(default)]
    composer_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    subtitle: String,
    created_at: Option<i64>,
    last_updated_at: Option<i64>,
    #[serde(default)]
    is_draft: bool,
    #[serde(default)]
    is_archived: bool,
    #[serde(default)]
    subagent_info: Option<CursorSubagentInfo>,
    workspace_identifier: Option<Value>,
    agent_location: Option<Value>,
    #[serde(default)]
    source: CursorHeaderSource,
}

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CursorHeaderSource {
    #[default]
    Global,
    Workspace,
    Transcript,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CursorSubagentInfo {
    #[serde(default)]
    parent_composer_id: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CursorComposerData {
    #[serde(default)]
    composer_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    workspace_identifier: Option<Value>,
    #[serde(default)]
    full_conversation_headers_only: Vec<CursorConversationHeader>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CursorConversationHeader {
    #[serde(default)]
    bubble_id: String,
    #[serde(rename = "type")]
    bubble_type: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CursorBubble {
    #[serde(default)]
    bubble_id: String,
    #[serde(rename = "type")]
    bubble_type: Option<i64>,
    #[serde(default)]
    text: String,
    created_at: Option<Value>,
    capability_type: Option<i64>,
    #[serde(default)]
    tool_former_data: Option<Value>,
    thinking: Option<Value>,
}

#[derive(Debug, Clone)]
struct TranscriptRef {
    composer_id: String,
    path: PathBuf,
    project_slug: String,
    updated_at_ms: i64,
}

impl CursorPlatform {
    pub fn new(cursor_home: PathBuf) -> Self {
        Self { cursor_home }
    }

    fn db_path(&self) -> PathBuf {
        self.cursor_home.join("globalStorage").join("state.vscdb")
    }

    fn workspace_storage_dir(&self) -> PathBuf {
        self.cursor_home.join("workspaceStorage")
    }

    fn connect_readonly_path(db_path: &Path) -> Result<Connection, String> {
        let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| format!("Failed to open Cursor db '{}': {e}", db_path.display()))?;
        conn.busy_timeout(Duration::from_millis(800)).ok();
        Ok(conn)
    }

    fn connect_readonly(&self) -> Result<Connection, String> {
        Self::connect_readonly_path(&self.db_path())
    }

    fn connect_write(&self) -> Result<Connection, String> {
        let db_path = self.db_path();
        let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|e| {
                format!(
                    "Failed to open Cursor db for writing '{}': {e}. Close Cursor and try again if the database is locked.",
                    db_path.display()
                )
            })?;
        conn.busy_timeout(Duration::from_millis(800)).ok();
        Ok(conn)
    }

    fn table_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![name],
            |_| Ok(()),
        )
        .optional()
        .ok()
        .flatten()
        .is_some()
    }

    fn read_headers_from_table(conn: &Connection) -> Result<Vec<CursorComposerHeader>, String> {
        if !Self::table_exists(conn, "composerHeaders") {
            return Ok(Vec::new());
        }

        let mut stmt = conn
            .prepare(
                "SELECT composerId, createdAt, lastUpdatedAt, isArchived, isSubagent, value, subagentTypeName
                 FROM composerHeaders",
            )
            .map_err(|e| format!("Failed to prepare composerHeaders query: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row_text(row, 0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?.unwrap_or(0) != 0,
                    row.get::<_, Option<i64>>(4)?.unwrap_or(0) != 0,
                    row_text(row, 5)?,
                    row_text(row, 6)?,
                ))
            })
            .map_err(|e| format!("Failed to query composerHeaders: {e}"))?;

        let mut headers = Vec::new();
        for row in rows {
            let Ok((composer_id, created_at, last_updated_at, is_archived, is_subagent, value_raw, subagent_type)) =
                row
            else {
                continue;
            };
            if composer_id.trim().is_empty() {
                continue;
            }

            let mut header = if value_raw.trim().is_empty() {
                CursorComposerHeader {
                    composer_id: composer_id.clone(),
                    ..Default::default()
                }
            } else {
                match serde_json::from_str::<CursorComposerHeader>(&value_raw) {
                    Ok(mut parsed) => {
                        if parsed.composer_id.trim().is_empty() {
                            parsed.composer_id = composer_id.clone();
                        }
                        parsed
                    }
                    Err(_) => CursorComposerHeader {
                        composer_id: composer_id.clone(),
                        ..Default::default()
                    },
                }
            };

            header.created_at = header.created_at.or(created_at);
            header.last_updated_at = header.last_updated_at.or(last_updated_at);
            header.is_archived = header.is_archived || is_archived;
            if is_subagent
                && header
                    .subagent_info
                    .as_ref()
                    .map(|info| info.parent_composer_id.trim().is_empty())
                    .unwrap_or(true)
            {
                header.subagent_info = Some(CursorSubagentInfo {
                    parent_composer_id: if subagent_type.trim().is_empty() {
                        "subagent".to_string()
                    } else {
                        subagent_type
                    },
                });
            }
            header.source = CursorHeaderSource::Global;
            headers.push(header);
        }
        Ok(headers)
    }

    fn read_headers_from_item_table(
        conn: &Connection,
    ) -> Result<Vec<CursorComposerHeader>, String> {
        let raw = conn
            .query_row(
                "SELECT value FROM ItemTable WHERE key = ?1",
                params![COMPOSER_HEADERS_KEY],
                |row| row_text(row, 0),
            )
            .optional()
            .map_err(|e| format!("Failed to read Cursor composer headers: {e}"))?
            .unwrap_or_default();

        if raw.trim().is_empty() {
            return Ok(Vec::new());
        }

        let headers: CursorComposerHeaders = serde_json::from_str(&raw)
            .map_err(|e| format!("Failed to parse Cursor composer headers: {e}"))?;
        Ok(headers.all_composers)
    }

    fn read_workspace_headers(&self) -> Vec<CursorComposerHeader> {
        let root = self.workspace_storage_dir();
        let Ok(entries) = fs::read_dir(&root) else {
            return Vec::new();
        };

        let mut headers = Vec::new();
        for entry in entries.flatten() {
            let db_path = entry.path().join("state.vscdb");
            if !db_path.is_file() {
                continue;
            }
            let Ok(conn) = Self::connect_readonly_path(&db_path) else {
                continue;
            };
            let Ok(Some(raw)) = conn
                .query_row(
                    "SELECT value FROM ItemTable WHERE key = ?1",
                    params![WORKSPACE_COMPOSER_DATA_KEY],
                    |row| row_text(row, 0),
                )
                .optional()
            else {
                continue;
            };
            let Ok(payload) = serde_json::from_str::<Value>(&raw) else {
                continue;
            };
            let Some(all_composers) = payload.get("allComposers").and_then(Value::as_array) else {
                continue;
            };

            let workspace_cwd = read_workspace_cwd(&entry.path());
            for item in all_composers {
                let Ok(mut header) = serde_json::from_value::<CursorComposerHeader>(item.clone())
                else {
                    continue;
                };
                if header.composer_id.trim().is_empty() {
                    continue;
                }
                if header.workspace_identifier.is_none() {
                    if let Some(cwd) = workspace_cwd.as_ref() {
                        header.workspace_identifier = Some(json!({
                            "uri": { "fsPath": cwd, "path": cwd }
                        }));
                    }
                }
                header.source = CursorHeaderSource::Workspace;
                headers.push(header);
            }
        }
        headers
    }

    fn collect_headers(
        &self,
        conn: Option<&Connection>,
    ) -> (Vec<CursorComposerHeader>, HashSet<String>) {
        let t0 = Instant::now();
        let mut by_id: HashMap<String, CursorComposerHeader> = HashMap::new();
        let mut global_count = 0usize;

        if let Some(conn) = conn {
            let table_headers = Self::read_headers_from_table(conn).unwrap_or_default();
            let item_headers = if table_headers.is_empty() {
                Self::read_headers_from_item_table(conn).unwrap_or_default()
            } else {
                Vec::new()
            };
            for header in table_headers.into_iter().chain(item_headers) {
                by_id.insert(header.composer_id.clone(), header);
                global_count += 1;
            }
        }
        let t_global = t0.elapsed();

        let t_ws = Instant::now();
        let workspace_headers = self.read_workspace_headers();
        let workspace_count = workspace_headers.len();
        for header in workspace_headers {
            by_id.entry(header.composer_id.clone()).or_insert(header);
        }
        let t_workspace = t_ws.elapsed();

        let t_tr = Instant::now();
        let transcripts = self.discover_transcripts();
        let transcript_count = transcripts.len();
        let mut transcript_ids = HashSet::with_capacity(transcript_count);
        for transcript in transcripts {
            transcript_ids.insert(transcript.composer_id.clone());
            by_id.entry(transcript.composer_id.clone()).or_insert(
                CursorComposerHeader {
                    composer_id: transcript.composer_id.clone(),
                    name: transcript.composer_id.clone(),
                    subtitle: format!("agent transcript · {}", transcript.project_slug),
                    last_updated_at: Some(transcript.updated_at_ms),
                    created_at: Some(transcript.updated_at_ms),
                    source: CursorHeaderSource::Transcript,
                    ..Default::default()
                },
            );
        }
        let t_transcript = t_tr.elapsed();

        let mut headers: Vec<_> = by_id.into_values().collect();
        headers.retain(CursorComposerHeader::is_listable_index_entry);
        headers.sort_by_key(|header| std::cmp::Reverse(header.updated_at_value()));
        app_log::perf(format!(
            "cursor.collect_headers global={global_count} {:?} workspace={workspace_count} {:?} transcripts={transcript_count} {:?} merged={} total={:?}",
            t_global,
            t_workspace,
            t_transcript,
            headers.len(),
            t0.elapsed()
        ));
        (headers, transcript_ids)
    }

    fn discover_transcripts(&self) -> Vec<TranscriptRef> {
        let Some(root) = default_cursor_projects_home() else {
            return Vec::new();
        };
        let Ok(projects) = fs::read_dir(&root) else {
            return Vec::new();
        };

        let mut out = Vec::new();
        for project in projects.flatten() {
            let project_slug = project.file_name().to_string_lossy().to_string();
            let transcripts_dir = project.path().join("agent-transcripts");
            let Ok(sessions) = fs::read_dir(&transcripts_dir) else {
                continue;
            };
            for session in sessions.flatten() {
                if !session.path().is_dir() {
                    continue;
                }
                let composer_id = session.file_name().to_string_lossy().to_string();
                if composer_id.trim().is_empty() {
                    continue;
                }
                let path = session.path().join(format!("{composer_id}.jsonl"));
                if !path.is_file() {
                    continue;
                }
                let updated_at_ms = fs::metadata(&path)
                    .and_then(|meta| meta.modified())
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_millis() as i64)
                    .unwrap_or(0);
                out.push(TranscriptRef {
                    composer_id,
                    path,
                    project_slug: project_slug.clone(),
                    updated_at_ms,
                });
            }
        }
        out
    }

    fn find_transcript(&self, composer_id: &str) -> Option<TranscriptRef> {
        let root = default_cursor_projects_home()?;
        let projects = fs::read_dir(&root).ok()?;
        for project in projects.flatten() {
            let project_slug = project.file_name().to_string_lossy().to_string();
            let path = project
                .path()
                .join("agent-transcripts")
                .join(composer_id)
                .join(format!("{composer_id}.jsonl"));
            if !path.is_file() {
                continue;
            }
            let updated_at_ms = fs::metadata(&path)
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as i64)
                .unwrap_or(0);
            return Some(TranscriptRef {
                composer_id: composer_id.to_string(),
                path,
                project_slug,
                updated_at_ms,
            });
        }
        None
    }

    fn kv_content_fingerprint(
        conn: &Connection,
        composer_id: &str,
    ) -> Option<SessionSummaryFingerprint> {
        if !Self::table_exists(conn, "cursorDiskKV") {
            return None;
        }
        let (start, end) = bubble_key_bounds(composer_id);
        let (bubble_bytes, bubble_count): (i64, i64) = conn
            .query_row(
                "SELECT COALESCE(SUM(LENGTH(value)), 0), COUNT(*)
                 FROM cursorDiskKV
                 WHERE key >= ?1 AND key < ?2",
                params![start, end],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok()?;
        let composer_bytes: i64 = conn
            .query_row(
                "SELECT LENGTH(value) FROM cursorDiskKV WHERE key = ?1",
                params![format!("composerData:{composer_id}")],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten()
            .unwrap_or(0);
        if bubble_bytes == 0 && composer_bytes == 0 {
            return None;
        }
        Some(SessionSummaryFingerprint {
            file_size: bubble_bytes.saturating_add(composer_bytes),
            modified_at: format!("b{bubble_count}:v{bubble_bytes}:c{composer_bytes}"),
        })
    }

    fn content_fingerprint_fast(&self, composer_id: &str) -> Option<SessionSummaryFingerprint> {
        if let Ok(conn) = self.connect_readonly() {
            if let Some(fp) = Self::kv_content_fingerprint(&conn, composer_id) {
                return Some(fp);
            }
        }
        self.find_transcript(composer_id)
            .and_then(|transcript| SessionSummaryCache::fingerprint(&transcript.path))
    }

    fn content_fingerprint(&self, composer_id: &str) -> Option<SessionSummaryFingerprint> {
        if let Some(fp) = self.content_fingerprint_fast(composer_id) {
            return Some(fp);
        }

        let root = self.workspace_storage_dir();
        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.flatten() {
                let db_path = entry.path().join("state.vscdb");
                let Ok(conn) = Self::connect_readonly_path(&db_path) else {
                    continue;
                };
                if let Some(fp) = Self::kv_content_fingerprint(&conn, composer_id) {
                    return Some(fp);
                }
            }
        }
        None
    }

    fn searchable_content_entries(&self, session_key: &str) -> Vec<SessionContentEntry> {
        let mut entries = Vec::new();
        let mut indexed_bytes = 0usize;

        if let Ok((data, _)) = self.read_composer_data(session_key) {
            if let Ok(bubbles) = self.read_bubbles(session_key) {
                for (index, header) in data.full_conversation_headers_only.iter().enumerate() {
                    let Some(bubble) = bubbles.get(&header.bubble_id) else {
                        continue;
                    };
                    let role = bubble_role(bubble.bubble_type.or(header.bubble_type))
                        .unwrap_or("assistant");
                    let mut texts = vec![bubble.text.clone()];
                    if let Some(thinking) = thinking_text(bubble) {
                        texts.push(thinking);
                    }
                    if let Some(tool) = bubble.tool_former_data.as_ref() {
                        if let Some(name) = tool.get("name").and_then(Value::as_str) {
                            texts.push(name.to_string());
                        }
                        if let Some(raw_args) = tool.get("rawArgs").and_then(Value::as_str) {
                            texts.push(raw_args.to_string());
                        }
                        if let Some(result) = tool.get("result").and_then(Value::as_str) {
                            texts.push(result.to_string());
                        }
                    }
                    let entry = SessionContentEntry::any_text(index, role, texts);
                    if !push_bounded_index_entry(&mut entries, &mut indexed_bytes, entry) {
                        break;
                    }
                }
                return entries;
            }
        }

        let Some(transcript) = self.find_transcript(session_key) else {
            return entries;
        };
        let Ok(blocks) = self.blocks_from_transcript(&transcript) else {
            return entries;
        };
        for (index, block) in blocks.into_iter().enumerate() {
            let mut texts = vec![block.content];
            for tool in block.tool_calls {
                texts.push(tool.name);
                if let Some(input) = tool.input {
                    texts.push(input);
                }
                if let Some(output) = tool.output {
                    texts.push(output);
                }
            }
            let entry = SessionContentEntry::any_text(index, block.role, texts);
            if !push_bounded_index_entry(&mut entries, &mut indexed_bytes, entry) {
                break;
            }
        }
        entries
    }

    fn read_composer_data_from_conn(
        conn: &Connection,
        composer_id: &str,
    ) -> Result<CursorComposerData, String> {
        let key = format!("composerData:{composer_id}");
        let raw = conn
            .query_row(
                "SELECT value FROM cursorDiskKV WHERE key = ?1",
                params![key],
                |row| row_text(row, 0),
            )
            .optional()
            .map_err(|e| format!("Failed to read Cursor composer data: {e}"))?
            .ok_or_else(|| format!("Cursor composer data not found: {composer_id}"))?;

        serde_json::from_str(&raw)
            .map_err(|e| format!("Failed to parse Cursor composer data '{composer_id}': {e}"))
    }

    fn read_composer_data(&self, composer_id: &str) -> Result<(CursorComposerData, bool), String> {
        if let Ok(conn) = self.connect_readonly() {
            if let Ok(data) = Self::read_composer_data_from_conn(&conn, composer_id) {
                return Ok((data, true));
            }
        }

        let root = self.workspace_storage_dir();
        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.flatten() {
                let db_path = entry.path().join("state.vscdb");
                let Ok(conn) = Self::connect_readonly_path(&db_path) else {
                    continue;
                };
                if let Ok(data) = Self::read_composer_data_from_conn(&conn, composer_id) {
                    return Ok((data, true));
                }
            }
        }

        Err(format!("Cursor composer data not found: {composer_id}"))
    }

    fn read_bubbles_from_conn(
        conn: &Connection,
        composer_id: &str,
    ) -> Result<HashMap<String, CursorBubble>, String> {
        if !Self::table_exists(conn, "cursorDiskKV") {
            return Ok(HashMap::new());
        }

        let (start, end) = bubble_key_bounds(composer_id);
        let mut stmt = conn
            .prepare(
                "SELECT key, value FROM cursorDiskKV WHERE key >= ?1 AND key < ?2 ORDER BY key",
            )
            .map_err(|e| format!("Failed to prepare Cursor bubble query: {e}"))?;
        let rows = stmt
            .query_map(params![start, end], |row| {
                Ok((row_text(row, 0)?, row_text(row, 1)?))
            })
            .map_err(|e| format!("Failed to query Cursor bubbles: {e}"))?;

        let prefix = format!("bubbleId:{composer_id}:");
        let mut bubbles = HashMap::new();
        for row in rows {
            let Ok((key, raw)) = row else { continue };
            let bubble_id = key.strip_prefix(&prefix).unwrap_or("").to_string();
            if bubble_id.is_empty() {
                continue;
            }
            let Ok(mut bubble) = serde_json::from_str::<CursorBubble>(&raw) else {
                continue;
            };
            if bubble.bubble_id.is_empty() {
                bubble.bubble_id = bubble_id.clone();
            }
            bubbles.insert(bubble_id, bubble);
        }
        Ok(bubbles)
    }

    fn read_bubbles(&self, composer_id: &str) -> Result<HashMap<String, CursorBubble>, String> {
        if let Ok(conn) = self.connect_readonly() {
            let bubbles = Self::read_bubbles_from_conn(&conn, composer_id)?;
            if !bubbles.is_empty() {
                return Ok(bubbles);
            }
        }

        let root = self.workspace_storage_dir();
        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.flatten() {
                let db_path = entry.path().join("state.vscdb");
                let Ok(conn) = Self::connect_readonly_path(&db_path) else {
                    continue;
                };
                let bubbles = Self::read_bubbles_from_conn(&conn, composer_id)?;
                if !bubbles.is_empty() {
                    return Ok(bubbles);
                }
            }
        }

        Ok(HashMap::new())
    }

    fn header_for<'a>(
        &self,
        headers: &'a [CursorComposerHeader],
        composer_id: &str,
    ) -> Option<&'a CursorComposerHeader> {
        headers
            .iter()
            .find(|header| header.composer_id == composer_id)
    }

    fn should_show_list_item(
        &self,
        header: &CursorComposerHeader,
        transcript_ids: &HashSet<String>,
        global_conn: Option<&Connection>,
    ) -> bool {
        // Fast path: never open workspace DBs during list filtering.
        if header.source == CursorHeaderSource::Transcript {
            return true;
        }
        if header.has_human_label() {
            return true;
        }
        // Workspace index entries come from allComposers and are real sessions.
        if header.source == CursorHeaderSource::Workspace {
            return true;
        }
        if transcript_ids.contains(&header.composer_id) {
            return true;
        }

        // Unlabeled global headers: inspect global state.vscdb only.
        let Some(conn) = global_conn else {
            return false;
        };
        match Self::read_composer_data_from_conn(conn, &header.composer_id) {
            Ok(data) => {
                !data.name.trim().is_empty() || !data.full_conversation_headers_only.is_empty()
            }
            Err(_) => false,
        }
    }

    fn enrich_header_title(
        &self,
        header: &mut CursorComposerHeader,
        global_conn: Option<&Connection>,
    ) {
        if header.has_human_label() {
            return;
        }
        let Some(conn) = global_conn else {
            return;
        };
        if let Ok(data) = Self::read_composer_data_from_conn(conn, &header.composer_id) {
            if !data.name.trim().is_empty() {
                header.name = data.name;
            }
        }
    }

    fn blocks_from_composer(
        &self,
        session_key: &str,
        data: &CursorComposerData,
        bubbles: &HashMap<String, CursorBubble>,
    ) -> Vec<TimelineBlock> {
        let mut blocks = Vec::new();
        let mut pending_tools = Vec::new();

        for (index, conversation_header) in data.full_conversation_headers_only.iter().enumerate() {
            let Some(bubble) = bubbles.get(&conversation_header.bubble_id) else {
                continue;
            };
            let bubble_type = bubble.bubble_type.or(conversation_header.bubble_type);

            if let Some(tool_call) =
                tool_former_to_block(session_key, &conversation_header.bubble_id, bubble)
            {
                attach_or_pending_tool(&mut blocks, &mut pending_tools, tool_call);
            }

            if let Some(thinking) = thinking_text(bubble) {
                flush_pending_tools(&mut blocks, &mut pending_tools);
                blocks.push(TimelineBlock {
                    id: format!("{}:thinking", conversation_header.bubble_id),
                    role: "thinking".to_string(),
                    content: thinking,
                    editable: false,
                    edit_target: String::new(),
                    source_meta: json!({
                        "composerId": session_key,
                        "bubbleId": conversation_header.bubble_id,
                        "bubbleType": bubble_type,
                        "capabilityType": bubble.capability_type,
                        "createdAt": bubble.created_at,
                        "conversationIndex": index,
                    }),
                    tool_calls: Vec::new(),
                });
            }

            let Some(role) = bubble_role(bubble_type) else {
                continue;
            };
            if bubble.text.trim().is_empty() {
                continue;
            }

            let mut block = TimelineBlock {
                id: conversation_header.bubble_id.clone(),
                role: role.to_string(),
                content: bubble.text.clone(),
                editable: true,
                edit_target: format!("{session_key}::{}", conversation_header.bubble_id),
                source_meta: json!({
                    "composerId": session_key,
                    "bubbleId": conversation_header.bubble_id,
                    "bubbleType": bubble_type,
                    "capabilityType": bubble.capability_type,
                    "createdAt": bubble.created_at,
                    "conversationIndex": index,
                }),
                tool_calls: Vec::new(),
            };
            if role == "assistant" {
                block.tool_calls.append(&mut pending_tools);
            } else {
                flush_pending_tools(&mut blocks, &mut pending_tools);
            }
            blocks.push(block);
        }

        flush_pending_tools(&mut blocks, &mut pending_tools);
        blocks
    }

    fn blocks_from_transcript(&self, transcript: &TranscriptRef) -> Result<Vec<TimelineBlock>, String> {
        let raw = fs::read_to_string(&transcript.path)
            .map_err(|e| format!("Failed to read Cursor transcript '{}': {e}", transcript.path.display()))?;
        Ok(parse_transcript_blocks(
            &transcript.composer_id,
            &raw,
            &transcript.path,
        ))
    }
}

impl CursorComposerHeader {
    fn is_listable_index_entry(&self) -> bool {
        !self.composer_id.trim().is_empty()
            && self.composer_id != "empty-state-draft"
            && !self.is_draft
            && !self.is_archived
            && self
                .subagent_info
                .as_ref()
                .map(|info| info.parent_composer_id.trim().is_empty())
                .unwrap_or(true)
    }

    fn has_human_label(&self) -> bool {
        !self.name.trim().is_empty() || !self.subtitle.trim().is_empty()
    }

    fn updated_at_value(&self) -> i64 {
        self.last_updated_at.or(self.created_at).unwrap_or(0)
    }

    fn title(&self) -> String {
        if self.name.trim().is_empty() {
            self.composer_id.clone()
        } else {
            self.name.clone()
        }
    }

    fn cwd(&self) -> String {
        workspace_path(self.workspace_identifier.as_ref())
            .or_else(|| agent_location_path(self.agent_location.as_ref()))
            .unwrap_or_default()
    }
}

impl CursorComposerData {
    fn title(&self, fallback_id: &str) -> String {
        if !self.name.trim().is_empty() {
            return self.name.clone();
        }
        if !self.composer_id.trim().is_empty() {
            return self.composer_id.clone();
        }
        fallback_id.to_string()
    }

    fn cwd(&self) -> String {
        workspace_path(self.workspace_identifier.as_ref()).unwrap_or_default()
    }
}

impl PlatformAdapter for CursorPlatform {
    // list_session_keys is for content-index warmup only; keyed paging stays off because
    // session_list_item is not implemented (would re-scan headers per row).
    fn list_session_keys(&self) -> Option<Vec<SessionKey>> {
        let conn = self.connect_readonly().ok();
        if conn.is_none()
            && !self.workspace_storage_dir().is_dir()
            && default_cursor_projects_home().is_none()
        {
            return Some(Vec::new());
        }
        let (headers, transcript_ids) = self.collect_headers(conn.as_ref());
        Some(
            headers
                .into_iter()
                .filter(|header| {
                    self.should_show_list_item(header, &transcript_ids, conn.as_ref())
                })
                .map(|header| {
                    SessionKey::standalone(
                        header.composer_id.clone(),
                        header.updated_at_value() as i128,
                    )
                })
                .collect(),
        )
    }

    fn list_sessions(
        &self,
        alias_map: &HashMap<String, String>,
        limit: Option<usize>,
        offset: usize,
    ) -> SessionListResult {
        let t0 = Instant::now();
        let conn = self.connect_readonly().ok();
        if conn.is_none() && !self.workspace_storage_dir().is_dir() && default_cursor_projects_home().is_none()
        {
            return SessionListResult {
                total: 0,
                items: Vec::new(),
            };
        }

        let t_collect = Instant::now();
        let (mut headers, transcript_ids) = self.collect_headers(conn.as_ref());
        let collected = headers.len();
        let collect_elapsed = t_collect.elapsed();

        let t_filter = Instant::now();
        headers.retain(|header| {
            self.should_show_list_item(header, &transcript_ids, conn.as_ref())
        });
        let after_filter = headers.len();
        let filter_elapsed = t_filter.elapsed();

        let t_enrich = Instant::now();
        for header in &mut headers {
            self.enrich_header_title(header, conn.as_ref());
        }
        let enrich_elapsed = t_enrich.elapsed();

        let total = headers.len();
        let items = headers
            .into_iter()
            .skip(offset)
            .take(limit.unwrap_or(usize::MAX))
            .map(|header| {
                let alias = alias_map
                    .get(&header.composer_id)
                    .cloned()
                    .unwrap_or_default();
                let display_title = if alias.is_empty() {
                    header.title()
                } else {
                    alias.clone()
                };
                let updated_at = header.updated_at_value().to_string();
                let cwd = header.cwd();
                let editable = header.source != CursorHeaderSource::Transcript;
                SessionListItem {
                    platform: "cursor".to_string(),
                    session_key: header.composer_id.clone(),
                    session_id: header.composer_id.clone(),
                    display_title,
                    alias_title: alias,
                    preview: header.subtitle,
                    updated_at,
                    cwd,
                    editable,
                    content_matches: Vec::new(),
                    total_content_matches: 0,
                    favorite: false,
                    agent_group: None,
                }
            })
            .collect::<Vec<_>>();

        app_log::perf(format!(
            "cursor.list_sessions collected={collected} after_filter={after_filter} page={} collect={:?} filter={:?} enrich={:?} total={:?}",
            items.len(),
            collect_elapsed,
            filter_elapsed,
            enrich_elapsed,
            t0.elapsed()
        ));

        SessionListResult { total, items }
    }

    fn get_session_detail(
        &self,
        session_key: &str,
        alias_map: &HashMap<String, String>,
    ) -> Result<SessionDetail, String> {
        let t0 = Instant::now();
        let session_key = session_key
            .strip_prefix(TRANSCRIPT_SESSION_PREFIX)
            .unwrap_or(session_key);

        let conn = self.connect_readonly().ok();
        let t_headers = Instant::now();
        let (headers, _transcript_ids) = self.collect_headers(conn.as_ref());
        let headers_elapsed = t_headers.elapsed();
        let header = self.header_for(&headers, session_key).cloned();

        if let Ok((data, _)) = self.read_composer_data(session_key) {
            let t_bubbles = Instant::now();
            let bubbles = self.read_bubbles(session_key)?;
            let bubbles_elapsed = t_bubbles.elapsed();
            let mut blocks = self.blocks_from_composer(session_key, &data, &bubbles);
            let mut used_transcript = false;
            if blocks.is_empty() {
                if let Some(transcript) = self.find_transcript(session_key) {
                    if let Ok(transcript_blocks) = self.blocks_from_transcript(&transcript) {
                        if !transcript_blocks.is_empty() {
                            blocks = transcript_blocks;
                            used_transcript = true;
                        }
                    }
                }
            }
            let alias = alias_map.get(session_key).cloned().unwrap_or_default();
            let title = if alias.is_empty() {
                header
                    .as_ref()
                    .map(CursorComposerHeader::title)
                    .filter(|value| value != session_key)
                    .unwrap_or_else(|| data.title(session_key))
            } else {
                alias.clone()
            };
            app_log::perf(format!(
                "cursor.get_session_detail key={session_key} headers={:?} bubbles={:?} blocks={} transcript_fallback={used_transcript} total={:?}",
                headers_elapsed,
                bubbles_elapsed,
                blocks.len(),
                t0.elapsed()
            ));
            return Ok(SessionDetail {
                platform: "cursor".to_string(),
                session_key: session_key.to_string(),
                session_id: session_key.to_string(),
                title,
                alias_title: alias,
                cwd: header
                    .as_ref()
                    .map(CursorComposerHeader::cwd)
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| data.cwd()),
                commands: build_commands("cursor", session_key),
                blocks,
            });
        }

        let transcript = self
            .find_transcript(session_key)
            .ok_or_else(|| format!("Cursor session not found: {session_key}"))?;
        let blocks = self.blocks_from_transcript(&transcript)?;
        let alias = alias_map.get(session_key).cloned().unwrap_or_default();
        let title = if alias.is_empty() {
            header
                .as_ref()
                .map(CursorComposerHeader::title)
                .unwrap_or_else(|| session_key.to_string())
        } else {
            alias.clone()
        };

        app_log::perf(format!(
            "cursor.get_session_detail key={session_key} headers={:?} transcript_only blocks={} total={:?}",
            headers_elapsed,
            blocks.len(),
            t0.elapsed()
        ));

        Ok(SessionDetail {
            platform: "cursor".to_string(),
            session_key: session_key.to_string(),
            session_id: session_key.to_string(),
            title,
            alias_title: alias,
            cwd: header.as_ref().map(CursorComposerHeader::cwd).unwrap_or_default(),
            commands: build_commands("cursor", session_key),
            blocks,
        })
    }

    fn update_message(&self, edit_target: &str, new_content: &str) -> Result<String, String> {
        let (composer_id, bubble_id) = edit_target
            .split_once("::")
            .ok_or_else(|| format!("Invalid Cursor edit target: {edit_target}"))?;
        if composer_id.is_empty() || bubble_id.is_empty() {
            return Err(format!("Invalid Cursor edit target: {edit_target}"));
        }
        if bubble_id.contains(":thinking") || bubble_id.starts_with("transcript:") {
            return Err("Cursor transcript/thinking blocks are read-only".to_string());
        }

        let conn = self.connect_write()?;
        let key = format!("bubbleId:{composer_id}:{bubble_id}");
        let raw = conn
            .query_row(
                "SELECT value FROM cursorDiskKV WHERE key = ?1",
                params![key],
                |row| row_text(row, 0),
            )
            .optional()
            .map_err(|e| format!("Failed to read Cursor bubble: {e}"))?
            .ok_or_else(|| format!("Cursor bubble not found: {bubble_id}"))?;

        let mut payload: Value = serde_json::from_str(&raw)
            .map_err(|e| format!("Failed to parse Cursor bubble: {e}"))?;
        let bubble_type = payload
            .get("type")
            .and_then(Value::as_i64)
            .ok_or_else(|| "Cursor bubble is missing type".to_string())?;
        if bubble_role(Some(bubble_type)).is_none() {
            return Err(format!("Cursor bubble type is not editable: {bubble_type}"));
        }
        if payload
            .get("toolFormerData")
            .and_then(|value| value.get("name"))
            .and_then(Value::as_str)
            .is_some()
            && payload
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .is_empty()
        {
            return Err("Cursor tool-call bubbles are not editable as text".to_string());
        }

        let old_content = payload
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        payload["text"] = Value::String(new_content.to_string());
        if bubble_type == 1 {
            payload["richText"] = Value::String(cursor_rich_text(new_content));
        }

        let serialized = serde_json::to_string(&payload)
            .map_err(|e| format!("Failed to serialize Cursor bubble: {e}"))?;
        conn.execute(
            "UPDATE cursorDiskKV SET value = ?1 WHERE key = ?2",
            params![serialized, format!("bubbleId:{composer_id}:{bubble_id}")],
        )
        .map_err(|e| {
            format!(
                "Failed to update Cursor bubble. Close Cursor and try again if the database is locked: {e}"
            )
        })?;

        Ok(old_content)
    }

    fn matches_query(&self, session_key: &str, query: &str) -> bool {
        !self.content_search(session_key, query).is_empty()
    }

    fn warm_content_index(
        &self,
        session_key: &str,
        index: Option<&SessionContentIndex<'_>>,
    ) -> bool {
        let Some(index) = index else {
            return false;
        };
        let Some(fingerprint) = self.content_fingerprint(session_key) else {
            return false;
        };
        if index.is_current("cursor", session_key, &fingerprint) {
            return true;
        }
        let entries = self.searchable_content_entries(session_key);
        index
            .replace("cursor", session_key, &fingerprint, &entries)
            .is_ok()
    }

    fn has_current_content_index(
        &self,
        session_key: &str,
        index: Option<&SessionContentIndex<'_>>,
    ) -> bool {
        let Some(index) = index else {
            return false;
        };
        if let Some(fingerprint) = self.content_fingerprint_fast(session_key) {
            return index.is_current("cursor", session_key, &fingerprint);
        }
        // Workspace-only composers: rely on background warm + periodic recheck.
        index.has_entry("cursor", session_key)
    }

    fn content_search(&self, session_key: &str, query: &str) -> Vec<ContentMatch> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        content_entries_to_matches(self.searchable_content_entries(session_key), &needle)
    }

    fn content_search_with_index(
        &self,
        session_key: &str,
        query: &str,
        index: Option<&SessionContentIndex<'_>>,
    ) -> Vec<ContentMatch> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        let Some(index) = index else {
            return self.content_search(session_key, &needle);
        };
        let Some(fingerprint) = self.content_fingerprint(session_key) else {
            return self.content_search(session_key, &needle);
        };
        if let Some(entries) = index.get_matches("cursor", session_key, &fingerprint, &needle) {
            return content_entries_to_matches(entries, &needle);
        }
        let entries = self.searchable_content_entries(session_key);
        let matches = content_entries_to_matches(entries.clone(), &needle);
        let _ = index.replace("cursor", session_key, &fingerprint, &entries);
        matches
    }
}

pub fn default_cursor_home() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("Cursor").join("User"))
    }

    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|home| {
            home.join("Library")
                .join("Application Support")
                .join("Cursor")
                .join("User")
        })
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        dirs::home_dir().map(|home| home.join(".config").join("Cursor").join("User"))
    }
}

fn default_cursor_projects_home() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".cursor").join("projects"))
}

fn bubble_role(bubble_type: Option<i64>) -> Option<&'static str> {
    match bubble_type {
        Some(1) => Some("user"),
        Some(2) => Some("assistant"),
        _ => None,
    }
}

fn bubble_has_visible_content(bubble: &CursorBubble, header_type: Option<i64>) -> bool {
    if !bubble.text.trim().is_empty() && bubble_role(bubble.bubble_type.or(header_type)).is_some() {
        return true;
    }
    if thinking_text(bubble).is_some() {
        return true;
    }
    bubble
        .tool_former_data
        .as_ref()
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .map(|name| !name.trim().is_empty())
        .unwrap_or(false)
}

fn bubble_key_bounds(composer_id: &str) -> (String, String) {
    (
        format!("bubbleId:{composer_id}:"),
        format!("bubbleId:{composer_id};"),
    )
}

fn workspace_path(value: Option<&Value>) -> Option<String> {
    let value = value?;
    value
        .pointer("/uri/fsPath")
        .and_then(Value::as_str)
        .or_else(|| value.pointer("/uri/path").and_then(Value::as_str))
        .map(ToString::to_string)
}

fn agent_location_path(value: Option<&Value>) -> Option<String> {
    let value = value?;
    value
        .pointer("/environment/uri/fsPath")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .pointer("/environment/uri/path")
                .and_then(Value::as_str)
        })
        .map(ToString::to_string)
}

fn read_workspace_cwd(workspace_dir: &Path) -> Option<String> {
    let raw = fs::read_to_string(workspace_dir.join("workspace.json")).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    value
        .get("folder")
        .and_then(Value::as_str)
        .map(|folder| {
            folder
                .strip_prefix("file:///")
                .or_else(|| folder.strip_prefix("file://"))
                .unwrap_or(folder)
                .replace('/', "\\")
        })
        .or_else(|| {
            value
                .pointer("/workspace/folders/0/uri/fsPath")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
}

fn thinking_text(bubble: &CursorBubble) -> Option<String> {
    let thinking = bubble.thinking.as_ref()?;
    let text = thinking
        .get("text")
        .and_then(Value::as_str)
        .or_else(|| thinking.as_str())
        .unwrap_or_default()
        .trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn tool_former_to_block(
    session_key: &str,
    bubble_id: &str,
    bubble: &CursorBubble,
) -> Option<ToolCallBlock> {
    let tool = bubble.tool_former_data.as_ref()?;
    let name = tool.get("name").and_then(Value::as_str)?.trim();
    if name.is_empty() {
        return None;
    }

    let status = tool
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("completed")
        .to_string();
    let id = tool
        .get("toolCallId")
        .and_then(Value::as_str)
        .unwrap_or(bubble_id)
        .to_string();
    let input = tool
        .get("rawArgs")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            tool.get("params")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        });
    let output = tool
        .get("result")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let error = if status == "error" {
        tool.pointer("/additionalData/status")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .or_else(|| Some(status.clone()))
    } else {
        None
    };

    Some(ToolCallBlock {
        id,
        name: name.to_string(),
        kind: "toolFormer".to_string(),
        status,
        input,
        output,
        error,
        started_at: None,
        ended_at: None,
        source_meta: json!({
            "composerId": session_key,
            "bubbleId": bubble_id,
            "capabilityType": bubble.capability_type,
            "tool": tool.get("tool"),
            "toolIndex": tool.get("toolIndex"),
            "modelCallId": tool.get("modelCallId"),
        }),
    })
}

fn attach_or_pending_tool(
    blocks: &mut Vec<TimelineBlock>,
    pending: &mut Vec<ToolCallBlock>,
    tool_call: ToolCallBlock,
) {
    if let Some(last) = blocks.iter_mut().rev().find(|block| block.role == "assistant") {
        last.tool_calls.push(tool_call);
    } else {
        pending.push(tool_call);
    }
}

fn flush_pending_tools(blocks: &mut Vec<TimelineBlock>, pending: &mut Vec<ToolCallBlock>) {
    if pending.is_empty() {
        return;
    }
    if let Some(last) = blocks.iter_mut().rev().find(|block| block.role == "assistant") {
        last.tool_calls.append(pending);
        return;
    }
    let mut block = TimelineBlock {
        id: format!("tool-host-{}", blocks.len()),
        role: "assistant".to_string(),
        content: String::new(),
        editable: false,
        edit_target: String::new(),
        source_meta: json!({ "itemType": "tool_calls" }),
        tool_calls: Vec::new(),
    };
    block.tool_calls.append(pending);
    blocks.push(block);
}

fn parse_transcript_blocks(composer_id: &str, raw: &str, path: &Path) -> Vec<TimelineBlock> {
    let mut blocks = Vec::new();
    let mut pending_tools = Vec::new();

    for (line_index, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if entry.get("type").and_then(Value::as_str) == Some("turn_ended") {
            continue;
        }
        let Some(role) = entry.get("role").and_then(Value::as_str) else {
            continue;
        };
        let Some(parts) = entry
            .pointer("/message/content")
            .and_then(Value::as_array)
        else {
            continue;
        };

        for (part_index, part) in parts.iter().enumerate() {
            let part_type = part.get("type").and_then(Value::as_str).unwrap_or_default();
            match (role, part_type) {
                ("user" | "assistant", "text") => {
                    let text = part
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if text.trim().is_empty() {
                        continue;
                    }
                    let mut block = TimelineBlock {
                        id: format!("{line_index}:{part_index}:{role}"),
                        role: role.to_string(),
                        content: text,
                        editable: false,
                        edit_target: String::new(),
                        source_meta: json!({
                            "composerId": composer_id,
                            "lineIndex": line_index,
                            "partIndex": part_index,
                            "source": "agent-transcript",
                            "path": path.display().to_string(),
                        }),
                        tool_calls: Vec::new(),
                    };
                    if role == "assistant" {
                        block.tool_calls.append(&mut pending_tools);
                    } else {
                        flush_pending_tools(&mut blocks, &mut pending_tools);
                    }
                    blocks.push(block);
                }
                ("assistant", "tool_use") => {
                    let name = part
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .to_string();
                    let id = part
                        .get("id")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                        .unwrap_or_else(|| format!("transcript-tool-{line_index}-{part_index}"));
                    let input = part
                        .get("input")
                        .and_then(|value| serde_json::to_string_pretty(value).ok());
                    let tool_call = ToolCallBlock {
                        id,
                        name,
                        kind: "tool_use".to_string(),
                        status: "completed".to_string(),
                        input,
                        output: None,
                        error: None,
                        started_at: None,
                        ended_at: None,
                        source_meta: json!({
                            "composerId": composer_id,
                            "lineIndex": line_index,
                            "partIndex": part_index,
                            "source": "agent-transcript",
                        }),
                    };
                    attach_or_pending_tool(&mut blocks, &mut pending_tools, tool_call);
                }
                _ => {}
            }
        }
    }

    flush_pending_tools(&mut blocks, &mut pending_tools);
    blocks
}

fn cursor_rich_text(text: &str) -> String {
    let content: Vec<Value> = text
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                json!({ "type": "paragraph" })
            } else {
                json!({
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": line }]
                })
            }
        })
        .collect();
    serde_json::to_string(&json!({ "type": "doc", "content": content })).unwrap_or_default()
}

fn row_text(row: &Row<'_>, index: usize) -> rusqlite::Result<String> {
    match row.get_ref(index)? {
        ValueRef::Null => Ok(String::new()),
        ValueRef::Integer(value) => Ok(value.to_string()),
        ValueRef::Real(value) => Ok(value.to_string()),
        ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
            Ok(String::from_utf8_lossy(bytes).to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_rich_text_keeps_blank_lines_as_paragraphs() {
        let raw = cursor_rich_text("one\n\ntwo");
        let parsed: Value = serde_json::from_str(&raw).expect("rich text json");

        assert_eq!(
            parsed
                .pointer("/content/0/content/0/text")
                .and_then(Value::as_str),
            Some("one")
        );
        assert_eq!(
            parsed.pointer("/content/1/type").and_then(Value::as_str),
            Some("paragraph")
        );
        assert_eq!(
            parsed
                .pointer("/content/2/content/0/text")
                .and_then(Value::as_str),
            Some("two")
        );
    }

    #[test]
    fn bubble_key_bounds_include_only_one_composer_prefix() {
        let (start, end) = bubble_key_bounds("abc");

        assert!("bubbleId:abc:1" >= start.as_str());
        assert!("bubbleId:abc:1" < end.as_str());
        assert!("bubbleId:abd:1" > end.as_str());
    }

    #[test]
    fn archived_and_draft_headers_are_not_listable() {
        let archived = CursorComposerHeader {
            composer_id: "abc".to_string(),
            is_archived: true,
            ..Default::default()
        };
        let draft = CursorComposerHeader {
            composer_id: "abc".to_string(),
            is_draft: true,
            ..Default::default()
        };
        let normal = CursorComposerHeader {
            composer_id: "abc".to_string(),
            ..Default::default()
        };

        assert!(!archived.is_listable_index_entry());
        assert!(!draft.is_listable_index_entry());
        assert!(normal.is_listable_index_entry());
    }

    #[test]
    fn subagent_headers_are_not_listable() {
        let subagent = CursorComposerHeader {
            composer_id: "child".to_string(),
            subagent_info: Some(CursorSubagentInfo {
                parent_composer_id: "parent".to_string(),
            }),
            ..Default::default()
        };
        let missing_parent = CursorComposerHeader {
            composer_id: "normal".to_string(),
            subagent_info: Some(CursorSubagentInfo::default()),
            ..Default::default()
        };

        assert!(!subagent.is_listable_index_entry());
        assert!(missing_parent.is_listable_index_entry());
    }

    #[test]
    fn tool_former_parses_stringified_params_and_result() {
        let bubble = CursorBubble {
            bubble_id: "b1".to_string(),
            bubble_type: Some(2),
            capability_type: Some(15),
            tool_former_data: Some(json!({
                "name": "read_file",
                "toolCallId": "tool_1",
                "status": "completed",
                "rawArgs": "{\"target_file\":\"a.rs\"}",
                "result": "{\"contents\":\"ok\"}"
            })),
            ..Default::default()
        };
        let tool = tool_former_to_block("c1", "b1", &bubble).expect("tool");
        assert_eq!(tool.name, "read_file");
        assert_eq!(tool.status, "completed");
        assert_eq!(tool.input.as_deref(), Some("{\"target_file\":\"a.rs\"}"));
        assert_eq!(tool.output.as_deref(), Some("{\"contents\":\"ok\"}"));
    }

    #[test]
    fn blocks_attach_tools_to_previous_assistant() {
        let platform = CursorPlatform::new(PathBuf::new());
        let data = CursorComposerData {
            composer_id: "c1".to_string(),
            full_conversation_headers_only: vec![
                CursorConversationHeader {
                    bubble_id: "u1".to_string(),
                    bubble_type: Some(1),
                },
                CursorConversationHeader {
                    bubble_id: "a1".to_string(),
                    bubble_type: Some(2),
                },
                CursorConversationHeader {
                    bubble_id: "t1".to_string(),
                    bubble_type: Some(2),
                },
            ],
            ..Default::default()
        };
        let mut bubbles = HashMap::new();
        bubbles.insert(
            "u1".to_string(),
            CursorBubble {
                bubble_id: "u1".to_string(),
                bubble_type: Some(1),
                text: "hi".to_string(),
                ..Default::default()
            },
        );
        bubbles.insert(
            "a1".to_string(),
            CursorBubble {
                bubble_id: "a1".to_string(),
                bubble_type: Some(2),
                text: "working".to_string(),
                ..Default::default()
            },
        );
        bubbles.insert(
            "t1".to_string(),
            CursorBubble {
                bubble_id: "t1".to_string(),
                bubble_type: Some(2),
                capability_type: Some(15),
                tool_former_data: Some(json!({
                    "name": "grep",
                    "toolCallId": "tool_grep",
                    "status": "completed",
                    "rawArgs": "{\"pattern\":\"foo\"}",
                    "result": "[]"
                })),
                ..Default::default()
            },
        );

        let blocks = platform.blocks_from_composer("c1", &data, &bubbles);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].role, "user");
        assert_eq!(blocks[1].role, "assistant");
        assert_eq!(blocks[1].tool_calls.len(), 1);
        assert_eq!(blocks[1].tool_calls[0].name, "grep");
    }

    #[test]
    fn transcript_parser_reads_text_and_tool_use() {
        let raw = r#"
{"role":"user","message":{"content":[{"type":"text","text":"hello"}]}}
{"role":"assistant","message":{"content":[{"type":"text","text":"sure"},{"type":"tool_use","name":"Read","input":{"path":"a.rs"}}]}}
{"type":"turn_ended","status":"success"}
"#;
        let blocks = parse_transcript_blocks("cid", raw, Path::new("x.jsonl"));
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].role, "user");
        assert_eq!(blocks[1].role, "assistant");
        assert_eq!(blocks[1].tool_calls.len(), 1);
        assert_eq!(blocks[1].tool_calls[0].name, "Read");
        assert!(!blocks[1].editable);
    }

    #[test]
    fn unlabeled_empty_headers_without_local_data_are_hidden() {
        let platform = CursorPlatform::new(PathBuf::from(
            "definitely-missing-cursor-home-for-unit-test",
        ));
        let empty = CursorComposerHeader {
            composer_id: "empty-not-a-real-composer".to_string(),
            ..Default::default()
        };
        assert!(!platform.should_show_list_item(&empty, &HashSet::new(), None));
    }

    #[test]
    fn bubble_visibility_includes_tools_and_thinking() {
        let text_bubble = CursorBubble {
            bubble_id: "b1".to_string(),
            bubble_type: Some(1),
            text: "hello".to_string(),
            ..Default::default()
        };
        let tool_bubble = CursorBubble {
            bubble_id: "b2".to_string(),
            bubble_type: Some(2),
            capability_type: Some(15),
            tool_former_data: Some(json!({"name": "read_file", "status": "completed"})),
            ..Default::default()
        };
        let thinking_bubble = CursorBubble {
            bubble_id: "b3".to_string(),
            bubble_type: Some(2),
            capability_type: Some(30),
            thinking: Some(json!({"text": "planning next step"})),
            ..Default::default()
        };

        assert!(bubble_has_visible_content(&text_bubble, Some(1)));
        assert!(bubble_has_visible_content(&tool_bubble, Some(2)));
        assert!(bubble_has_visible_content(&thinking_bubble, Some(2)));
    }
}
