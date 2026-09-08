//! 本机 Codex 额度观察：相邻快照先固化为 observation，后续 no_change 不覆盖历史重置事实。
//! 重置卡数量观察固化 banked_reset_count 的整数增加（到账）与减少（未判定为已使用）。

use crate::storage::database::Database;
use crate::storage::repository::{
    BankedResetObservationRecord, CapabilityValueSample, QuotaResetObservationRecord,
    RadarEventRecord, WindowSampleRecord,
};
use serde::Serialize;

const RECOVER_MIN: f64 = 0.10;
const SCHEDULE_TOLERANCE_MS: i64 = 30 * 60_000;
const SAMPLE_LOOKBACK_MS: i64 = 30 * 86_400_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaVerificationView {
    pub account_id: String,
    pub account_name: String,
    pub source_id: String,
    pub status: String,
    pub attribution: String,
    pub observation_id: Option<i64>,
    pub temporal_correlation: String,
    pub window_id: Option<String>,
    pub window_label: Option<String>,
    pub window_seconds: Option<i64>,
    pub previous: Option<QuotaWindowPointView>,
    pub current: Option<QuotaWindowPointView>,
    pub last_success_at: Option<i64>,
    pub note: Option<String>,
    pub last_reset_observed_at: Option<i64>,
    /// 只读：可用重置卡 0 张 / 可用重置卡 1 张 / 暂无法获取。
    pub banked_reset_label: String,
    /// 该账号最近一次本机观察到的重置卡数量增加。
    pub last_banked_grant_at: Option<i64>,
    pub last_banked_grant_from: Option<i64>,
    pub last_banked_grant_to: Option<i64>,
    /// 该账号最近一次本机观察到的重置卡数量减少；不得单独断言已使用。
    pub last_banked_decrease_at: Option<i64>,
    pub last_banked_decrease_from: Option<i64>,
    pub last_banked_decrease_to: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BankedGrantView {
    pub account_id: String,
    pub account_name: String,
    pub source_id: String,
    pub previous_count: i64,
    pub current_count: i64,
    pub observed_at: i64,
    /// 该来源当前可用张数；字段缺失为 None，禁止补零。
    pub live_count: Option<i64>,
    /// grant = 数量增加（到账）；drop = 数量减少（不能单独判定为已使用）。
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindowPointView {
    pub captured_at: i64,
    pub remaining: Option<f64>,
    pub reset_at: Option<i64>,
}

pub fn materialize_observations(
    database: &Database,
    event: Option<&RadarEventRecord>,
) -> Result<(), String> {
    for source in database.openai_quota_sources()? {
        let samples = database.recent_window_samples(&source.id)?;
        for (previous, current) in all_pairs(&samples) {
            let classification = classify(previous, current);
            if !matches!(
                classification,
                "scheduled" | "possible_reset" | "unscheduled_reset"
            ) {
                continue;
            }
            let linked = event.filter(|value| classification == "unscheduled_reset" && confirms_landing(value,current.captured_at));
            let correlation = linked
                .map(|value| correlation(value, current.captured_at))
                .unwrap_or("none");
            database.insert_quota_reset_observation(&QuotaResetObservationRecord {
                id: 0,
                account_id: source.account_id.clone(),
                source_id: source.id.clone(),
                capability_id: current.capability_id.clone(),
                previous_snapshot_id: previous.id,
                current_snapshot_id: current.id,
                classification: classification.into(),
                observed_at: current.captured_at,
                event_id: linked.map(|value| value.id.clone()),
                temporal_correlation: correlation.into(),
                user_confirmed_at: None,
            })?;
        }
    }
    Ok(())
}

pub fn materialize_banked_grants(
    database: &Database,
    event: Option<&RadarEventRecord>,
) -> Result<(), String> {
    let since = epoch_ms().saturating_sub(SAMPLE_LOOKBACK_MS);
    for source in database.openai_quota_sources()? {
        let samples =
            database.capability_value_samples(&source.id, "banked_reset_count", since)?;
        for (previous, current) in parseable_count_pairs(&samples) {
            if current.1 == previous.1 {
                continue;
            }
            let linked = event.filter(|value| {
                current.1 > previous.1 && confirms_landing(value,current.0.captured_at)
                    && current.0.captured_at > previous.0.captured_at && current.0.captured_at - previous.0.captured_at <= 2 * 3_600_000
            });
            database.insert_banked_reset_observation(&BankedResetObservationRecord {
                id: 0,
                account_id: source.account_id.clone(),
                source_id: source.id.clone(),
                previous_snapshot_id: previous.0.id,
                current_snapshot_id: current.0.id,
                previous_count: previous.1,
                current_count: current.1,
                observed_at: current.0.captured_at,
                event_id: linked.map(|value| value.id.clone()),
            })?;
        }
    }
    Ok(())
}

pub fn assess_quota_verifications(
    database: &Database,
    event: Option<&RadarEventRecord>,
) -> Result<Vec<QuotaVerificationView>, String> {
    let grants = database.banked_reset_observations(None)?;
    let mut result = Vec::new();
    for source in database.openai_quota_sources()? {
        let mut view = empty_view(&source);
        if source.state == "error" || source.state == "auth_required" {
            view.status = "unavailable".into();
            view.note = source
                .error_message
                .clone()
                .or_else(|| Some("额度来源暂不可用".into()));
            view.banked_reset_label = banked_reset_state(database, &source.id)?.0;
            apply_grant(&mut view, &grants);
            result.push(view);
            continue;
        }
        let samples = database.recent_window_samples(&source.id)?;
        let pairs = latest_pairs(&samples);
        let mut window_changes = Vec::new();
        for (previous, current) in &pairs {
            let status = if event.is_some_and(|value| value.first_signal_at > current.captured_at) {
                "pending"
            } else {
                classify(previous, current)
            };
            window_changes.push((status, *previous, *current));
        }
        if let Some((status, previous, current)) = window_changes.iter().max_by_key(|(status, _, current)| {
            (status_rank(status), current.window_seconds.unwrap_or(0))
        }) {
            view.window_id = Some(current.capability_id.clone());
            view.window_label = Some(current.display_name.clone());
            view.window_seconds = current.window_seconds;
            view.previous = Some(point(previous));
            view.current = Some(point(current));
            view.status = (*status).into();
            view.note = Some(status_note(&view.status).into());
            if view.status == "scheduled" {
                view.attribution = "scheduled".into();
            }
        } else {
            view.status = "insufficient_data".into();
            view.note = Some("缺少额度基线快照，成功刷新两次后可观察".into());
        }
        let observations = database
            .quota_reset_observations(Some(&source.id), event.map(|value| value.id.as_str()))?;
        // Confirmed reset facts come only from unscheduled window recovery, never from
        // "I used a reset card" on a possible_reset, and never from scheduled cycle recovery.
        let reset_fact = observations.iter().find(|value| {
            value.classification.as_str() == "unscheduled_reset"
        });
        if let Some(observation) = observations.iter().find(|value| {
            matches!(
                value.classification.as_str(),
                "unscheduled_reset" | "possible_reset"
            )
        }) {
            view.observation_id = Some(observation.id);
            view.last_reset_observed_at = reset_fact.map(|value| value.observed_at);
            view.temporal_correlation = observation.temporal_correlation.clone();
            if observation.user_confirmed_at.is_some() {
                view.attribution = "user_confirmed".into();
            }
            if view.status == "no_change" {
                view.note = Some("本次刷新未见进一步变化；之前已观察到窗口重置".into());
            }
        }
        let (label, previous_count, current_count) = banked_reset_state(database, &source.id)?;
        view.banked_reset_label = label;
        apply_grant(&mut view, &grants);
        if let (Some(previous), Some(current)) = (previous_count, current_count) {
            if current < previous
                && matches!(view.status.as_str(), "unscheduled_reset" | "possible_reset")
            {
                view.note = Some("疑似使用重置卡".into());
            }
        }
        result.push(view);
    }
    // 两个账号在 60 分钟内同时观察到，相关等级提升为 high。
    for index in 0..result.len() {
        let Some(at) = result[index].last_reset_observed_at else {
            continue;
        };
        if result.iter().enumerate().any(|(other, value)| {
            other != index
                && value.account_id != result[index].account_id
                && value
                    .last_reset_observed_at
                    .is_some_and(|peer| (peer - at).abs() <= 60 * 60_000)
        }) {
            result[index].temporal_correlation = "high".into();
        }
    }
    Ok(result)
}

pub fn save_confirmation(
    database: &Database,
    observation_id: i64,
    confirmed_at: i64,
) -> Result<(), String> {
    database.confirm_quota_reset_observation(observation_id, confirmed_at)
}

fn empty_view(source: &crate::storage::repository::SourceRecord) -> QuotaVerificationView {
    QuotaVerificationView {
        account_id: source.account_id.clone(),
        account_name: source.account_name.clone(),
        source_id: source.id.clone(),
        status: "no_change".into(),
        attribution: "unknown".into(),
        observation_id: None,
        temporal_correlation: "none".into(),
        window_id: None,
        window_label: None,
        window_seconds: None,
        previous: None,
        current: None,
        last_success_at: source.last_success_at,
        note: None,
        last_reset_observed_at: None,
        banked_reset_label: "暂无法获取".into(),
        last_banked_grant_at: None,
        last_banked_grant_from: None,
        last_banked_grant_to: None,
        last_banked_decrease_at: None,
        last_banked_decrease_from: None,
        last_banked_decrease_to: None,
    }
}

pub fn banked_reset_observed_at(
    database: &Database,
    event: &RadarEventRecord,
) -> Result<Option<i64>, String> {
    Ok(database
        .banked_reset_observations(None)?
        .into_iter()
        .filter(|item| {
            item.current_count > item.previous_count
                && item.event_id.as_deref() == Some(event.id.as_str())
                && confirms_landing(event,item.observed_at)
                && database.observation_pair_is_contiguous(item.previous_snapshot_id,item.current_snapshot_id).unwrap_or(false)
        })
        .map(|item| item.observed_at)
        .min())
}

pub fn confirming_quota_reset_at(
    database: &Database,
    event: &RadarEventRecord,
) -> Result<Option<i64>, String> {
    Ok(database
        .quota_reset_observations(None, Some(&event.id))?
        .into_iter()
        .filter(|item| {
            item.classification == "unscheduled_reset" && confirms_landing(event, item.observed_at)
                && database.observation_pair_is_contiguous(item.previous_snapshot_id,item.current_snapshot_id).unwrap_or(false)
        })
        .map(|item| item.observed_at)
        .min())
}

pub(super) fn confirms_landing(event: &RadarEventRecord, observed_at: i64) -> bool {
    if observed_at < event.first_signal_at - 2 * 3_600_000 || event.expires_at.is_some_and(|at|observed_at>at) { return false; }
    matches!(correlation(event, observed_at), "high")
        || (event.claimed_landed_at.is_some() && correlation(event, observed_at) == "medium")
}

fn status_rank(status: &str) -> u8 {
    match status {
        "unscheduled_reset" => 4,
        "possible_reset" => 3,
        "scheduled" => 2,
        "pending" => 1,
        _ => 0,
    }
}

pub fn latest_banked_grant(database: &Database) -> Result<Option<BankedGrantView>, String> {
    latest_banked_change(database, true)
}

pub fn latest_banked_decrease(database: &Database) -> Result<Option<BankedGrantView>, String> {
    latest_banked_change(database, false)
}

fn latest_banked_change(
    database: &Database,
    increase: bool,
) -> Result<Option<BankedGrantView>, String> {
    let Some(observation) = database
        .banked_reset_observations(None)?
        .into_iter()
        .find(|item| {
            if increase {
                item.current_count > item.previous_count
            } else {
                item.current_count < item.previous_count
            }
        })
    else {
        return Ok(None);
    };
    let sources = database.openai_quota_sources()?;
    let source = sources.iter().find(|item| item.id == observation.source_id);
    let live_count = match source {
        Some(source) => {
            let samples =
                database.recent_capability_primary_values(&source.id, "banked_reset_count", 1)?;
            samples
                .first()
                .and_then(|(_, value)| parse_count(value.as_deref()))
        }
        None => None,
    };
    Ok(Some(BankedGrantView {
        account_id: observation.account_id,
        account_name: source
            .map(|item| item.account_name.clone())
            .unwrap_or_default(),
        source_id: observation.source_id,
        previous_count: observation.previous_count,
        current_count: observation.current_count,
        observed_at: observation.observed_at,
        live_count,
        kind: if increase { "grant" } else { "drop" }.into(),
    }))
}

fn banked_reset_state(
    database: &Database,
    source_id: &str,
) -> Result<(String, Option<i64>, Option<i64>), String> {
    let samples = database.recent_capability_primary_values(source_id, "banked_reset_count", 2)?;
    let current = samples
        .first()
        .and_then(|(_, value)| parse_count(value.as_deref()));
    let previous = samples
        .get(1)
        .and_then(|(_, value)| parse_count(value.as_deref()));
    let label = match current {
        Some(count) => format!("可用重置卡 {count} 张"),
        None => "暂无法获取".into(),
    };
    Ok((label, previous, current))
}

fn parse_count(value: Option<&str>) -> Option<i64> {
    value?.trim().parse::<i64>().ok().filter(|count| *count >= 0)
}

fn parseable_count_pairs(
    samples: &[CapabilityValueSample],
) -> Vec<((&CapabilityValueSample, i64), (&CapabilityValueSample, i64))> {
    let parsed: Vec<(&CapabilityValueSample, i64)> = samples
        .iter()
        .filter_map(|sample| {
            parse_count(sample.primary_value.as_deref()).map(|count| (sample, count))
        })
        .collect();
    parsed.windows(2).map(|pair| (pair[0], pair[1])).collect()
}

fn apply_grant(view: &mut QuotaVerificationView, grants: &[BankedResetObservationRecord]) {
    if let Some(grant) = grants.iter().find(|item| {
        item.source_id == view.source_id && item.current_count > item.previous_count
    }) {
        view.last_banked_grant_at = Some(grant.observed_at);
        view.last_banked_grant_from = Some(grant.previous_count);
        view.last_banked_grant_to = Some(grant.current_count);
    }
    if let Some(drop) = grants.iter().find(|item| {
        item.source_id == view.source_id && item.current_count < item.previous_count
    }) {
        view.last_banked_decrease_at = Some(drop.observed_at);
        view.last_banked_decrease_from = Some(drop.previous_count);
        view.last_banked_decrease_to = Some(drop.current_count);
    }
}

fn epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn point(value: &WindowSampleRecord) -> QuotaWindowPointView {
    QuotaWindowPointView {
        captured_at: value.captured_at,
        remaining: value.progress,
        reset_at: value.reset_at,
    }
}

fn all_pairs(samples: &[WindowSampleRecord]) -> Vec<(&WindowSampleRecord, &WindowSampleRecord)> {
    let mut result = Vec::new();
    for group in grouped(samples) {
        for pair in group.windows(2) {
            result.push((&pair[0], &pair[1]));
        }
    }
    result
}

fn latest_pairs(samples: &[WindowSampleRecord]) -> Vec<(&WindowSampleRecord, &WindowSampleRecord)> {
    grouped(samples)
        .into_iter()
        .filter_map(|group| {
            (group.len() >= 2).then(|| (&group[group.len() - 2], &group[group.len() - 1]))
        })
        .collect()
}

fn grouped(samples: &[WindowSampleRecord]) -> Vec<&[WindowSampleRecord]> {
    let mut groups = Vec::new();
    let mut start = 0;
    while start < samples.len() {
        let mut end = start + 1;
        while end < samples.len() && samples[end].capability_id == samples[start].capability_id {
            end += 1;
        }
        groups.push(&samples[start..end]);
        start = end;
    }
    groups
}

fn classify(previous: &WindowSampleRecord, current: &WindowSampleRecord) -> &'static str {
    let (Some(prev_remaining), Some(curr_remaining), Some(prev_reset), Some(curr_reset)) = (
        previous.progress,
        current.progress,
        previous.reset_at,
        current.reset_at,
    ) else {
        return "insufficient_data";
    };
    if curr_remaining - prev_remaining < RECOVER_MIN {
        return "no_change";
    }
    if previous.capability_id != current.capability_id || previous.window_seconds != current.window_seconds
        || current.captured_at <= previous.captured_at || current.captured_at - previous.captured_at > 2 * 3_600_000 {
        return "possible_reset";
    }
    let window_ms = current.window_seconds.unwrap_or(0) * 1000;
    let tolerance = SCHEDULE_TOLERANCE_MS.max(window_ms / 10);
    let shift = curr_reset - prev_reset;
    if current.captured_at >= prev_reset - SCHEDULE_TOLERANCE_MS
        && (shift - window_ms).abs() <= tolerance
    {
        "scheduled"
    } else if shift > tolerance && current.captured_at < prev_reset {
        "unscheduled_reset"
    } else {
        "possible_reset"
    }
}

fn correlation(event: &RadarEventRecord, observed_at: i64) -> &'static str {
    if event
        .expected_at
        .is_some_and(|value| (observed_at - value).abs() <= 2 * 3_600_000)
    {
        "high"
    } else if event
        .claimed_landed_at
        .is_some_and(|value| observed_at >= value && observed_at - value <= 12 * 3_600_000)
    {
        "medium"
    } else if observed_at >= event.first_signal_at
        && observed_at - event.first_signal_at <= 24 * 3_600_000
    {
        "low"
    } else {
        "none"
    }
}

