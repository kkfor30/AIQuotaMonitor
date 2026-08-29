//! Grok CLI（SuperGrok）本机订阅额度 Source。
//!
//! 只读本机 Grok CLI 的 OAuth 凭据（`~/.grok/auth.json`，scope → 条目 map，
//! `key` 即 Bearer token）；token 刷新由 Grok CLI 自己负责，本模块不代刷新、
//! 不把凭据写入本项目存储或前端。过期时提示用户在终端重新 `grok login`。
//!
//! 查询端点来自本机 Grok CLI 实测（`~/.grok/logs/unified.jsonl` 抓取）：
//! `GET https://cli-chat-proxy.grok.com/v1/billing?format=credits`
//! 返回 `{ config: { creditUsagePercent, currentPeriod { start, end }, prepaidBalance } }`，
//! `creditUsagePercent` 是已用百分比（图 /usage 面板的 Weekly limit）。
//! 该端点无公开文档；对照实现为 cc-switch `src-tauri/src/services/subscription_grok.rs`
//! （提交 6243e20a，移植自 CodexBar，走 grok.com gRPC-web 备用路径）。
//! 本模块改造为：JSON 端点 + Capability 快照 + 结构化 RefreshError；
//! proto3 会省略零值 percent 字段，仅有周期无百分比时按 0% 特判，
//! 两者都缺才报 response_shape_changed，不补零不造数。

use crate::domain::refresh::{CapabilityData, RefreshError, SourceRefreshOutput};
use crate::providers::money::{format_percent, WEB_UA};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

pub const SOURCE_ID: &str = "grok-cli-local";

const BILLING_ENDPOINT: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
/// SuperGrok（OIDC）条目的 scope 前缀。
const OIDC_SCOPE_PREFIX: &str = "https://auth.x.ai::";
const RELOGIN_HINT: &str = "请在终端运行 grok login 重新登录";

pub fn local_auth_available() -> bool {
    read_access_token().is_some()
}

fn grok_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".grok"))
}

/// auth.json 顶层是 scope → 条目 map：优先 SuperGrok OIDC 条目，
/// 回退旧版 `accounts.x.ai/sign-in` 会话条目；`key` 为空的残缺条目不遮蔽可用条目
/// （口径对齐 cc-switch `select_preferred_entry`）。
fn read_access_token() -> Option<String> {
    let content = std::fs::read_to_string(grok_home()?.join("auth.json")).ok()?;
    let parsed: Value = serde_json::from_str(&content).ok()?;
    select_access_token(parsed.as_object()?)
}

fn select_access_token(root: &serde_json::Map<String, Value>) -> Option<String> {
    let mut oidc = None;
    let mut legacy = None;
    for (scope, value) in root {
        let Some(entry) = value.as_object() else { continue };
        let Some(key) = entry
            .get("key")
            .and_then(Value::as_str)
            .filter(|key| !key.is_empty())
        else {
            continue;
        };
        if scope.starts_with(OIDC_SCOPE_PREFIX) {
            oidc = Some(key.to_string());
        } else if scope.contains("/sign-in") {
            legacy = Some(key.to_string());
        }
    }
    oidc.or(legacy)
}

pub async fn fetch(client: &Client) -> SourceRefreshOutput {
    let Some(token) = read_access_token() else {
        return SourceRefreshOutput::failure(RefreshError::new(
            "auth_required",
            format!("未检测到本机 Grok CLI 登录。{RELOGIN_HINT}"),
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
            .get(BILLING_ENDPOINT)
            .bearer_auth(token)
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
                    format!("Grok CLI 登录已失效。{RELOGIN_HINT}"),
                    true,
                    false,
                ));
            }
            StatusCode::TOO_MANY_REQUESTS => {
                return Err(RefreshError::new(
                    "rate_limited",
                    "Grok 额度请求过于频繁，请稍后重试",
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
                    format!("Grok 额度查询失败（HTTP {}）", status.as_u16()),
                    false,
                    status.is_server_error(),
                ));
            }
            _ => {}
        }
        let body: Value = response.json().await.map_err(|_| {
            RefreshError::new("response_shape_changed", "Grok 额度返回格式发生变化", false, false)
        })?;
        return parse(&body);
    }
    Err(RefreshError::new(
        "network_error",
        last_transport
            .map(|error| format!("Grok 额度服务暂时无法连接：{error}"))
            .unwrap_or_else(|| "Grok 额度服务暂时无法连接".to_string()),
        false,
        true,
    ))
}

