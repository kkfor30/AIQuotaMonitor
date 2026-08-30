//! 本机 Codex 额度窗口观察器：确定性比较同账号同来源同窗口的相邻成功快照。
//! 网络不可达只表示无法验证（unavailable），不能推导「没有重置」，也不改变雷达事件阶段。
//! 只读结构化 window_seconds / reset_at / progress 字段，禁止反解析 secondary_value 中文文本。

use crate::storage::database::Database;
use serde::Serialize;

/// 剩余比例回升达到该幅度（0..1，即 10 个百分点）视为「窗口恢复」。
const RECOVER_MIN: f64 = 0.10;
/// 周期刷新的 reset_at 允许偏差：取 30 分钟与窗口 10% 的较大值。
const SCHEDULE_TOLERANCE_MS: i64 = 30 * 60_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaVerificationView {
    pub account_id: String,
    pub account_name: String,
    pub source_id: String,
    /// unavailable | insufficient_data | pending | scheduled | possible_reset | unscheduled_reset | no_change
    pub status: String,
    /// unknown | scheduled | user_confirmed | radar_correlated
    pub attribution: String,
    pub window_id: Option<String>,
    pub window_label: Option<String>,
    pub window_seconds: Option<i64>,
    pub previous: Option<QuotaWindowPointView>,
    pub current: Option<QuotaWindowPointView>,
    pub last_success_at: Option<i64>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindowPointView {
    pub captured_at: i64,
    /// 剩余比例 0..1；缺失表示该快照没有真实值。
    pub remaining: Option<f64>,
    pub reset_at: Option<i64>,
}

/// 事件首次信号时间用于 pending 判定与 radar_correlated 归因；无活动事件传 None。
pub fn assess_quota_verifications(
    database: &Database,
    event_first_signal_at: Option<i64>,
) -> Result<Vec<QuotaVerificationView>, String> {
    let sources = database.openai_quota_sources()?;
    let confirmations = load_confirmations(database)?;
    let mut out = Vec::new();
    for source in sources {
        let mut verification = assess_source(database, &source, event_first_signal_at)?;
        if confirmations
            .iter()
            .any(|item| item.account_id == source.account_id && item.source_id == source.id)
        {
            verification.attribution = "user_confirmed".into();
        }
        out.push(verification);
    }
    Ok(out)
}

fn assess_source(
    database: &Database,
    source: &crate::storage::repository::SourceRecord,
    event_first_signal_at: Option<i64>,
) -> Result<QuotaVerificationView, String> {
    let base = QuotaVerificationView {
        account_id: source.account_id.clone(),
        account_name: source.account_name.clone(),
        source_id: source.id.clone(),
        status: "no_change".into(),
        attribution: "unknown".into(),
        window_id: None,
        window_label: None,
        window_seconds: None,
        previous: None,
        current: None,
        last_success_at: source.last_success_at,
        note: None,
    };
    // 凭据或网络失败：只代表无法验证，不代表没有重置。
    if source.state == "error" || source.state == "auth_required" {
        return Ok(QuotaVerificationView {
            status: "unavailable".into(),
            note: source.error_message.clone().or_else(|| Some("额度来源暂不可用".into())),
            ..base
        });
    }
    let samples = database.recent_window_samples(&source.id)?;
    let pairs = adjacent_window_pairs(&samples);
    if pairs.is_empty() {
        return Ok(QuotaVerificationView {
            status: "insufficient_data".into(),
            note: Some("缺少额度基线快照，成功刷新两次后可观察".into()),
            ..base
        });
    }
    // 主判定窗口取最长窗口（Plus 7 天 / Free 30 天）：重置事件观察的是套餐级周期窗口，
    // 5 小时窗口的常规滚动不能冒充套餐窗口的刷新结论。
    let mut candidates: Vec<&(&SnapshotPairInput, &SnapshotPairInput)> = pairs.iter().collect();
    candidates.sort_by_key(|pair| std::cmp::Reverse(pair.0.window_seconds.unwrap_or(0)));
    let primary_pair = candidates[0];
    let mut verification = assess_pair(&base, primary_pair, event_first_signal_at);
    verification.window_id = Some(primary_pair.0.capability_id.clone());
    verification.window_label = Some(primary_pair.0.display_name.clone());
    verification.window_seconds = primary_pair.0.window_seconds;
    Ok(verification)
}

/// 相邻成功快照对的输入。为避免引入中间结构，直接复用 SnapshotRecord 的克隆切片。
type SnapshotPairInput = crate::storage::repository::SnapshotRecord;

fn adjacent_window_pairs(samples: &[SnapshotPairInput]) -> Vec<(&SnapshotPairInput, &SnapshotPairInput)> {
    let mut pairs = Vec::new();
    let mut index = 0;
    while index < samples.len() {
        let capability = &samples[index].capability_id;
        let mut end = index;
        while end < samples.len() && samples[end].capability_id == *capability {
            end += 1;
        }
        let group = &samples[index..end];
        if group.len() >= 2 {
            pairs.push((&group[0], &group[1]));
        }
        index = end;
    }
    pairs
}

fn assess_pair(
    base: &QuotaVerificationView,
    pair: &(&SnapshotPairInput, &SnapshotPairInput),
    event_first_signal_at: Option<i64>,
) -> QuotaVerificationView {
    let (current, previous) = pair;
    let point = |record: &SnapshotPairInput| QuotaWindowPointView {
        captured_at: record.captured_at,
        remaining: record.progress,
        reset_at: record.reset_at,
    };
    let mut verification = QuotaVerificationView {
        previous: Some(point(previous)),
        current: Some(point(current)),
        ..base.clone()
    };
    // 事件发生在最近一次成功快照之后：等待下一次成功刷新再判断。
    if let Some(signal_at) = event_first_signal_at {
        if signal_at > current.captured_at {
            verification.status = "pending".into();
            verification.note = Some("等待事件后的下一次成功额度刷新".into());
            return verification;
        }
    }
    let (Some(prev_remaining), Some(curr_remaining)) = (previous.progress, current.progress) else {
        verification.status = "insufficient_data".into();
        verification.note = Some("快照缺少真实剩余值".into());
        return verification;
    };
    let (Some(prev_reset), Some(curr_reset)) = (previous.reset_at, current.reset_at) else {
        verification.status = "insufficient_data".into();
        verification.note = Some("快照缺少结构化重置时间".into());
        return verification;
    };
    let recovered = curr_remaining - prev_remaining >= RECOVER_MIN;
    if !recovered {
        verification.status = "no_change".into();
        verification.note = Some("已成功刷新，本次未观察到窗口恢复".into());
        return verification;
    }
    let window_ms = current.window_seconds.unwrap_or(0) * 1000;
    let reset_shift = curr_reset - prev_reset;
    let tolerance = SCHEDULE_TOLERANCE_MS.max(window_ms / 10);
    if (reset_shift - window_ms).abs() <= tolerance {
        verification.status = "scheduled".into();
        verification.attribution = "scheduled".into();
        verification.note = Some("到达原定时间后的正常周期刷新".into());
        return verification;
    }
    if curr_reset - prev_reset > tolerance && current.captured_at <= prev_reset {
        verification.status = "unscheduled_reset".into();
        verification.note = Some("未到原定时间窗口已恢复，重置时间明显后移".into());
        if let Some(signal_at) = event_first_signal_at {
            if signal_at >= previous.captured_at && signal_at <= current.captured_at {
                // 事件时间落在前后快照区间内才可关联；仍不是官方全局结论。
                verification.attribution = "radar_correlated".into();
            }
        }
        return verification;
    }
    verification.status = "possible_reset".into();
    verification.note = Some("窗口比例恢复，但结构化证据不足".into());
    verification
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct QuotaConfirmation {
    account_id: String,
    source_id: String,
    #[serde(default)]
    captured_at: i64,
}

/// 用户手动确认的重置归因（主窗口触发），持久化在 settings，最多保留 20 条。
fn load_confirmations(database: &Database) -> Result<Vec<QuotaConfirmation>, String> {
    let Some(raw) = database.setting_string("radar_quota_confirm")? else {
        return Ok(Vec::new());
    };
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

pub fn save_confirmation(
    database: &Database,
    account_id: &str,
    source_id: &str,
    captured_at: i64,
) -> Result<(), String> {
    let mut items = load_confirmations(database)?;
    items.push(QuotaConfirmation {
        account_id: account_id.to_string(),
        source_id: source_id.to_string(),
        captured_at,
    });
    let keep_from = items.len().saturating_sub(20);
    let trimmed = &items[keep_from..];
    let payload = serde_json::to_string(trimmed).unwrap_or_else(|_| "[]".into());
    database.set_setting_string("radar_quota_confirm", &payload)
}
