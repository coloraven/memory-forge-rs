use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration as StdDuration, Instant};

use chrono::{Duration, Local, TimeZone};
use serde::Serialize;

use crate::database::{self, DbState};
use crate::platforms::{
    self, content_entries_to_matches, SessionDetail, SessionListItem, SessionListResult,
};
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

const SEARCH_PAGE_SIZE: usize = 50;
const SEARCH_PAGE_SIZE_MAX: usize = 100;
const SEARCH_MATCHES_PER_SESSION: usize = 5;
const INDEX_RECHECK_INTERVAL: StdDuration = StdDuration::from_secs(30);

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchIndexStatus {
    pub supported: bool,
    pub running: bool,
    pub indexed: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListResponse {
    pub total: usize,
    pub items: Vec<SessionListItem>,
    pub search_index: SearchIndexStatus,
}

#[derive(Debug, Default)]
struct IndexJobState {
    status: SearchIndexStatus,
    last_finished: Option<Instant>,
}

static INDEX_JOBS: OnceLock<Mutex<HashMap<String, IndexJobState>>> = OnceLock::new();

fn index_jobs() -> &'static Mutex<HashMap<String, IndexJobState>> {
    INDEX_JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn content_index_supported(platform: &str) -> bool {
    matches!(platform, "claude" | "codex" | "pi" | "grok")
}

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
) -> Result<SessionListResponse, String> {
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
        let indexed_matches = if content_index_supported(platform) {
            content_index.search_platform(platform, &needle, SEARCH_MATCHES_PER_SESSION)?
        } else {
            HashMap::new()
        };
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
                } else if let Some(indexed) = indexed_matches.get(&item.session_key).filter(|_| {
                    adapter.has_current_content_index(&item.session_key, Some(&content_index))
                }) {
                    let mut item = item;
                    item.total_content_matches = indexed.total;
                    item.content_matches =
                        content_entries_to_matches(indexed.entries.clone(), &needle);
                    Some(item)
                } else if !content_index_supported(platform) {
                    let content_matches = adapter.content_search(&item.session_key, &needle);
                    if content_matches.is_empty() {
                        None
                    } else {
                        let mut item = item;
                        item.total_content_matches = content_matches.len();
                        item.content_matches = content_matches
                            .into_iter()
                            .take(SEARCH_MATCHES_PER_SESSION)
                            .collect();
                        Some(item)
                    }
                } else {
                    None
                }
            })
            .collect();
        let mut filtered = apply_flags(filtered, &archived, &favorites, show_archived);
        eprintln!(
            "[perf] session_list({platform}) indexed search -> {} hits: {:?}",
            filtered.len(),
            t2.elapsed()
        );

        let total = filtered.len();
        let start = offset.min(total);
        let page_size = limit
            .unwrap_or(SEARCH_PAGE_SIZE)
            .clamp(1, SEARCH_PAGE_SIZE_MAX);
        let end = (start + page_size).min(total);
        let items = filtered.drain(start..end).collect();

        let search_index = schedule_content_index_warmup(settings, platform, &db.db_path);

        eprintln!("[perf] session_list({platform}) total: {:?}", t0.elapsed());
        Ok(SessionListResponse {
            total,
            items,
            search_index,
        })
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
        let search_index = schedule_content_index_warmup(settings, platform, &db.db_path);
        Ok(SessionListResponse {
            total: page_result.total,
            items: page_result.items,
            search_index,
        })
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
) -> SearchIndexStatus {
    if !content_index_supported(platform) {
        return SearchIndexStatus::default();
    }

    let Ok(adapter) = platforms::get_adapter(platform, settings) else {
        return SearchIndexStatus {
            supported: true,
            ..Default::default()
        };
    };
    let Some(session_keys) = adapter.list_session_keys() else {
        return SearchIndexStatus {
            supported: true,
            ..Default::default()
        };
    };
    let total = session_keys.len();
    let job_key = format!("{db_path}\u{1f}{platform}");
    {
        let Ok(mut jobs) = index_jobs().lock() else {
            return SearchIndexStatus {
                supported: true,
                total,
                ..Default::default()
            };
        };
        let state = jobs.entry(job_key.clone()).or_default();
        state.status.supported = true;
        state.status.total = total;
        if state.status.running
            || state
                .last_finished
                .is_some_and(|finished| finished.elapsed() < INDEX_RECHECK_INTERVAL)
        {
            return state.status.clone();
        }
        state.status.running = true;
        state.status.indexed = 0;
    }

    let settings = settings.clone();
    let platform = platform.to_string();
    let db_path = db_path.to_string();
    let worker_job_key = job_key.clone();
    thread::spawn(move || {
        let Ok(adapter) = platforms::get_adapter(&platform, &settings) else {
            finish_index_job(&worker_job_key);
            return;
        };
        let Ok(db) = database::DbState::new(&db_path) else {
            finish_index_job(&worker_job_key);
            return;
        };
        let index = database::SessionContentIndex::new(&db.conn);
        let mut indexed = 0usize;
        for session_key in session_keys {
            if adapter.warm_content_index(&session_key.key, Some(&index)) {
                indexed += 1;
            }
            if let Ok(mut jobs) = index_jobs().lock() {
                if let Some(state) = jobs.get_mut(&worker_job_key) {
                    state.status.indexed = indexed;
                }
            }
            thread::yield_now();
        }
        finish_index_job(&worker_job_key);
    });

    index_jobs()
        .lock()
        .ok()
        .and_then(|jobs| jobs.get(&job_key).map(|state| state.status.clone()))
        .unwrap_or(SearchIndexStatus {
            supported: true,
            running: true,
            indexed: 0,
            total,
        })
}

