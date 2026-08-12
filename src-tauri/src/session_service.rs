use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::thread;
use std::time::Instant;

use chrono::{Duration, Local, TimeZone};
use serde::Serialize;

use crate::database::{self, DbState};
use crate::platforms::{
    self, SessionAgentGroup, SessionDetail, SessionKey, SessionListItem, SessionListResult,
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
        let all_items = apply_flags(result.items, &archived, &favorites, show_archived);
        let mut matched_keys = HashSet::new();
        let mut filtered: Vec<SessionListItem> = all_items
            .iter()
            .cloned()
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
                    matched_keys.insert(item.session_key.clone());
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
                        matched_keys.insert(item.session_key.clone());
                        Some(item)
                    } else {
                        None
                    }
                }
            })
            .collect();

        // Keep the complete parent chain for every hit so a matching subagent remains
        // discoverable even though subagents are hidden from the root list by default.
        let included_keys = search_result_tree_keys(&all_items, &matched_keys);
        let hit_items: HashMap<String, SessionListItem> = filtered
            .drain(..)
            .map(|item| (item.session_key.clone(), item))
            .collect();
        filtered = all_items
            .into_iter()
            .filter(|item| included_keys.contains(&item.session_key))
            .map(|item| hit_items.get(&item.session_key).cloned().unwrap_or(item))
            .collect();
        let mut filtered = group_session_items(filtered);
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
            session_tree_keys(&page_result.items),
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

        let (root_indices, children) = session_key_graph(&keys);
        let total = root_indices.len();
        let page_roots: Vec<usize> = root_indices
            .into_iter()
            .skip(offset.min(total))
            .take(limit.unwrap_or(usize::MAX))
            .collect();
        let mut page_indices = Vec::new();
        for root in page_roots {
            collect_descendant_indices(root, &children, &mut page_indices);
        }

        let items = page_indices
            .into_iter()
            .filter_map(|index| adapter.session_list_item(&keys[index].key, aliases, summary_cache))
            .map(|mut item| {
                item.favorite = favorites.contains(&item.session_key);
                item
            })
            .collect();
        let items = group_session_items(items);

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

    let items = group_session_items(items);
    let total = items.len();
    let start = offset.min(total);
    let end = limit.map(|l| (start + l).min(total)).unwrap_or(total);
    let page = items[start..end].to_vec();

    SessionListResult { total, items: page }
}

fn session_key_graph(keys: &[SessionKey]) -> (Vec<usize>, Vec<Vec<usize>>) {
    let id_to_index: HashMap<&str, usize> = keys
        .iter()
        .enumerate()
        .map(|(index, key)| (key.session_id.as_str(), index))
        .collect();
    let mut parents: Vec<Option<usize>> = keys
        .iter()
        .map(|key| {
            key.parent_session_id
                .as_deref()
                .and_then(|id| id_to_index.get(id).copied())
        })
        .collect();
    break_parent_cycles(&mut parents);

    let mut children = vec![Vec::new(); keys.len()];
    let mut roots = Vec::new();
    for (index, parent) in parents.into_iter().enumerate() {
        if let Some(parent) = parent {
            children[parent].push(index);
        } else {
            roots.push(index);
        }
    }
    (roots, children)
}

fn break_parent_cycles(parents: &mut [Option<usize>]) {
    for start in 0..parents.len() {
        let mut current = Some(start);
        let mut visited = HashSet::new();
        while let Some(index) = current {
            if !visited.insert(index) {
                parents[index] = None;
                break;
            }
            current = parents[index];
        }
    }
}

fn collect_descendant_indices(index: usize, children: &[Vec<usize>], output: &mut Vec<usize>) {
    output.push(index);
    for &child in &children[index] {
        collect_descendant_indices(child, children, output);
    }
}

fn group_session_items(items: Vec<SessionListItem>) -> Vec<SessionListItem> {
    if items.is_empty() {
        return items;
    }

    let keys: Vec<SessionKey> = items
        .iter()
        .map(|item| SessionKey {
            key: item.session_key.clone(),
            sort_key: 0,
            session_id: item.session_id.clone(),
            parent_session_id: item
                .agent_group
                .as_ref()
                .and_then(|group| group.parent_session_id.clone()),
        })
        .collect();
    let (roots, children) = session_key_graph(&keys);
    let mut slots: Vec<Option<SessionListItem>> = items.into_iter().map(Some).collect();

    fn build(
        index: usize,
        children: &[Vec<usize>],
        slots: &mut [Option<SessionListItem>],
    ) -> SessionListItem {
        let mut item = slots[index]
            .take()
            .expect("session tree node consumed once");
        let child_items = children[index]
            .iter()
            .map(|&child| build(child, children, slots))
            .collect::<Vec<_>>();
        if !child_items.is_empty() {
            item.agent_group
                .get_or_insert_with(SessionAgentGroup::default)
                .children = child_items;
        }
        item
    }

    roots
        .into_iter()
        .map(|root| {
            let mut item = build(root, &children, &mut slots);
            if item
                .agent_group
                .as_ref()
                .and_then(|group| group.parent_session_id.as_ref())
                .is_some()
            {
                item.agent_group
                    .get_or_insert_with(SessionAgentGroup::default)
                    .orphaned = true;
            }
            item
        })
        .collect()
}