/// 观察与事件的时间相关等级（供幂等关联复用；语义与 materialize 时一致）。
pub(crate) fn correlation_for(event: &RadarEventRecord, observed_at: i64) -> &'static str {
    correlation(event, observed_at)
}

fn status_note(status: &str) -> &'static str {
    match status {
        "pending" => "等待事件后的下一次成功额度刷新",
        "scheduled" => "到达原定时间后的正常周期刷新",
        "unscheduled_reset" => "未到原定时间窗口已恢复，重置时间明显后移",
        "possible_reset" => "窗口比例恢复，但结构化证据不足",
        "insufficient_data" => "快照缺少真实窗口字段",
        _ => "已成功刷新，本次未观察到窗口恢复",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_reset_pair_when_a_later_pair_has_no_change() {
        let sample = |id, remaining, reset, at| WindowSampleRecord {
            id,
            capability_id: "quota_window_7d".into(),
            display_name: "7 天窗口".into(),
            progress: Some(remaining),
            captured_at: at,
            window_seconds: Some(604_800),
            reset_at: Some(reset),
        };
        // reset_at 用真实毫秒尺度：原定 10e9，恢复后后移一个窗口（+604_800_000）。
        let values = vec![
            sample(1, 0.12, 10_000_000_000, 100),
            sample(2, 0.94, 10_000_000_000 + 604_800_000, 200),
            sample(3, 0.92, 10_000_000_000 + 604_800_000, 300),
        ];
        let pairs = all_pairs(&values);
        assert_eq!(pairs.len(), 2);
        assert_eq!(classify(pairs[0].0, pairs[0].1), "unscheduled_reset");
        assert_eq!(classify(pairs[1].0, pairs[1].1), "no_change");
    }

    fn temp_db() -> (Database, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "ai-quota-banked-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let database = Database::initialize_at(path.clone()).expect("db");
        (database, path)
    }

    fn insert_count(database: &Database, captured_at: i64, count: Option<&str>) {
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO capability_snapshots(account_id, source_id, capability_id, display_name, value_kind, primary_value, secondary_value, progress, trend_json, captured_at, generation)
                 VALUES ('openai-codex-local','openai-codex-local','banked_reset_count','可用重置卡','count',?1,NULL,NULL,'[]',?2,1)",
                rusqlite::params![count, captured_at],
            )
            .unwrap();
    }

    fn watching_banked_event(first_signal_at: i64) -> RadarEventRecord {
        RadarEventRecord {
            id: "banked-event".into(),
            phase: "watching".into(),
            title: "重置卡可能即将到账".into(),
            summary: None,
            first_signal_at,
            latest_evidence_at: first_signal_at,
            claimed_landed_at: None,
            observed_reset_at: None,
            closed_at: None,
            close_reason: None,
            expected_at: Some(first_signal_at + 60_000),
            expires_at: None,
            state_revision: 0,
            user_confirmed_reset_at: None,
            event_type: "banked_reset".into(),
        }
    }

    #[test]
    fn materializes_count_increases_and_decreases_skips_missing_and_unchanged() {
        let (database, path) = temp_db();
        let t0 = epoch_ms() - 4 * 3_600_000;
        insert_count(&database, t0, Some("1"));
        insert_count(&database, t0 + 1_000, None);
        insert_count(&database, t0 + 2_000, Some("1"));
        insert_count(&database, t0 + 3_000, Some("2"));
        insert_count(&database, t0 + 4_000, Some("2"));
        insert_count(&database, t0 + 5_000, Some("1"));
        materialize_banked_grants(&database, None).unwrap();
        let changes = database.banked_reset_observations(None).unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].previous_count, 2);
        assert_eq!(changes[0].current_count, 1);
        assert_eq!(changes[0].observed_at, t0 + 5_000);
        assert_eq!(changes[1].previous_count, 1);
        assert_eq!(changes[1].current_count, 2);
        assert_eq!(changes[1].observed_at, t0 + 3_000);
        let grant = latest_banked_grant(&database).unwrap().expect("grant");
        assert_eq!(grant.kind, "grant");
        assert_eq!((grant.previous_count, grant.current_count), (1, 2));
        let drop = latest_banked_decrease(&database).unwrap().expect("drop");
        assert_eq!(drop.kind, "drop");
        assert_eq!((drop.previous_count, drop.current_count), (2, 1));
        let event = watching_banked_event(t0 - 60_000);
        database.insert_radar_event(&event).unwrap();
        database.link_banked_grant_observations(&event.id, t0 - 60_000).unwrap();
        assert_eq!(
            banked_reset_observed_at(&database, &event).unwrap(),
            Some(t0 + 3_000)
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn unrelated_grant_and_long_snapshot_gap_do_not_confirm() {
        let (db,path)=temp_db();let now=epoch_ms();
        insert_count(&db,now-5*3600000,Some("1"));insert_count(&db,now,Some("2"));
        let mut event=watching_banked_event(now-3600000);event.expected_at=Some(now);
        db.insert_radar_event(&event).unwrap();materialize_banked_grants(&db,Some(&event)).unwrap();
        assert_eq!(banked_reset_observed_at(&db,&event).unwrap(),None);
        event.expected_at=None;
        assert!(!confirms_landing(&event,now));
        let previous=WindowSampleRecord{id:1,capability_id:"quota_window_7d".into(),display_name:"7 天".into(),progress:Some(0.1),captured_at:now-4*3600000,window_seconds:Some(604800),reset_at:Some(now+86400000)};
        let current=WindowSampleRecord{id:2,progress:Some(0.9),captured_at:now,reset_at:Some(now+604800000),..previous.clone()};
        assert_eq!(classify(&previous,&current),"possible_reset");
        drop(db);let _=std::fs::remove_file(path);
    }

    #[test]
    fn grant_observation_survives_later_unchanged_snapshots() {
        let (database, path) = temp_db();
        let t0 = epoch_ms() - 3 * 3_600_000;
        insert_count(&database, t0, Some("1"));
        insert_count(&database, t0 + 1_000, Some("2"));
        insert_count(&database, t0 + 2_000, Some("2"));
        materialize_banked_grants(&database, None).unwrap();
        let event = watching_banked_event(t0 - 60_000);
        database.insert_radar_event(&event).unwrap();
        database.link_banked_grant_observations(&event.id, t0 - 60_000).unwrap();
        assert_eq!(
            banked_reset_observed_at(&database, &event).unwrap(),
            Some(t0 + 1_000)
        );
        let grant = latest_banked_grant(&database).unwrap().expect("grant");
        assert_eq!(grant.previous_count, 1);
        assert_eq!(grant.current_count, 2);
        assert_eq!(grant.live_count, Some(2));
        let mut later_event = watching_banked_event(t0 + 1_500);
        later_event.id="another-event".into();
        assert_eq!(banked_reset_observed_at(&database, &later_event).unwrap(), None);
        let _ = std::fs::remove_file(path);
    }
}
