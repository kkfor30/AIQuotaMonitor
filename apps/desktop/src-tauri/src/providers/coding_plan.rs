//! Kimi / GLM / MiniMax 官方 Coding Plan、Token Plan 额度。
//!
//! 查询 URL、鉴权头和响应字段参考 cc-switch `src-tauri/src/services/coding_plan.rs`
//!（审计基线 6243e20a，MIT）。改写为 Source 级 Capability 快照：百分比窗口用剩余值
//! 展示，失败保留结构化错误，不把 Token Plan 做成路由或代理。

use crate::domain::refresh::{CapabilityData, RefreshError, SourceRefreshOutput};
use crate::providers::money::format_percent;
use reqwest::{Client, StatusCode};
use serde_json::Value;
use std::time::Duration;

pub const KIMI_SOURCE_ID: &str = "kimi-coding-plan";
pub const GLM_SOURCE_ID: &str = "glm-coding-plan";
pub const GLM_INTL_SOURCE_ID: &str = "glm-intl-coding-plan";
pub const MINIMAX_SOURCE_ID: &str = "minimax-coding-plan";
pub const MINIMAX_INTL_SOURCE_ID: &str = "minimax-intl-coding-plan";

pub fn is_coding_plan_source(source_id: &str) -> bool {
    matches!(
        source_id,
        KIMI_SOURCE_ID
            | GLM_SOURCE_ID
            | GLM_INTL_SOURCE_ID
            | MINIMAX_SOURCE_ID
            | MINIMAX_INTL_SOURCE_ID
    )
}

enum AuthStyle {
    Bearer,
    Raw,
}

pub async fn fetch(
    client: &Client,
    source_id: &str,
    api_key: &str,
    base_url: Option<&str>,
) -> SourceRefreshOutput {
    match fetch_inner(client, source_id, api_key, base_url).await {
        Ok(capabilities)
            if capabilities
                .iter()
                .any(|item| item.capability_id.starts_with("quota_window_")) =>
        {
            SourceRefreshOutput::success(capabilities)
        }
        Ok(_) => SourceRefreshOutput::failure(RefreshError::new(
            "missing_quota",
            "已通过官方接口，但未返回可识别的 Token Plan 窗口",
            false,
            false,
        )),
        Err(error) => SourceRefreshOutput::failure(error),
    }
}