fn session_tree_keys(items: &[SessionListItem]) -> Vec<String> {
    fn collect(item: &SessionListItem, output: &mut Vec<String>) {
        output.push(item.session_key.clone());
        if let Some(group) = &item.agent_group {
            for child in &group.children {
                collect(child, output);
            }
        }
    }

    let mut keys = Vec::new();
    for item in items {
        collect(item, &mut keys);
    }
    keys
}

fn search_result_tree_keys(
    items: &[SessionListItem],
    matched_keys: &HashSet<String>,
) -> HashSet<String> {
    let items_by_id: HashMap<&str, &SessionListItem> = items
        .iter()
        .map(|item| (item.session_id.as_str(), item))
        .collect();
    let mut included_keys = matched_keys.clone();
    for item in items
        .iter()
        .filter(|item| matched_keys.contains(&item.session_key))
    {
        let mut parent_id = item
            .agent_group
            .as_ref()
            .and_then(|group| group.parent_session_id.as_deref());
        let mut visited = HashSet::new();
        while let Some(id) = parent_id {
            if !visited.insert(id) {
                break;
            }
            let Some(parent) = items_by_id.get(id).copied() else {
                break;
            };
            included_keys.insert(parent.session_key.clone());
            parent_id = parent
                .agent_group
                .as_ref()
                .and_then(|group| group.parent_session_id.as_deref());
        }
    }
    included_keys
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

    fn session(id: &str, parent: Option<&str>) -> SessionListItem {
        SessionListItem {
            platform: "codex".to_string(),
            session_key: format!("{id}.jsonl"),
            session_id: id.to_string(),
            display_title: id.to_string(),
            alias_title: String::new(),
            preview: String::new(),
            updated_at: String::new(),
            cwd: String::new(),
            editable: true,
            content_matches: Vec::new(),
            total_content_matches: 0,
            favorite: false,
            agent_group: parent.map(|parent| SessionAgentGroup {
                parent_session_id: Some(parent.to_string()),
                depth: None,
                nickname: None,
                role: None,
                path: None,
                orphaned: false,
                children: Vec::new(),
            }),
        }
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
    fn groups_multilevel_subagents_under_the_root() {
        let grouped = group_session_items(vec![
            session("root", None),
            session("child", Some("root")),
            session("grandchild", Some("child")),
        ]);

        assert_eq!(grouped.len(), 1);
        let children = &grouped[0].agent_group.as_ref().unwrap().children;
        assert_eq!(children[0].session_id, "child");
        assert_eq!(
            children[0].agent_group.as_ref().unwrap().children[0].session_id,
            "grandchild"
        );
    }

    #[test]
    fn orphan_and_cycle_sessions_remain_visible_roots() {
        let grouped = group_session_items(vec![
            session("orphan", Some("missing")),
            session("a", Some("b")),
            session("b", Some("a")),
        ]);

        assert_eq!(grouped.len(), 2);
        assert!(grouped.iter().any(|item| {
            item.session_id == "orphan" && item.agent_group.as_ref().unwrap().orphaned
        }));
        assert_eq!(session_tree_keys(&grouped).len(), 3);
    }

    #[test]
    fn key_graph_paginates_only_root_sessions() {
        let keys = vec![
            SessionKey::standalone("root-a".to_string(), 3),
            SessionKey {
                key: "child".to_string(),
                sort_key: 2,
                session_id: "child".to_string(),
                parent_session_id: Some("root-a".to_string()),
            },
            SessionKey::standalone("root-b".to_string(), 1),
        ];

        let (roots, children) = session_key_graph(&keys);
        assert_eq!(roots, vec![0, 2]);
        let mut first_page = Vec::new();
        collect_descendant_indices(roots[0], &children, &mut first_page);
        assert_eq!(first_page, vec![0, 1]);
    }

    #[test]
    fn search_hit_on_subagent_includes_its_parent_chain() {
        let items = vec![
            session("root", None),
            session("child", Some("root")),
            session("grandchild", Some("child")),
        ];
        let matched = HashSet::from(["grandchild.jsonl".to_string()]);

        let included = search_result_tree_keys(&items, &matched);

        assert_eq!(included.len(), 3);
        assert!(included.contains("root.jsonl"));
        assert!(included.contains("child.jsonl"));
    }
}
