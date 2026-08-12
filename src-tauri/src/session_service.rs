use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::thread;
use std::time::Instant;

use chrono::{Duration, Local, TimeZone};
use serde::Serialize;

use crate::database::{self, DbState};
use crate::platforms::{self, SessionDetail, SessionListItem, SessionListResult};
use crate::settings::AppSettings;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformSummary {
    pub platform: String,
    pub count: usize,
    pub latest: String,
    pub items: Vec<SessionListItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendPoint {
    pub day: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSummary {
    pub platforms: Vec<PlatformSummary>,
    pub trend: Vec<TrendPoint>,
    pub recent_sessions: Vec<SessionListItem>,
}

const DASHBOARD_PLATFORM_NAMES: [&str; 9] = [
    "claude", "codex", "opencode", "grok", "pi", "cursor", "kiro", "kiro-ide", "gemini",
];

pub fn dashboard_summary(db: &DbState, settings: &AppSettings) -> Result<DashboardSummary, String> {
    let t0 = Instant::now();
    let mut platforms_summary = Vec::new();
    let mut recent_sessions = Vec::new();
    let mut trend_map: HashMap<String, usize> = HashMap::new();

    let platform_names = dashboard_platform_names(settings);
    eprintln!(
        "[perf] dashboard_summary visible_platforms={:?}",
        platform_names
    );
    let db_path = db.db_path.clone();
    let settings = settings.clone();
    let platform_results = thread::scope(|scope| {
        let handles: Vec<_> = platform_names
            .iter()
            .map(|platform_name| {
                let db_path = db_path.clone();
                let settings = settings.clone();
                scope.spawn(move || dashboard_platform_summary(&db_path, &settings, platform_name))
            })
            .collect();

        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err("dashboard worker panicked".to_string()))
            })
            .collect::<Vec<_>>()
    });

    for result in platform_results {
        let (summary, items) = result?;
        for item in items.iter().take(20) {
            let day = format_timestamp(&item.updated_at);
            if !day.is_empty() {
                *trend_map.entry(day).or_insert(0) += 1;
            }
        }

        recent_sessions.extend(items.iter().take(10).cloned());
        platforms_summary.push(summary);
    }

    recent_sessions.sort_by_key(|item| std::cmp::Reverse(timestamp_sort_key(&item.updated_at)));
    recent_sessions.truncate(10);

    let today = Local::now().date_naive();
    let mut trend = Vec::new();
    for offset in (0..7).rev() {
        let day = today - Duration::days(offset);
        let key = day.format("%Y-%m-%d").to_string();
        trend.push(TrendPoint {
            day: key.clone(),
            count: trend_map.get(&key).copied().unwrap_or(0),
        });
    }

    eprintln!("[perf] dashboard_summary: {:?}", t0.elapsed());
    Ok(DashboardSummary {
        platforms: platforms_summary,
        trend,
        recent_sessions,
    })
}

fn dashboard_platform_names(settings: &AppSettings) -> Vec<&'static str> {
    let visible: HashSet<&str> = settings
        .visible_platforms
        .iter()
        .map(String::as_str)
        .collect();

    DASHBOARD_PLATFORM_NAMES
        .into_iter()
        .filter(|platform_name| visible.contains(platform_name))
        .collect()
}

fn dashboard_platform_summary(
    db_path: &str,
    settings: &AppSettings,
    platform_name: &str,
) -> Result<(PlatformSummary, Vec<SessionListItem>), String> {
    let tp = Instant::now();
    let db = DbState::new(db_path)?;
    let adapter = platforms::get_adapter(platform_name, settings)?;
    let aliases = database::get_alias_map(&db.conn, platform_name)?;
    let archived =
        database::get_flagged_keys(&db.conn, platform_name, "archived").unwrap_or_default();
    let favorites =
        database::get_flagged_keys(&db.conn, platform_name, "favorite").unwrap_or_default();
    let summary_cache = database::SessionSummaryCache::new(&db.conn);
    let result = list_sessions_page(
        adapter.as_ref(),
        &aliases,
        Some(50),
        0,
        &archived,
        &favorites,
        false,
        Some(&summary_cache),
    );
    let total = result.total;
    let items = result.items;
    eprintln!(
        "[perf] dashboard({platform_name}) list ({total} active): {:?}",
        tp.elapsed()
    );

    let summary = PlatformSummary {
        platform: platform_name.to_string(),
        count: total,
        latest: items
            .first()
            .map(|item| format_timestamp(&item.updated_at))
            .unwrap_or_default(),
        items: items.iter().take(5).cloned().collect(),
    };

    Ok((summary, items))
}