async fn fetch_inner(
    client: &Client,
    source_id: &str,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<Vec<CapabilityData>, RefreshError> {
    match source_id {
        KIMI_SOURCE_ID => {
            let body = request_json(
                client,
                &kimi_url(base_url),
                api_key,
                AuthStyle::Bearer,
                "Kimi",
            )
            .await?;
            Ok(parse_kimi(&body))
        }
        GLM_SOURCE_ID => {
            let body = request_json(
                client,
                &glm_url(base_url, false),
                api_key,
                AuthStyle::Raw,
                "GLM",
            )
            .await?;
            Ok(parse_glm(&body))
        }
        GLM_INTL_SOURCE_ID => {
            let body = request_json(
                client,
                &glm_url(base_url, true),
                api_key,
                AuthStyle::Raw,
                "GLM",
            )
            .await?;
            Ok(parse_glm(&body))
        }
        MINIMAX_SOURCE_ID => {
            let body = request_json(
                client,
                &minimax_url(base_url, false),
                api_key,
                AuthStyle::Bearer,
                "MiniMax",
            )
            .await?;
            Ok(parse_minimax(&body)?)
        }
        MINIMAX_INTL_SOURCE_ID => {
            let body = request_json(
                client,
                &minimax_url(base_url, true),
                api_key,
                AuthStyle::Bearer,
                "MiniMax",
            )
            .await?;
            Ok(parse_minimax(&body)?)
        }
        _ => Err(RefreshError::new(
            "unsupported_source",
            "当前版本尚未实现此数据来源",
            false,
            false,
        )),
    }
}

fn kimi_url(base_url: Option<&str>) -> String {
    let base = trim_base(base_url, "https://api.kimi.com/coding");
    if base.ends_with("/usages") {
        base
    } else {
        format!("{base}/v1/usages")
    }
}

fn glm_url(base_url: Option<&str>, intl: bool) -> String {
    let default = if intl {
        "https://api.z.ai"
    } else {
        "https://open.bigmodel.cn"
    };
    let base = trim_base(base_url, default);
    if base.contains("/quota/limit") {
        base
    } else {
        format!("{base}/api/monitor/usage/quota/limit")
    }
}

fn minimax_url(base_url: Option<&str>, intl: bool) -> String {
    let default = if intl {
        "https://api.minimax.io"
    } else {
        "https://api.minimaxi.com"
    };
    let base = trim_base(base_url, default);
    if base.contains("/coding_plan/remains") {
        base
    } else {
        format!("{base}/v1/api/openplatform/coding_plan/remains")
    }
}

fn trim_base(base_url: Option<&str>, fallback: &str) -> String {
    base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .trim_end_matches('/')
        .to_string()
}

async fn request_json(
    client: &Client,
    url: &str,
    api_key: &str,
    auth: AuthStyle,
    platform: &str,
) -> Result<Value, RefreshError> {
    let mut last_transport = None;
    for attempt in 0..2 {
        let mut request = client
            .get(url)
            .header("Accept", "application/json")
            .timeout(Duration::from_secs(15));
        request = match auth {
            AuthStyle::Bearer => request.bearer_auth(api_key.trim()),
            AuthStyle::Raw => request.header("Authorization", api_key.trim()),
        };
        let response = match request.send().await {
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
                    format!("{platform} API Key 无效或已过期"),
                    true,
                    false,
                ));
            }
            StatusCode::TOO_MANY_REQUESTS => {
                return Err(RefreshError::new(
                    "rate_limited",
                    format!("{platform} 额度请求过于频繁，请稍后重试"),
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
                    format!("{platform} 额度查询失败（HTTP {}）", status.as_u16()),
                    false,
                    status.is_server_error(),
                ));
            }
            _ => {}
        }
        let raw = response.bytes().await.map_err(|error| {
            RefreshError::new(
                "network_error",
                format!("{platform} 额度响应读取失败：{error}"),
                false,
                true,
            )
        })?;
        return serde_json::from_slice(&raw).map_err(|_| {
            RefreshError::new(
                "response_shape_changed",
                format!("{platform} 额度返回格式发生变化"),
                false,
                false,
            )
        });
    }
    Err(RefreshError::new(
        "network_error",
        last_transport
            .map(|error| format!("{platform} 额度服务暂时无法连接：{error}"))
            .unwrap_or_else(|| format!("{platform} 额度服务暂时无法连接")),
        false,
        true,
    ))
}

fn parse_kimi(body: &Value) -> Vec<CapabilityData> {
    let mut capabilities = Vec::new();
    if let Some(limits) = body.get("limits").and_then(Value::as_array) {
        for item in limits {
            let Some(detail) = item.get("detail") else {
                continue;
            };
            if let Some(capability) = remaining_from_limit_remaining(
                "quota_window_5h",
                "5 小时窗口",
                detail.get("limit"),
                detail.get("remaining"),
                detail.get("resetTime"),
            ) {
                capabilities.push(capability);
                break;
            }
        }
    }
    if let Some(usage) = body.get("usage") {
        if let Some(capability) = remaining_from_limit_remaining(
            "quota_window_7d",
            "周限额",
            usage.get("limit"),
            usage.get("remaining"),
            usage.get("resetTime"),
        ) {
            capabilities.push(capability);
        }
    }
    capabilities
}

