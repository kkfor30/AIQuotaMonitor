//! 静态 DeepSeek ViewModel。
//!
//! 阶段一用于验证窗口与 UI 框架，不发起真实请求。
//! 时间戳为 epoch 毫秒，在进程启动时基于当前时间生成，由前端负责本地化展示。

// 迁移说明：数据形态参考 DeepSeekMonitorWindows-final（提交 f3ab3ec6）的
// balance/usage 数据结构，但字段按新的多 Source 契约重新建模，非直接复制。

use crate::domain::{
    CapabilityDisplayValue, CapabilitySnapshotViewModel, DataFreshness, PlatformAggregateStatus,
    PlatformSummaryViewModel, SourceState, SourceSummaryViewModel, SourceType, TrendPoint,
};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 返回「现在 minus 分钟数」的 epoch 毫秒。
fn minutes_ago(mins: u64) -> u64 {
    now_epoch_ms().saturating_sub(mins * 60 * 1000)
}

const MINUTE: u64 = 60 * 1000;
const HOUR: u64 = 60 * MINUTE;

pub fn summary() -> PlatformSummaryViewModel {
    let balance_at = minutes_ago(5);
    let web_last_good = minutes_ago(6 * HOUR);

    PlatformSummaryViewModel {
        provider_id: "deepseek".into(),
        display_name: "DeepSeek".into(),
        aggregate_status: PlatformAggregateStatus::Partial,
        access_summary: "API Key + 网页会话".into(),
        sources: vec![
            SourceSummaryViewModel {
                source_id: "balance_api".into(),
                source_type: SourceType::ApiKey,
                display_name: "API 余额".into(),
                state: SourceState::Ready,
                credential_configured: true,
                last_validated_at: Some(balance_at),
                last_success_at: Some(balance_at),
                error_code: None,
                error_message: None,
                capability_ids: vec!["balance".into()],
            },
            SourceSummaryViewModel {
                source_id: "web_session".into(),
                source_type: SourceType::WebSession,
                display_name: "网页用量与缓存".into(),
                state: SourceState::Error,
                credential_configured: true,
                last_validated_at: Some(web_last_good),
                last_success_at: Some(web_last_good),
                error_code: Some("session_expired".into()),
                error_message: Some("网页会话已过期，请重新登录后刷新".into()),
                capability_ids: vec![
                    "today_spend".into(),
                    "month_spend".into(),
                    "model_usage_v4_flash".into(),
                    "model_usage_v4_pro".into(),
                    "cache_hit_rate".into(),
                    "usage_trend".into(),
                ],
            },
        ],
        capabilities: vec![
            CapabilitySnapshotViewModel {
                capability_id: "balance".into(),
                source_id: "balance_api".into(),
                display_name: "账户余额".into(),
                freshness: DataFreshness::Fresh,
                captured_at: Some(balance_at),
                last_good_at: None,
                value: CapabilityDisplayValue {
                    kind: "money".into(),
                    primary: Some("¥110.50".into()),
                    secondary: Some("含赠送金额 ¥10.00".into()),
                    progress: None,
                },
                trend: vec![],
            },
            CapabilitySnapshotViewModel {
                capability_id: "today_spend".into(),
                source_id: "web_session".into(),
                display_name: "今日消费".into(),
                freshness: DataFreshness::Stale,
                captured_at: Some(web_last_good),
                last_good_at: Some(web_last_good),
                value: CapabilityDisplayValue {
                    kind: "money".into(),
                    primary: Some("¥6.42".into()),
                    secondary: Some("上次成功快照".into()),
                    progress: None,
                },
                trend: vec![],
            },
            CapabilitySnapshotViewModel {
                capability_id: "month_spend".into(),
                source_id: "web_session".into(),
                display_name: "本月消费".into(),
                freshness: DataFreshness::Stale,
                captured_at: Some(web_last_good),
                last_good_at: Some(web_last_good),
                value: CapabilityDisplayValue {
                    kind: "money".into(),
                    primary: Some("¥89.15".into()),
                    secondary: Some("月额度 ¥200.00".into()),
                    progress: Some(0.446),
                },
                trend: vec![],
            },
            CapabilitySnapshotViewModel {
                capability_id: "model_usage_v4_flash".into(),
                source_id: "web_session".into(),
                display_name: "V4 Flash 用量".into(),
                freshness: DataFreshness::Stale,
                captured_at: Some(web_last_good),
                last_good_at: Some(web_last_good),
                value: CapabilityDisplayValue {
                    kind: "tokens".into(),
                    primary: Some("1.28M".into()),
                    secondary: Some("缓存 · 可能过期".into()),
                    progress: None,
                },
                trend: vec![],
            },
            CapabilitySnapshotViewModel {
                capability_id: "model_usage_v4_pro".into(),
                source_id: "web_session".into(),
                display_name: "V4 Pro 用量".into(),
                freshness: DataFreshness::Stale,
                captured_at: Some(web_last_good),
                last_good_at: Some(web_last_good),
                value: CapabilityDisplayValue {
                    kind: "tokens".into(),
                    primary: Some("3.42M".into()),
                    secondary: Some("缓存 · 可能过期".into()),
                    progress: None,
                },
                trend: vec![],
            },
            CapabilitySnapshotViewModel {
                capability_id: "cache_hit_rate".into(),
                source_id: "web_session".into(),
                display_name: "缓存命中率".into(),
                freshness: DataFreshness::Stale,
                captured_at: Some(web_last_good),
                last_good_at: Some(web_last_good),
                value: CapabilityDisplayValue {
                    kind: "percent".into(),
                    primary: Some("87.3%".into()),
                    secondary: Some("缓存 · 可能过期".into()),
                    progress: Some(0.873),
                },
                trend: vec![],
            },
            CapabilitySnapshotViewModel {
                capability_id: "usage_trend".into(),
                source_id: "web_session".into(),
                display_name: "近 7 日消费趋势".into(),
                freshness: DataFreshness::Stale,
                captured_at: Some(web_last_good),
                last_good_at: Some(web_last_good),
                value: CapabilityDisplayValue {
                    kind: "trend".into(),
                    primary: None,
                    secondary: Some("缓存 · 可能过期".into()),
                    progress: None,
                },
                trend: vec![
                    TrendPoint { label: "08-21".into(), value: 9.8 },
                    TrendPoint { label: "08-22".into(), value: 14.2 },
                    TrendPoint { label: "08-23".into(), value: 7.4 },
                    TrendPoint { label: "08-24".into(), value: 18.9 },
                    TrendPoint { label: "08-25".into(), value: 12.6 },
                    TrendPoint { label: "08-26".into(), value: 20.8 },
                    TrendPoint { label: "08-27".into(), value: 6.4 },
                ],
            },
        ],
    }
}
