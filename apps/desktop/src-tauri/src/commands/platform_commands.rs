//! 平台数据 Tauri Commands。
//!
//! 阶段一返回进程内静态 ViewModel，主窗口与悬浮球消费同一份数据。
//! 阶段二起替换为真实刷新调度与 SQLite 快照。

use crate::domain::PlatformSummaryViewModel;
use crate::providers;

#[tauri::command]
pub fn get_platform_summaries() -> Result<Vec<PlatformSummaryViewModel>, String> {
    Ok(providers::platform_summaries())
}