pub fn session_list(
    db: &DbState,
    settings: &AppSettings,
    platform: &str,
    query: Option<&str>,
    limit: Option<usize>,
    offset: usize,
    show_archived: bool,
) -> Result<SessionListResult, String> {
    let t0 = Instant::now();
    let adapter = platforms::get_adapter(platform, settings)?;
    let aliases = database::get_alias_map(&db.conn, platform)?;
    let archived = database::get_flagged_keys(&db.conn, platform, "archived").unwrap_or_default();
    let favorites = database::get_flagged_keys(&db.conn, platform, "favorite").unwrap_or_default();
    eprintln!("[perf] session_list({platform}) init: {:?}", t0.elapsed());

    let has_query = query.map(|q| !q.trim().is_empty()).unwrap_or(false);

    // Helper: filter by archive status and annotate favorites
    let apply_flags = |items: Vec<SessionListItem>,
                       archived: &HashSet<String>,
                       favorites: &HashSet<String>,
                       show_archived: bool|
     -> Vec<SessionListItem> {
        items
            .into_iter()
            .filter(|item| {
                let is_archived = archived.contains(&item.session_key);
                if show_archived {
                    is_archived
                } else {
                    !is_archived
                }
            })
            .map(|mut item| {
                item.favorite = favorites.contains(&item.session_key);
                item
            })
            .collect()
    };

    if has_query {
        let t1 = Instant::now();
        let summary_cache = database::SessionSummaryCache::new(&db.conn);
        let content_index = database::SessionContentIndex::new(&db.conn);
        let result = adapter.list_sessions_with_cache(&aliases, None, 0, Some(&summary_cache));
        eprintln!(
            "[perf] session_list({platform}) list_all {} sessions: {:?}",
            result.items.len(),
            t1.elapsed()
        );

        let needle = query.unwrap().trim().to_lowercase();
        let t2 = Instant::now();
        let mut search_count = 0usize;
        let filtered: Vec<SessionListItem> = result
            .items
            .into_iter()
            .filter_map(|item| {
                let title_match = [
                    item.display_title.as_str(),
                    item.preview.as_str(),
                    item.cwd.as_str(),
                    item.session_id.as_str(),
                ]
                .join(" ")
                .to_lowercase()
                .contains(&needle);

                // Skip expensive content_search when title already matches
                if title_match {
                    Some(item)
                } else {
                    search_count += 1;
                    let content_matches = adapter.content_search_with_index(
                        &item.session_key,
                        &needle,
                        Some(&content_index),
                    );
                    if !content_matches.is_empty() {
                        let mut item = item;
                        item.total_content_matches = content_matches.len();
                        item.content_matches = content_matches;
                        Some(item)
                    } else {
                        None
                    }
                }
            })
            .collect();
        let mut filtered = apply_flags(filtered, &archived, &favorites, show_archived);
        eprintln!(
            "[perf] session_list({platform}) content_search x{search_count} -> {} hits: {:?}",
            filtered.len(),
            t2.elapsed()
        );

        let total = filtered.len();
        let start = offset.min(total);
        let end = limit.map(|l| (start + l).min(total)).unwrap_or(total);
        let items = filtered.drain(start..end).collect();

        eprintln!("[perf] session_list({platform}) total: {:?}", t0.elapsed());
        Ok(SessionListResult { total, items })
    } else {
        let t1 = Instant::now();
        // For non-search: load enough to fill the page after filtering
        let summary_cache = database::SessionSummaryCache::new(&db.conn);
        let page_result = list_sessions_page(
            adapter.as_ref(),
            &aliases,
            limit,
            offset,
            &archived,
            &favorites,
            show_archived,
            Some(&summary_cache),
        );
        eprintln!(
            "[perf] session_list({platform}) paginated {} items: {:?}",
            page_result.total,
            t1.elapsed()
        );
        eprintln!("[perf] session_list({platform}) total: {:?}", t0.elapsed());
        schedule_content_index_warmup(
            settings,
            platform,
            &db.db_path,
            page_result
                .items
                .iter()
                .map(|item| item.session_key.clone())
                .collect(),
        );
        Ok(page_result)
    }
}