fn parse(body: &Value) -> Result<Vec<CapabilityData>, RefreshError> {
    let config = body
        .get("config")
        .ok_or_else(|| RefreshError::new("response_shape_changed", "Grok 额度返回格式发生变化", false, false))?;
    let period_end = config
        .pointer("/currentPeriod/end")
        .and_then(Value::as_str)
        .filter(|end| !end.is_empty());
    // proto3 零值省略：有周期但无 creditUsagePercent 视为 0%；两者都缺才是结构变化。
    let used = match (
        config.get("creditUsagePercent").and_then(Value::as_f64),
        config.get("currentPeriod").is_some(),
    ) {
        (Some(value), _) => value,
        (None, true) => 0.0,
        (None, false) => {
            return Err(RefreshError::new(
                "response_shape_changed",
                "Grok 额度返回格式发生变化",
                false,
                false,
            ))
        }
    };
    let used = used.clamp(0.0, 100.0);
    let remaining = 100.0 - used;
    let used_text = format_percent(used);
    let secondary = match period_end.and_then(reset_label) {
        Some(reset) => format!("已使用 {used_text} · {reset}"),
        None => format!("已使用 {used_text}"),
    };
    Ok(vec![CapabilityData {
        capability_id: "quota_window_7d".into(),
        display_name: "周窗口".into(),
        value_kind: "percent".into(),
        primary_value: Some(format_percent(remaining)),
        secondary_value: Some(secondary),
        progress: Some(remaining / 100.0),
        trend: vec![],
    }])
}

fn reset_label(iso: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|time| format!("重置 {}", time.with_timezone(&chrono::Local).format("%m-%d %H:%M")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_weekly_window_as_remaining_percent() {
        let values = parse(&json!({
            "config": {
                "creditUsagePercent": 73.0,
                "currentPeriod": {
                    "type": "USAGE_PERIOD_TYPE_WEEKLY",
                    "start": "2026-08-28T07:19:04.698807+00:00",
                    "end": "2026-09-04T07:19:04.698807+00:00"
                },
                "prepaidBalance": { "val": 0 }
            }
        }))
        .expect("grok billing");
        assert_eq!(values[0].capability_id, "quota_window_7d");
        assert_eq!(values[0].value_kind, "percent");
        assert_eq!(values[0].primary_value.as_deref(), Some("27%"));
        assert!(values[0].secondary_value.as_deref().unwrap().contains("已使用 73%"));
        assert!(values[0].secondary_value.as_deref().unwrap().contains("重置 "));
        assert_eq!(values[0].progress, Some(0.27));
    }

    #[test]
    fn zero_usage_without_percent_field_reads_as_zero() {
        let values = parse(&json!({
            "config": { "currentPeriod": { "end": "2026-09-04T07:19:04+00:00" } }
        }))
        .expect("zero usage");
        assert_eq!(values[0].primary_value.as_deref(), Some("100%"));
    }

    #[test]
    fn missing_percent_and_period_is_shape_change() {
        let error = parse(&json!({ "config": { "prepaidBalance": { "val": 0 } } })).expect_err("shape");
        assert_eq!(error.code, "response_shape_changed");
        let error = parse(&json!({ "unexpected": true })).expect_err("no config");
        assert_eq!(error.code, "response_shape_changed");
    }

    #[test]
    fn auth_entry_selection_prefers_oidc_over_legacy() {
        let oidc = format!("{OIDC_SCOPE_PREFIX}client-id");
        let content = json!({
            "https://accounts.x.ai/sign-in": { "key": "legacy-token" },
            &oidc: { "key": "oidc-token", "expires_at": "2099-01-01T00:00:00Z" }
        });
        assert_eq!(select_access_token(content.as_object().unwrap()).as_deref(), Some("oidc-token"));

        let broken = json!({ &oidc: { "key": "" }, "https://accounts.x.ai/sign-in": { "key": "legacy-token" } });
        assert_eq!(
            select_access_token(broken.as_object().unwrap()).as_deref(),
            Some("legacy-token")
        );

        let other = json!({ "other": { "key": "x" } });
        assert_eq!(select_access_token(other.as_object().unwrap()), None);
    }
}
