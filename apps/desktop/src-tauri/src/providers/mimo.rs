//! MiMo 网页会话余额。没有官方 API Key 余额接口。
//!
//! GET https://platform.xiaomimimo.com/api/v1/balance，鉴权为浏览器 Cookie
//!（serviceToken / api-platform_serviceToken，常为 httpOnly）。
//! 金额按 JSON 原文转 Decimal。查询字段参考 DeepSeekMonitorWindows `providers/mimo.rs`。

use super::money::{cookie_named, format_cny, pick_decimal, WEB_UA};
use crate::domain::refresh::{CapabilityData, RefreshError, SourceRefreshOutput};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use std::time::Duration;

pub const SOURCE_ID: &str = "mimo-web-session";
const BALANCE_URL: &str = "https://platform.xiaomimimo.com/api/v1/balance";

pub async fn fetch(client: &Client, cookie: &str) -> SourceRefreshOutput {
    match fetch_inner(client, cookie).await {
        Ok(capabilities) => SourceRefreshOutput::success(capabilities),
        Err(error) => SourceRefreshOutput::failure(error),
    }
}

pub fn cookie_looks_logged_in(cookie: &str) -> bool {
    let trimmed = cookie.trim();
    trimmed.len() >= 40 && cookie_named(trimmed, "serviceToken")
}

async fn fetch_inner(client: &Client, cookie: &str) -> Result<Vec<CapabilityData>, RefreshError> {
    let cookie = cookie.trim();
    if cookie.is_empty() {
        return Err(RefreshError::new(
            "auth_required",
            "MiMo 网页会话未配置",
            true,
            false,
        ));
    }
    let mut last_transport = None;
    for attempt in 0..2 {
        let response = client
            .get(BALANCE_URL)
            .header("Cookie", cookie)
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
                    "session_expired",
                    "MiMo 登录已过期，请重新登录",
                    true,
                    false,
                ));
            }
            StatusCode::TOO_MANY_REQUESTS => {
                return Err(RefreshError::new(
                    "rate_limited",
                    "MiMo 余额请求过于频繁，请稍后重试",
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
                    format!("MiMo 余额查询失败（HTTP {}）", status.as_u16()),
                    false,
                    status.is_server_error(),
                ));
            }
            _ => {}
        }
        let body: Value = response.json().await.map_err(|_| {
            RefreshError::new(
                "response_shape_changed",
                "MiMo 余额返回格式发生变化",
                false,
                false,
            )
        })?;
        return parse(&body);
    }
    Err(RefreshError::new(
        "network_error",
        last_transport
            .map(|error| format!("MiMo 余额服务暂时无法连接：{error}"))
            .unwrap_or_else(|| "MiMo 余额服务暂时无法连接".to_string()),
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
                .unwrap_or("登录态无效");
            return Err(RefreshError::new(
                "session_expired",
                format!("MiMo 登录状态验证失败：{message}（请重新登录后再试）"),
                true,
                false,
            ));
        }
    } else {
        return Err(RefreshError::new(
            "response_shape_changed",
            "MiMo 余额返回缺少 code 字段",
            false,
            false,
        ));
    }
    let total =
        pick_decimal(body, &["balance", "totalBalance", "availableBalance"]).ok_or_else(|| {
            RefreshError::new(
                "missing_balance",
                "MiMo 未返回可解析的余额字段",
                false,
                false,
            )
        })?;
    let cash = pick_decimal(body, &["cashBalance", "cash_balance"]);
    let gift = pick_decimal(body, &["giftBalance", "gift_balance"]);
    let currency = body
        .get("data")
        .and_then(|data| data.get("currency"))
        .and_then(Value::as_str)
        .unwrap_or("CNY");
    let mut secondary_parts = Vec::new();
    if let Some(cash) = cash {
        secondary_parts.push(format!("现金 {}", format_cny(cash)));
    }
    if let Some(gift) = gift {
        secondary_parts.push(format!("赠送 {}", format_cny(gift)));
    }
    if currency != "CNY" {
        secondary_parts.push(currency.to_string());
    }
    Ok(vec![CapabilityData {
        capability_id: "balance".into(),
        display_name: "账户余额".into(),
        value_kind: "money".into(),
        primary_value: Some(format_cny(total)),
        secondary_value: if secondary_parts.is_empty() {
            Some("网页会话".into())
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
    fn parses_string_and_number_balance_amounts() {
        let values = parse(&json!({
            "code": 0,
            "data": {
                "balance": "16.24",
                "cashBalance": 12,
                "giftBalance": "4.24",
                "currency": "CNY"
            }
        }))
        .expect("mimo balance");
        assert_eq!(values[0].primary_value.as_deref(), Some("¥16.24"));
        assert_eq!(
            values[0].secondary_value.as_deref(),
            Some("现金 ¥12.00 · 赠送 ¥4.24")
        );
    }

    #[test]
    fn treats_nonzero_code_as_expired_session() {
        let error = parse(&json!({"code": 401, "message": "unauthorized"})).unwrap_err();
        assert_eq!(error.code, "session_expired");
        assert!(error.auth_required);
    }

    #[test]
    fn rejects_missing_balance_instead_of_zero() {
        let error = parse(&json!({"code": 0, "data": {"currency": "CNY"}})).unwrap_err();
        assert_eq!(error.code, "missing_balance");
    }

    #[test]
    fn accepts_http_only_service_token_cookie() {
        assert!(cookie_looks_logged_in(
            "other=1; api-platform_serviceToken=abcdefghijklmnopqrstuvwxyz012345; x=2"
        ));
        assert!(!cookie_looks_logged_in("session=abc"));
    }
}
