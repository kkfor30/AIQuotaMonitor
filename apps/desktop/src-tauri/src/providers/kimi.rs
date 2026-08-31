//! Kimi 开放平台个人余额。
//!
//! 官方接口：GET https://api.moonshot.cn/v1/users/me/balance
//! 文档：https://platform.moonshot.cn/docs/api/balance
//! 与 Coding Plan（api.kimi.com/coding）是另一套 Key，不共用请求地址。
//! 金额按 JSON 原文转 Decimal。查询与字段参考 DeepSeekMonitorWindows `providers/moonshot.rs`。

use super::money::{decimal_from_json, format_cny, pick_decimal};
use crate::domain::refresh::{CapabilityData, RefreshError, SourceRefreshOutput};
use reqwest::{Client, StatusCode};
use rust_decimal::Decimal;
use serde_json::Value;
use std::time::Duration;

pub const BALANCE_SOURCE_ID: &str = "kimi-balance-api";
const DEFAULT_BASE: &str = "https://api.moonshot.cn";

pub async fn fetch(client: &Client, api_key: &str, base_url: Option<&str>) -> SourceRefreshOutput {
    match fetch_inner(client, api_key, base_url).await {
        Ok(capabilities) => SourceRefreshOutput::success(capabilities),
        Err(error) => SourceRefreshOutput::failure(error),
    }
}

fn balance_url(base_url: Option<&str>) -> String {
    let base = match base_url.map(str::trim).filter(|value| !value.is_empty()) {
        Some(url) if url.contains("moonshot") => url.trim_end_matches('/'),
        _ => DEFAULT_BASE,
    };
    if base.contains("/users/me/balance") {
        base.to_string()
    } else if base.ends_with("/v1") {
        format!("{base}/users/me/balance")
    } else {
        format!("{base}/v1/users/me/balance")
    }
}

async fn fetch_inner(
    client: &Client,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<Vec<CapabilityData>, RefreshError> {
    let endpoint = balance_url(base_url);
    let mut last_transport = None;
    for attempt in 0..2 {
        let response = client
            .get(&endpoint)
            .bearer_auth(api_key.trim())
            .header("Accept", "application/json")
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
                    "Kimi 开放平台 API Key 无效或已过期",
                    true,
                    false,
                ));
            }
            StatusCode::TOO_MANY_REQUESTS => {
                return Err(RefreshError::new(
                    "rate_limited",
                    "Kimi 余额请求过于频繁，请稍后重试",
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
                    format!("Kimi 余额查询失败（HTTP {}）", status.as_u16()),
                    false,
                    status.is_server_error(),
                ));
            }
            _ => {}
        }
        let body: Value = response.json().await.map_err(|_| {
            RefreshError::new(
                "response_shape_changed",
                "Kimi 余额返回格式发生变化",
                false,
                false,
            )
        })?;
        return parse(&body);
    }
    Err(RefreshError::new(
        "network_error",
        last_transport
            .map(|error| format!("Kimi 余额服务暂时无法连接：{error}"))
            .unwrap_or_else(|| "Kimi 余额服务暂时无法连接".to_string()),
        false,
        true,
    ))
}

fn parse(body: &Value) -> Result<Vec<CapabilityData>, RefreshError> {
    if let Some(code) = body.get("code").and_then(Value::as_i64) {
        if code != 0 {
            let message = body
                .get("message")
                .or_else(|| body.get("msg"))
                .and_then(Value::as_str)
                .unwrap_or("平台返回业务错误");
            return Err(RefreshError::new(
                "http_error",
                format!("Kimi 余额接口错误：{message}"),
                false,
                false,
            ));
        }
    }
    let available = pick_decimal(
        body,
        &[
            "available_balance",
            "availableBalance",
            "total_balance",
            "totalBalance",
            "balance",
        ],
    )
    .ok_or_else(|| {
        RefreshError::new(
            "missing_balance",
            "Kimi 未返回可解析的余额字段",
            false,
            false,
        )
    })?;
    let voucher = pick_decimal(body, &["voucher_balance", "voucherBalance"]);
    let cash = pick_decimal(body, &["cash_balance", "cashBalance"]);
    let data = body.get("data").unwrap_or(body);
    let unavailable = data
        .get("available_balance")
        .and_then(decimal_from_json)
        .is_some_and(|value| value <= Decimal::ZERO);
    let mut secondary_parts = Vec::new();
    if let Some(voucher) = voucher {
        secondary_parts.push(format!("代金券 {}", format_cny(voucher)));
    }
    if let Some(cash) = cash {
        secondary_parts.push(format!("现金 {}", format_cny(cash)));
    }
    if unavailable {
        secondary_parts.push("可用余额不足".into());
    }
    Ok(vec![CapabilityData {
        capability_id: "balance".into(),
        display_name: "账户余额".into(),
        value_kind: "money".into(),
        primary_value: Some(format_cny(available)),
        secondary_value: if secondary_parts.is_empty() {
            Some("Moonshot 开放平台".into())
        } else {
            Some(secondary_parts.join(" · "))
        },
        progress: None,
        trend: vec![],
        window_seconds: None,
        reset_at: None,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn uses_official_moonshot_url_not_coding_plan_base() {
        assert_eq!(
            balance_url(Some("https://api.kimi.com/coding")),
            "https://api.moonshot.cn/v1/users/me/balance"
        );
        assert_eq!(
            balance_url(Some("https://api.moonshot.cn/v1")),
            "https://api.moonshot.cn/v1/users/me/balance"
        );
    }

    #[test]
    fn parses_official_balance_fields_as_decimal() {
        let values = parse(&json!({
            "code": 0,
            "data": {
                "available_balance": 13.22,
                "voucher_balance": "10.00",
                "cash_balance": 3.22
            }
        }))
        .expect("kimi balance");
        assert_eq!(values[0].primary_value.as_deref(), Some("¥13.22"));
        assert_eq!(
            values[0].secondary_value.as_deref(),
            Some("代金券 ¥10.00 · 现金 ¥3.22")
        );
    }

    #[test]
    fn rejects_missing_balance_instead_of_zero() {
        let error = parse(&json!({"code": 0, "data": {"foo": 1}})).expect_err("missing");
        assert_eq!(error.code, "missing_balance");
    }
}
