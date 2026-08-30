//! Claude Code 本机订阅额度 Source。
//!
//! 只读本机 Claude Code 的 OAuth 凭据并查询官方用量接口：
//! - 凭据：`~/.claude/.credentials.json`，`claudeAiOauth`（兼容 `claude.ai_oauth`）
//!   条目下的 `accessToken`；token 刷新由 Claude CLI 自己负责，本模块不代刷新，
//!   过期时提示用户在终端重新 `claude /login`。
//! - 查询：`GET https://api.anthropic.com/api/oauth/usage`（Bearer + `anthropic-beta:
//!   oauth-2025-04-20`），返回 `five_hour` / `seven_day` / `seven_day_opus` /
//!   `seven_day_sonnet` 等窗口的 `{ utilization, resets_at }`（utilization 为已用百分比）。
//!
//! 凭据与端点口径参考 cc-switch `src-tauri/src/services/subscription.rs`（提交 6243e20a）；
//! 改造为 local_cli Source + Capability 快照与结构化 RefreshError。API Key 模式的
//! Claude Code 没有订阅用量接口，未检测到 OAuth 登录时返回 auth_required 并引导终端登录。

use super::coding_plan::window_capability;
use crate::domain::refresh::{CapabilityData, RefreshError, SourceRefreshOutput};
use crate::providers::money::WEB_UA;
use reqwest::{Client, StatusCode};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

pub const SOURCE_ID: &str = "claude-code-local";

const USAGE_ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const RELOGIN_HINT: &str = "请在终端运行 claude 并执行 /login 重新登录";

/// 已知窗口 → (capability_id, 展示名)。Opus / Sonnet 是同周期内的模型专属子窗口，
/// capability_id 必须与 seven_day 区分，避免同 Source 下互相覆盖。
const KNOWN_WINDOWS: &[(&str, &str, &str)] = &[
    ("five_hour", "quota_window_5h", "5 小时窗口"),
    ("seven_day", "quota_window_7d", "周窗口"),
    ("seven_day_opus", "quota_window_7d_opus", "周窗口 · Opus"),
    ("seven_day_sonnet", "quota_window_7d_sonnet", "周窗口 · Sonnet"),
];

pub fn local_auth_available() -> bool {
    read_access_token().is_some()
}

fn claude_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".claude"))
}

/// `.credentials.json`：`claudeAiOauth`（兼容 `claude.ai_oauth`）→ `accessToken` 非空。
/// 残缺条目不算已登录；expiresAt 过期仍返回 token，由接口 401 给出真实失效错误。
fn read_access_token() -> Option<String> {
    let content = std::fs::read_to_string(claude_home()?.join(".credentials.json")).ok()?;
    let parsed: Value = serde_json::from_str(&content).ok()?;
    let entry = parsed
        .get("claudeAiOauth")
        .or_else(|| parsed.get("claude.ai_oauth"))?;
    entry
        .get("accessToken")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
}

pub async fn fetch(client: &Client) -> SourceRefreshOutput {
    let Some(token) = read_access_token() else {
        return SourceRefreshOutput::failure(RefreshError::new(
            "auth_required",
            format!("未检测到本机 Claude Code 登录。{RELOGIN_HINT}"),
            true,
            false,
        ));
    };
    match fetch_inner(client, &token).await {
        Ok(capabilities) => SourceRefreshOutput::success(capabilities),
        Err(error) => SourceRefreshOutput::failure(error),
    }
}

async fn fetch_inner(client: &Client, token: &str) -> Result<Vec<CapabilityData>, RefreshError> {
    let mut last_transport = None;
    for attempt in 0..2 {
        let response = client
            .get(USAGE_ENDPOINT)
            .bearer_auth(token)
            .header("anthropic-beta", OAUTH_BETA)
            .header("Accept", "application/json")
            .header("User-Agent", WEB_UA)
            .timeout(Duration::from_secs(15))
            .send()
            .await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                last_transport = Some(error);
                if attempt == 0 {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    continue;
                }
                break;
            }
        };
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(RefreshError::new(
                    "credential_expired",
                    format!("Claude 登录已失效。{RELOGIN_HINT}"),
                    true,
                    false,
                ));
            }
            StatusCode::TOO_MANY_REQUESTS => {
                return Err(RefreshError::new(
                    "rate_limited",
                    "Claude 用量请求过于频繁，请稍后重试",
                    false,
                    true,
                ));
            }
            status if status.is_server_error() && attempt == 0 => {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
            status if !status.is_success() => {
                return Err(RefreshError::new(
                    "http_error",
                    format!("Claude 用量查询失败（HTTP {}）", status.as_u16()),
                    false,
                    status.is_server_error(),
                ));
            }
            _ => {}
        }
        let body: Value = response.json().await.map_err(|_| {
            RefreshError::new("response_shape_changed", "Claude 用量返回格式发生变化", false, false)
        })?;
        return parse(&body);
    }
    Err(RefreshError::new(
        "network_error",
        last_transport
            .map(|error| format!("Claude 用量服务暂时无法连接：{error}"))
            .unwrap_or_else(|| "Claude 用量服务暂时无法连接".to_string()),
        false,
        true,
    ))
}

