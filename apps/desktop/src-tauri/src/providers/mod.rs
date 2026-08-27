//! 平台 Provider 注册表。
//!
//! 阶段一：DeepSeek 提供静态 ViewModel，其余平台为「需配置」占位条目。
//! 阶段二起逐个替换为真实 Source 适配器。

pub mod deepseek_static;

use crate::domain::{
    PlatformAggregateStatus, PlatformSummaryViewModel, SourceState, SourceSummaryViewModel,
    SourceType,
};

/// 返回平台目录需要的全部平台摘要。
/// 静态数据在进程内构造一次，主窗口与悬浮球消费同一份。
pub fn platform_summaries() -> Vec<PlatformSummaryViewModel> {
    vec![
        deepseek_static::summary(),
        setup_required_placeholder("openai", "GPT / Codex"),
        setup_required_placeholder("claude_code", "Claude Code"),
        setup_required_placeholder("glm", "GLM"),
        setup_required_placeholder("kimi", "Kimi"),
        setup_required_placeholder("mimo", "MiMo"),
        setup_required_placeholder("minimax", "MiniMax"),
    ]
}

/// 未配置平台：不携带任何示例数值，只带接入引导信息。
fn setup_required_placeholder(provider_id: &str, display_name: &str) -> PlatformSummaryViewModel {
    PlatformSummaryViewModel {
        provider_id: provider_id.into(),
        display_name: display_name.into(),
        aggregate_status: PlatformAggregateStatus::SetupRequired,
        access_summary: "尚未接入".into(),
        sources: vec![SourceSummaryViewModel {
            source_id: "default".into(),
            source_type: SourceType::ApiKey,
            display_name: "默认来源".into(),
            state: SourceState::AuthRequired,
            credential_configured: false,
            last_validated_at: None,
            last_success_at: None,
            error_code: None,
            error_message: None,
            capability_ids: vec![],
        }],
        capabilities: vec![],
    }
}