fn list_sessions_page(
    adapter: &dyn platforms::PlatformAdapter,
    aliases: &HashMap<String, String>,
    limit: Option<usize>,
    offset: usize,
    archived: &HashSet<String>,
    favorites: &HashSet<String>,
    show_archived: bool,
    summary_cache: Option<&database::SessionSummaryCache<'_>>,
) -> SessionListResult {
    if let Some(mut keys) = adapter.list_session_keys() {
        keys.retain(|item| {
            let is_archived = archived.contains(&item.key);
            if show_archived {
                is_archived
            } else {
                !is_archived
            }
        });
        keys.sort_by(|a, b| {
            favorites
                .contains(&b.key)
                .cmp(&favorites.contains(&a.key))
                .then_with(|| b.sort_key.cmp(&a.sort_key))
        });

        let total = keys.len();
        let page_keys: Vec<String> = keys
            .into_iter()
            .skip(offset.min(total))
            .take(limit.unwrap_or(usize::MAX))
            .map(|item| item.key)
            .collect();

        let items = page_keys
            .into_iter()
            .filter_map(|key| adapter.session_list_item(&key, aliases, summary_cache))
            .map(|mut item| {
                item.favorite = favorites.contains(&item.session_key);
                item
            })
            .collect();

        return SessionListResult { total, items };
    }

    let mut items = adapter
        .list_sessions_with_cache(aliases, None, 0, summary_cache)
        .items;
    items = items
        .into_iter()
        .filter(|item| {
            let is_archived = archived.contains(&item.session_key);
            if show_archived {
                is_archived
            } else {
                !is_archived
            }
        })
        .map(|mut item| {
            item.favorite = favorites.contains(&item.session_key);
            item
        })
        .collect();
    items.sort_by(|a, b| b.favorite.cmp(&a.favorite));

    let total = items.len();
    let start = offset.min(total);
    let end = limit.map(|l| (start + l).min(total)).unwrap_or(total);
    let page = items[start..end].to_vec();

    SessionListResult { total, items: page }
}

fn schedule_content_index_warmup(
    settings: &AppSettings,
    platform: &str,
    db_path: &str,
    session_keys: Vec<String>,
) {
    if session_keys.is_empty() || !matches!(platform, "claude" | "codex" | "pi" | "grok") {
        return;
    }

    let settings = settings.clone();
    let platform = platform.to_string();
    let db_path = db_path.to_string();
    thread::spawn(move || {
        let Ok(adapter) = platforms::get_adapter(&platform, &settings) else {
            return;
        };
        let Ok(db) = database::DbState::new(&db_path) else {
            return;
        };
        let index = database::SessionContentIndex::new(&db.conn);
        for session_key in session_keys.into_iter().take(20) {
            let _ = adapter.warm_content_index(&session_key, Some(&index));
        }
    });
}

pub fn session_toggle_flag(
    db: &DbState,
    platform: &str,
    session_key: &str,
    flag: &str,
) -> Result<bool, String> {
    database::toggle_session_flag(&db.conn, platform, session_key, flag)
}

pub fn session_batch_set_flag(
    db: &DbState,
    platform: &str,
    session_keys: &[String],
    flag: &str,
    set: bool,
) -> Result<usize, String> {
    let t0 = Instant::now();
    let affected = database::batch_set_session_flag(&db.conn, platform, session_keys, flag, set)?;
    eprintln!(
        "[perf] session_batch_set_flag({platform}, {flag}, set={set}) {} keys -> {} affected: {:?}",
        session_keys.len(),
        affected,
        t0.elapsed()
    );
    Ok(affected)
}

pub fn session_detail(
    db: &DbState,
    settings: &AppSettings,
    platform: &str,
    session_key: &str,
) -> Result<SessionDetail, String> {
    let t0 = Instant::now();
    let adapter = platforms::get_adapter(platform, settings)?;
    let aliases = database::get_alias_map(&db.conn, platform)?;
    let detail = adapter.get_session_detail(session_key, &aliases)?;
    eprintln!(
        "[perf] session_detail({platform}) {} blocks: {:?}",
        detail.blocks.len(),
        t0.elapsed()
    );
    Ok(detail)
}

pub fn session_execution_output(
    settings: &AppSettings,
    platform: &str,
    session_key: &str,
    edit_target: &str,
) -> Result<String, String> {
    let t0 = Instant::now();
    let adapter = platforms::get_adapter(platform, settings)?;
    let output = adapter.resolve_execution_output(session_key, edit_target)?;
    eprintln!(
        "[perf] session_execution_output({platform}) chars={}: {:?}",
        output.chars().count(),
        t0.elapsed()
    );
    Ok(output)
}

