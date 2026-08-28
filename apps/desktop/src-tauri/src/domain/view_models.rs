//! 领域层：前后端共享的 ViewModel 契约。
//!
//! 阶段一仅定义静态数据结构，不涉及数据库与真实平台请求。
//! 金额由后端格式化为字符串下发，前端只渲染、不计算。

use serde::Serialize;

/// 平台级聚合状态：所有 Source 汇总后的健康度。
/// 变体集合是前后端数据契约（前端 lib/types.ts 同步定义）；
/// 阶段一静态数据未覆盖全部变体，阶段二真实 Source 接入后补齐。
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformAggregateStatus {
    /// 全部 Source 正常
    Healthy,
    /// 部分 Source 正常，其余失败或使用缓存
    Partial,
    /// 尚未配置任何凭据
    SetupRequired,
    /// 全部 Source 失败
    Error,
}

/// 数据新鲜度：快照相对当前时间的可用性。
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataFreshness {
    /// 最近一次刷新成功
    Fresh,
    /// Source 已失败，展示的是上次成功快照
    Stale,
    /// 没有任何真实值，禁止补零
    Missing,
}

/// 单个 Source 的运行状态。
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceState {
    Ready,
    Refreshing,
    AuthRequired,
    Error,
}

/// Source 的接入类型。
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    ApiKey,
    WebSession,
    LocalCli,
    OAuth,
}

/// 一个平台条目的汇总视图模型（平台目录、悬浮球共用）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformSummaryViewModel {
    pub provider_id: String,
    pub display_name: String,
    pub aggregate_status: PlatformAggregateStatus,
    pub official_url: Option<String>,
    /// 平台接入方式摘要，例如「API Key + 网页会话」
    pub access_summary: String,
    pub sources: Vec<SourceSummaryViewModel>,
    pub capabilities: Vec<CapabilitySnapshotViewModel>,
    pub refresh_history: Vec<RefreshHistoryEntryViewModel>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialInputViewModel {
    pub label: String,
    pub placeholder: String,
    pub help_text: String,
    /// api_key | bearer_token
    pub secret_kind: String,
}

/// 一种独立数据来源的摘要（接入与来源 Tab 的卡片数据）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSummaryViewModel {
    pub source_id: String,
    pub source_type: SourceType,
    pub display_name: String,
    pub state: SourceState,
    /// 凭据是否已配置（不包含任何明文凭据内容）
    pub credential_configured: bool,
    /// 以下时间均为 epoch 毫秒，由前端负责本地化格式化
    pub last_validated_at: Option<u64>,
    pub last_success_at: Option<u64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    /// 该 Source 覆盖的能力 id 列表
    pub capability_ids: Vec<String>,
    pub credential_input: Option<CredentialInputViewModel>,
    pub supports_interactive_login: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshHistoryEntryViewModel {
    pub id: String,
    pub source_id: String,
    pub source_name: String,
    /// running | success | partial | failed
    pub status: String,
    pub finished_at: Option<u64>,
    pub error_message: Option<String>,
}

/// 能力快照的展示值。后端负责格式化，前端零计算。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDisplayValue {
    /// money | tokens | percent
    pub kind: String,
    /// 已格式化的主值，如 "¥110.50"
    pub primary: Option<String>,
    /// 辅助值，如 "较昨日 +2.1%"
    pub secondary: Option<String>,
    /// 进度型能力的当前比例（0..1），无进度则为 None
    pub progress: Option<f64>,
}

/// 趋势图数据点。value 仅用于渲染，不参与金额汇总。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendPoint {
    pub label: String,
    pub value: f64,
}

/// 某个能力的快照视图模型。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySnapshotViewModel {
    pub capability_id: String,
    pub source_id: String,
    pub display_name: String,
    pub freshness: DataFreshness,
    /// epoch 毫秒
    pub captured_at: Option<u64>,
    /// 上次成功快照时间（stale 时展示），epoch 毫秒
    pub last_good_at: Option<u64>,
    pub value: CapabilityDisplayValue,
    /// 可选的随附趋势
    pub trend: Vec<TrendPoint>,
}