fn finish_index_job(job_key: &str) {
    if let Ok(mut jobs) = index_jobs().lock() {
        if let Some(state) = jobs.get_mut(job_key) {
            state.status.running = false;
            state.last_finished = Some(Instant::now());
        }
    }
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
    use std::fs;

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
    fn indexed_session_search_never_scans_source_on_the_request_path() {
        let root = std::env::var_os("MEMORY_FORGE_TEST_TMP")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!(
                "memory-forge-search-service-{}",
                std::process::id()
            ));
        let codex_home = root.join("codex-home");
        let sessions_dir = codex_home.join("sessions").join("2026");
        fs::create_dir_all(&sessions_dir).expect("create sessions dir");
        fs::write(
            sessions_dir.join("session.jsonl"),
            [
                serde_json::to_string(&serde_json::json!({
                    "payload": {
                        "type": "user_message",
                        "id": "session",
                        "cwd": root.display().to_string(),
                        "message": "ordinary preview"
                    }
                }))
                .expect("serialize session summary"),
                serde_json::to_string(&serde_json::json!({
                    "payload": {
                        "type": "agent_message",
                        "message": "unique-index-needle"
                    }
                }))
                .expect("serialize indexed content"),
            ]
            .join("\n"),
        )
        .expect("write session");
        let db_path = root.join("memory-forge.db");
        let db = DbState::new(db_path.to_string_lossy().as_ref()).expect("open db");
        {
            let conn = db.conn.lock().expect("db lock");
            database::init_tables(&conn).expect("init tables");
        }
        let settings = AppSettings {
            codex_home: Some(codex_home.to_string_lossy().into_owned()),
            ..AppSettings::default()
        };

        let first = session_list(
            &db,
            &settings,
            "codex",
            Some("unique-index-needle"),
            Some(50),
            0,
            false,
        )
        .expect("initial indexed search");
        assert!(first.items.is_empty());
        assert!(first.search_index.running);

        let deadline = Instant::now() + StdDuration::from_secs(5);
        while Instant::now() < deadline {
            let status = index_jobs()
                .lock()
                .ok()
                .and_then(|jobs| {
                    jobs.get(&format!("{}\u{1f}codex", db.db_path))
                        .map(|state| state.status.clone())
                })
                .unwrap_or_default();
            if !status.running && status.indexed == 1 {
                break;
            }
            thread::sleep(StdDuration::from_millis(10));
        }

        let second = session_list(
            &db,
            &settings,
            "codex",
            Some("unique-index-needle"),
            Some(50),
            0,
            false,
        )
        .expect("completed indexed search");
        assert_eq!(second.items.len(), 1);
        assert_eq!(second.items[0].total_content_matches, 1);

        drop(db);
        fs::remove_dir_all(root).ok();
    }
}