pub fn session_execution_outputs(
    settings: &AppSettings,
    platform: &str,
    session_key: &str,
    edit_targets: &[String],
) -> Result<std::collections::HashMap<String, String>, String> {
    let t0 = Instant::now();
    let adapter = platforms::get_adapter(platform, settings)?;
    let outputs = adapter.resolve_execution_outputs(session_key, edit_targets)?;
    eprintln!(
        "[perf] session_execution_outputs({platform}) requested={} found={}: {:?}",
        edit_targets.len(),
        outputs.len(),
        t0.elapsed()
    );
    Ok(outputs)
}

pub fn session_set_alias(
    db: &DbState,
    platform: &str,
    session_key: &str,
    title: &str,
) -> Result<database::SessionAlias, String> {
    database::save_alias(&db.conn, platform, session_key, title.trim())
}

pub fn session_edit_message(
    db: &DbState,
    settings: &AppSettings,
    platform: &str,
    edit_target: &str,
    content: &str,
    session_key: &str,
) -> Result<(), String> {
    let adapter = platforms::get_adapter(platform, settings)?;
    let old_content = adapter.update_message(edit_target, content)?;
    database::insert_edit_log(
        &db.conn,
        platform,
        session_key,
        edit_target,
        &old_content,
        content,
    )
}

const TOOL_EDIT_TARGET_PREFIX: &str = "tool-call::";

pub fn session_erase_tool_call(
    db: &DbState,
    settings: &AppSettings,
    platform: &str,
    tool_call_id: &str,
    session_key: &str,
) -> Result<(), String> {
    let adapter = platforms::get_adapter(platform, settings)?;
    let old_record = adapter
        .replace_tool_call(session_key, tool_call_id, None)?
        .ok_or_else(|| "Tool call was not found in this session".to_string())?;
    let edit_target = format!("{TOOL_EDIT_TARGET_PREFIX}{tool_call_id}");
    if let Err(error) = database::insert_edit_log(
        &db.conn,
        platform,
        session_key,
        &edit_target,
        &old_record,
        "",
    ) {
        let _ = adapter.replace_tool_call(session_key, tool_call_id, Some(&old_record));
        return Err(error);
    }
    Ok(())
}

pub fn session_edit_log(
    db: &DbState,
    platform: &str,
    session_key: &str,
) -> Result<Vec<database::EditLog>, String> {
    database::get_edit_log(&db.conn, platform, session_key)
}

pub fn session_delete_edit_log(
    db: &DbState,
    platform: &str,
    session_key: &str,
    edit_log_id: i64,
) -> Result<bool, String> {
    database::delete_edit_log(&db.conn, edit_log_id, platform, session_key)
}

pub fn session_clear_edit_logs(
    db: &DbState,
    platform: &str,
    session_key: &str,
) -> Result<usize, String> {
    database::clear_edit_logs(&db.conn, platform, session_key)
}

pub fn session_restore_message(
    db: &DbState,
    settings: &AppSettings,
    platform: &str,
    edit_log_id: i64,
    session_key: &str,
) -> Result<(), String> {
    let log =
        database::get_edit_log_by_id_for_session(&db.conn, edit_log_id, platform, session_key)?;
    if let Some(tool_call_id) = log.edit_target.strip_prefix(TOOL_EDIT_TARGET_PREFIX) {
        let adapter = platforms::get_adapter(platform, settings)?;
        let restore_record = (!log.old_content.is_empty()).then_some(log.old_content.as_str());
        let current_record =
            adapter.replace_tool_call(session_key, tool_call_id, restore_record)?;
        if let Err(error) = database::insert_edit_log(
            &db.conn,
            platform,
            session_key,
            &log.edit_target,
            current_record.as_deref().unwrap_or(""),
            &log.old_content,
        ) {
            let _ = adapter.replace_tool_call(session_key, tool_call_id, current_record.as_deref());
            return Err(error);
        }
        return Ok(());
    }
    session_edit_message(
        db,
        settings,
        platform,
        &log.edit_target,
        &log.old_content,
        session_key,
    )
}

fn format_timestamp(value: &str) -> String {
    let text = value.trim();
    if text.is_empty() {
        return String::new();
    }

    let Ok(mut number) = text.parse::<i128>() else {
        return text.to_string();
    };

    if number > 100_000_000_000_000_000 {
        number /= 1_000_000_000;
    } else if number > 1_000_000_000_000_000 {
        number /= 1_000_000;
    } else if number > 1_000_000_000_000 {
        number /= 1_000;
    }

    let Some(date_time) = Local.timestamp_opt(number as i64, 0).single() else {
        return String::new();
    };

    date_time.format("%Y-%m-%d").to_string()
}

