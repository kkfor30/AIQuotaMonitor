//! 本机 Codex 额度观察：相邻快照先固化为 observation，后续 no_change 不覆盖历史重置事实。

use crate::storage::database::Database;
use crate::storage::repository::{
    QuotaResetObservationRecord, RadarEventRecord, WindowSampleRecord,
};
use serde::Serialize;

const RECOVER_MIN: f64 = 0.10;
const SCHEDULE_TOLERANCE_MS: i64 = 30 * 60_000;

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
            let linked = event.filter(|value| value.first_signal_at <= current.captured_at);
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

pub fn assess_quota_verifications(
    database: &Database,
    event: Option<&RadarEventRecord>,
) -> Result<Vec<QuotaVerificationView>, String> {
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
            result.push(view);
            continue;
        }
        let samples = database.recent_window_samples(&source.id)?;
        let pairs = latest_pairs(&samples);
        if let Some((previous, current)) = pairs
            .iter()
            .max_by_key(|(_, current)| current.window_seconds.unwrap_or(0))
        {
            view.window_id = Some(current.capability_id.clone());
            view.window_label = Some(current.display_name.clone());
            view.window_seconds = current.window_seconds;
            view.previous = Some(point(previous));
            view.current = Some(point(current));
            view.status = if event.is_some_and(|value| value.first_signal_at > current.captured_at)
            {
                "pending".into()
            } else {
                classify(previous, current).into()
            };
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
        // “已重置”事实只来自计划外重置观察或用户人工确认；单纯 possible_reset
        // （如 5h 窗口正常轮换因 reset_at 顺延被误判）不得冒充重置时间。
        let reset_fact = observations.iter().find(|value| {
            value.classification.as_str() == "unscheduled_reset" || value.user_confirmed_at.is_some()
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
    }
}

pub fn banked_reset_observed_at(
    database: &Database,
    event: &RadarEventRecord,
) -> Result<Option<i64>, String> {
    let mut observed = None;
    for source in database.openai_quota_sources()? {
        let samples = database.recent_capability_primary_values(&source.id, "banked_reset_count", 2)?;
        if samples.len() < 2 {
            continue;
        }
        let (current_at, current_value) = &samples[0];
        let previous_value = &samples[1].1;
        let Some(current) = parse_count(current_value.as_deref()) else {
            continue;
        };
        let Some(previous) = parse_count(previous_value.as_deref()) else {
            continue;
        };
        if current > previous && *current_at >= event.first_signal_at {
            observed = Some(observed.map_or(*current_at, |at: i64| at.min(*current_at)));
        }
    }
    Ok(observed)
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
        "medium"
    } else {
        "low"
    }
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
}