fn parse_glm(body: &Value) -> Vec<CapabilityData> {
    if body.get("success").and_then(Value::as_bool) == Some(false) {
        return Vec::new();
    }
    let Some(data) = body.get("data") else {
        return Vec::new();
    };
    let mut five_hour = None;
    let mut weekly = None;
    let mut unclassified = Vec::new();
    if let Some(limits) = data.get("limits").and_then(Value::as_array) {
        for item in limits {
            let limit_type = item.get("type").and_then(Value::as_str).unwrap_or("");
            if !(limit_type.eq_ignore_ascii_case("TOKENS_LIMIT")
                || limit_type.eq_ignore_ascii_case("CREDIT_LIMIT"))
            {
                continue;
            }
            let used = item.get("percentage").and_then(parse_f64).unwrap_or(0.0);
            let reset = item.get("nextResetTime");
            match item.get("unit").and_then(Value::as_i64) {
                Some(3) if five_hour.is_none() => five_hour = Some((used, reset.cloned())),
                Some(6) if weekly.is_none() => weekly = Some((used, reset.cloned())),
                _ => unclassified.push((used, reset.cloned())),
            }
        }
    }
    unclassified
        .sort_by_key(|(_, reset)| reset.as_ref().and_then(Value::as_i64).unwrap_or(i64::MIN));
    for entry in unclassified {
        if five_hour.is_none() {
            five_hour = Some(entry);
        } else if weekly.is_none() {
            weekly = Some(entry);
        }
    }
    let mut capabilities = Vec::new();
    if let Some((used, reset)) = five_hour {
        capabilities.push(remaining_from_used(
            "quota_window_5h",
            "5 小时窗口",
            used,
            reset.as_ref(),
        ));
    }
    if let Some((used, reset)) = weekly {
        capabilities.push(remaining_from_used(
            "quota_window_7d",
            "周窗口",
            used,
            reset.as_ref(),
        ));
    }
    if let Some(level) = data
        .get("level")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        capabilities.push(CapabilityData {
            capability_id: "plan_level".into(),
            display_name: "订阅计划".into(),
            value_kind: "text".into(),
            primary_value: Some(level.to_string()),
            secondary_value: Some("官方 Coding Plan".into()),
            progress: None,
            trend: vec![],
            window_seconds: None,
            reset_at: None,
        });
    }
    capabilities
}

fn parse_minimax(body: &Value) -> Result<Vec<CapabilityData>, RefreshError> {
    if let Some(base_resp) = body.get("base_resp") {
        let status_code = base_resp
            .get("status_code")
            .and_then(Value::as_i64)
            .unwrap_or(-1);
        if status_code != 0 {
            let message = base_resp
                .get("status_msg")
                .and_then(Value::as_str)
                .unwrap_or("未知错误");
            return Err(RefreshError::new(
                "http_error",
                format!("MiniMax 额度查询失败：{message}"),
                false,
                false,
            ));
        }
    }
    let Some(item) = body
        .get("model_remains")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("model_name").and_then(Value::as_str) == Some("general"))
        })
    else {
        return Ok(Vec::new());
    };
    let mut capabilities = Vec::new();
    if let Some(remain) = item
        .get("current_interval_remaining_percent")
        .and_then(parse_f64)
    {
        capabilities.push(remaining_from_remaining(
            "quota_window_5h",
            "5 小时窗口",
            remain,
            item.get("end_time"),
        ));
    }
    if item.get("current_weekly_status").and_then(Value::as_i64) == Some(1) {
        if let Some(remain) = item
            .get("current_weekly_remaining_percent")
            .and_then(parse_f64)
        {
            capabilities.push(remaining_from_remaining(
                "quota_window_7d",
                "周窗口",
                remain,
                item.get("weekly_end_time"),
            ));
        }
    }
    Ok(capabilities)
}

fn remaining_from_limit_remaining(
    id: &str,
    label: &str,
    limit: Option<&Value>,
    remaining: Option<&Value>,
    reset: Option<&Value>,
) -> Option<CapabilityData> {
    let limit = parse_f64(limit?)?;
    let remaining_value = parse_f64(remaining?)?;
    if limit <= 0.0 {
        return None;
    }
    let remaining_percent = ((remaining_value / limit) * 100.0).clamp(0.0, 100.0);
    Some(window_capability(id, label, remaining_percent, reset))
}