fn timestamp_sort_key(value: &str) -> i128 {
    let text = value.trim();
    if text.is_empty() {
        return 0;
    }

    let Ok(mut number) = text.parse::<i128>() else {
        return 0;
    };

    if number > 100_000_000_000_000_000 {
        number /= 1_000_000_000;
    } else if number > 1_000_000_000_000_000 {
        number /= 1_000_000;
    } else if number > 1_000_000_000_000 {
        number /= 1_000;
    }

    number
}

#[allow(dead_code)]
fn path_exists(path: &str) -> bool {
    Path::new(path).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use uuid::Uuid;

    fn tool_test_paths(label: &str) -> (PathBuf, PathBuf, PathBuf) {
        let base = std::env::var_os("MEMORY_FORGE_TEST_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let root = base.join(format!("opencode-service-{label}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create service test directory");
        (
            root.clone(),
            root.join("memory-forge.db"),
            root.join("opencode.db"),
        )
    }

    #[test]
    fn dashboard_platform_names_only_include_visible_supported_platforms() {
        let settings = AppSettings {
            visible_platforms: vec![
                "gemini".to_string(),
                "unknown".to_string(),
                "claude".to_string(),
                "pi".to_string(),
            ],
            ..AppSettings::default()
        };

        assert_eq!(
            dashboard_platform_names(&settings),
            vec!["claude", "pi", "gemini"]
        );
    }

    #[test]
    fn dashboard_platform_names_can_be_empty_when_all_platforms_are_hidden() {
        let settings = AppSettings {
            visible_platforms: Vec::new(),
            ..AppSettings::default()
        };

        assert!(dashboard_platform_names(&settings).is_empty());
    }

    #[test]
    fn opencode_tool_erasure_is_logged_and_restore_is_reversible() {
        let (root, forge_path, opencode_path) = tool_test_paths("audit");
        let forge_conn = Connection::open(&forge_path).expect("create Memory Forge database");
        database::init_tables(&forge_conn).expect("initialize Memory Forge database");
        let db = DbState {
            conn: Mutex::new(forge_conn),
            db_path: forge_path.to_string_lossy().to_string(),
        };

        let opencode_conn = Connection::open(&opencode_path).expect("create OpenCode database");
        opencode_conn
            .execute_batch(
                "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT NOT NULL, directory TEXT NOT NULL, parent_id TEXT, time_updated INTEGER NOT NULL);
                 CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, time_created INTEGER NOT NULL, data TEXT NOT NULL);
                 CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT NOT NULL, session_id TEXT NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, data TEXT NOT NULL);
                 INSERT INTO session VALUES ('s1', 'test', 'F:\\work', NULL, 1);
                 INSERT INTO message VALUES ('m1', 's1', 1, '{\"role\":\"assistant\"}');",
            )
            .expect("create OpenCode test schema");
        let rejected_tool = json!({
            "type": "tool",
            "tool": "bash",
            "state": {
                "status": "error",
                "input": { "command": "dangerous-command" },
                "error": "Permission rejected by user"
            }
        })
        .to_string();
        opencode_conn
            .execute(
                "INSERT INTO part VALUES ('p1', 'm1', 's1', 10, 11, ?1)",
                params![rejected_tool],
            )
            .expect("insert rejected tool");
        drop(opencode_conn);

        let settings = AppSettings {
            opencode_path: Some(opencode_path.to_string_lossy().to_string()),
            ..AppSettings::default()
        };
        session_erase_tool_call(&db, &settings, "opencode", "p1", "s1")
            .expect("erase tool through service");
        let logs = session_edit_log(&db, "opencode", "s1").expect("read erase log");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].edit_target, "tool-call::p1");
        assert!(logs[0].old_content.contains("Permission rejected by user"));
        assert!(logs[0].new_content.is_empty());

        session_restore_message(&db, &settings, "opencode", logs[0].id, "s1")
            .expect("restore erased tool through edit log");
        let opencode_conn =
            Connection::open(&opencode_path).expect("open restored OpenCode database");
        let restored: String = opencode_conn
            .query_row("SELECT data FROM part WHERE id = 'p1'", [], |row| {
                row.get(0)
            })
            .expect("read restored tool");
        assert_eq!(restored, rejected_tool);
        drop(opencode_conn);

        let logs = session_edit_log(&db, "opencode", "s1").expect("read restore log");
        assert_eq!(logs.len(), 2);
        assert!(logs[0].old_content.is_empty());
        assert!(logs[0].new_content.contains("Permission rejected by user"));
        fs::remove_dir_all(root).ok();
    }
}