/// 每个窗口 `{ utilization, resets_at }`（utilization 已用百分比）转剩余展示。
/// 无任何已知窗口视为结构变化，不补零；utilization 缺失的窗口跳过。
fn parse(body: &Value) -> Result<Vec<CapabilityData>, RefreshError> {
    let mut capabilities = Vec::new();
    for (key, capability_id, label) in KNOWN_WINDOWS {
        let Some(window) = body.get(*key) else { continue };
        let Some(used) = window.get("utilization").and_then(Value::as_f64) else {
            continue;
        };
        let used = used.clamp(0.0, 100.0);
        let reset = window.get("resets_at").and_then(Value::as_str).filter(|r| !r.is_empty());
        let reset = reset.map(|value| Value::String(value.to_string()));
        capabilities.push(window_capability(capability_id, label, 100.0 - used, reset.as_ref()));
    }
    if capabilities.is_empty() {
        return Err(RefreshError::new(
            "response_shape_changed",
            "Claude 用量返回格式发生变化",
            false,
            false,
        ));
    }
    Ok(capabilities)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_known_windows_as_remaining_percent() {
        let values = parse(&json!({
            "five_hour": { "utilization": 36.2, "resets_at": "2026-08-30T12:00:00Z" },
            "seven_day": { "utilization": 12.0, "resets_at": "2026-09-02T08:00:00Z" },
            "seven_day_opus": { "utilization": 71.0 }
        }))
        .expect("claude usage");
        assert_eq!(values.len(), 3);
        assert_eq!(values[0].capability_id, "quota_window_5h");
        assert_eq!(values[0].primary_value.as_deref(), Some("63.8%"));
        assert_eq!(values[0].progress, Some(0.638));
        assert!(values[0].secondary_value.as_deref().unwrap().contains("已使用 36.2%"));
        assert!(values[0].secondary_value.as_deref().unwrap().contains("重置 "));
        assert_eq!(values[1].capability_id, "quota_window_7d");
        assert_eq!(values[2].capability_id, "quota_window_7d_opus");
        assert_eq!(values[2].display_name, "周窗口 · Opus");
    }

    #[test]
    fn missing_windows_is_shape_change_not_zero() {
        let error = parse(&json!({ "unexpected": true })).expect_err("empty");
        assert_eq!(error.code, "response_shape_changed");
        // utilization 缺失的窗口跳过，不按 0 处理
        let error = parse(&json!({ "five_hour": {} })).expect_err("missing utilization");
        assert_eq!(error.code, "response_shape_changed");
        // 有任一有效窗口即可，其余缺失窗口不出现
        let values = parse(&json!({
            "five_hour": { "utilization": 10.0 },
            "seven_day": { "utilization": "bad" }
        }))
        .expect("one valid window");
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].capability_id, "quota_window_5h");
    }

    #[test]
    fn reads_oauth_token_across_key_aliases() {
        assert_eq!(
            read_token_from(&json!({ "claudeAiOauth": { "accessToken": " token " } })).as_deref(),
            Some("token")
        );
        assert_eq!(
            read_token_from(&json!({ "claude.ai_oauth": { "accessToken": "t2" } })).as_deref(),
            Some("t2")
        );
        assert_eq!(
            read_token_from(&json!({ "claudeAiOauth": { "accessToken": "" } })),
            None
        );
        assert_eq!(read_token_from(&json!({ "other": {} })), None);
    }

    fn read_token_from(content: &Value) -> Option<String> {
        let entry = content.get("claudeAiOauth").or_else(|| content.get("claude.ai_oauth"))?;
        entry
            .get("accessToken")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_string)
    }
}