fn remaining_from_used(
    id: &str,
    label: &str,
    used_percent: f64,
    reset: Option<&Value>,
) -> CapabilityData {
    window_capability(id, label, (100.0 - used_percent).clamp(0.0, 100.0), reset)
}

fn remaining_from_remaining(
    id: &str,
    label: &str,
    remaining_percent: f64,
    reset: Option<&Value>,
) -> CapabilityData {
    window_capability(id, label, remaining_percent.clamp(0.0, 100.0), reset)
}

/// 窗口能力构造：remaining 为剩余百分比，reset 支持 ISO 字符串 / 秒 / 毫秒时间戳。
/// grok / claude 等订阅窗口 Source 与 Coding Plan 共用同一展示格式。
pub(crate) fn window_capability(
    id: &str,
    label: &str,
    remaining: f64,
    reset: Option<&Value>,
) -> CapabilityData {
    let used = (100.0 - remaining).clamp(0.0, 100.0);
    let used_int = used.round() as i64;
    CapabilityData {
        capability_id: id.into(),
        display_name: label.into(),
        value_kind: "percent".into(),
        primary_value: Some(format_percent(remaining)),
        secondary_value: Some(match reset.and_then(reset_label) {
            Some(reset) => format!("已使用 {used_int}% · {reset}"),
            None => format!("已使用 {used_int}%"),
        }),
        progress: Some(remaining / 100.0),
        trend: vec![],
        window_seconds: None,
        reset_at: None,
    }
}

fn parse_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
}

fn reset_label(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str().filter(|text| !text.is_empty()) {
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(text) {
            return Some(format!(
                "重置 {}",
                parsed.with_timezone(&chrono::Local).format("%m-%d %H:%M")
            ));
        }
        return Some(format!("重置 {text}"));
    }
    let timestamp = value
        .as_i64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))?;
    if timestamp <= 0 {
        return None;
    }
    let seconds = if timestamp > 10_000_000_000 {
        timestamp / 1000
    } else {
        timestamp
    };
    chrono::DateTime::from_timestamp(seconds, 0).map(|time| {
        format!(
            "重置 {}",
            time.with_timezone(&chrono::Local).format("%m-%d %H:%M")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_kimi_five_hour_and_weekly() {
        let values = parse_kimi(&json!({
            "limits": [{"detail": {"limit": 100, "remaining": 40, "resetTime": "2026-08-28T12:00:00Z"}}],
            "usage": {"limit": "200", "remaining": "50"}
        }));
        assert_eq!(values[0].capability_id, "quota_window_5h");
        assert_eq!(values[0].primary_value.as_deref(), Some("40%"));
        assert_eq!(values[1].capability_id, "quota_window_7d");
        assert_eq!(values[1].primary_value.as_deref(), Some("25%"));
    }

    #[test]
    fn parses_glm_unit_windows_and_plan() {
        let values = parse_glm(&json!({
            "success": true,
            "data": {
                "level": "pro",
                "limits": [
                    {"type": "TOKENS_LIMIT", "unit": 6, "percentage": 20, "nextResetTime": 1780000000000i64},
                    {"type": "TOKENS_LIMIT", "unit": 3, "percentage": 10}
                ]
            }
        }));
        assert_eq!(values[0].capability_id, "quota_window_5h");
        assert_eq!(values[0].primary_value.as_deref(), Some("90%"));
        assert_eq!(values[1].capability_id, "quota_window_7d");
        assert_eq!(values[2].primary_value.as_deref(), Some("pro"));
    }

    #[test]
    fn parses_minimax_general_and_skips_inactive_week() {
        let values = parse_minimax(&json!({
            "base_resp": {"status_code": 0},
            "model_remains": [
                {"model_name": "video", "current_interval_remaining_percent": 1},
                {
                    "model_name": "general",
                    "current_interval_remaining_percent": 70,
                    "current_weekly_status": 3,
                    "current_weekly_remaining_percent": 100
                }
            ]
        }))
        .expect("minimax should parse");
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].primary_value.as_deref(), Some("70%"));
    }
}
