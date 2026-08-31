//! GLM 网页个人余额。官方 Coding Plan 不覆盖按量账户余额。
//!
//! 登录 Cookie 中的 `bigmodel_token_production` 作为 Authorization（不加 Bearer），
//! 查询 GET https://open.bigmodel.cn/api/biz/account/query-customer-account-report。
//! 这是控制台内部接口，不是官方开放余额 API。金额按 JSON 原文转 Decimal。

use super::money::{extract_token_cookie, format_cny, pick_decimal, WEB_UA};
use crate::domain::refresh::{CapabilityData, RefreshError, SourceRefreshOutput};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use std::time::Duration;

pub const WEB_BALANCE_SOURCE_ID: &str = "glm-web-balance";
const BALANCE_URLS: &[&str] = &[
    "https://open.bigmodel.cn/api/biz/account/query-customer-account-report",
    "https://open.bigmodel.cn/api/biz/customer/getCustomerInfo",
];
const REFERER: &str = "https://open.bigmodel.cn/usercenter/financialoverview";

pub async fn fetch(client: &Client, secret: &str) -> SourceRefreshOutput {
    match fetch_inner(client, secret).await {
        Ok(capabilities) => SourceRefreshOutput::success(capabilities),
        Err(error) => SourceRefreshOutput::failure(error),
    }
}

fn authorization_token(secret: &str) -> Result<String, RefreshError> {
    if let Some(token) = extract_token_cookie(secret) {
        return Ok(token);
    }
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        return Err(RefreshError::new(
            "auth_required",
            "GLM 网页会话未配置",
            true,
            false,
        ));
    }
    if trimmed.contains('=') && trimmed.contains(';') {
        return Err(RefreshError::new(
            "session_expired",
            "GLM 登录态缺少可用 Token Cookie，请重新完成网页登录",
            true,
            false,
        ));
    }
    if trimmed.len() < 20 {
        return Err(RefreshError::new(
            "session_expired",
            "GLM 登录态无效，请重新完成网页登录",
            true,
            false,
        ));
    }
    Ok(trimmed.trim_start_matches("Bearer ").trim().to_string())
}

async fn fetch_inner(client: &Client, secret: &str) -> Result<Vec<CapabilityData>, RefreshError> {
    let token = authorization_token(secret)?;
    let cookie = cookie_header(secret, &token);
    let mut last_error = None;
    for url in BALANCE_URLS {
        match request_balance(client, url, &token, cookie.as_deref()).await {
            Ok(body) => match parse(&body) {
                Ok(capabilities) => return Ok(capabilities),
                Err(error) => last_error = Some(error),
            },
            Err(error) if error.auth_required => return Err(error),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        RefreshError::new("network_error", "GLM 余额服务暂时无法连接", false, true)
    }))
}

fn cookie_header(secret: &str, token: &str) -> Option<String> {
    let trimmed = secret.trim();
    if trimmed.contains('=') {
        Some(trimmed.to_string())
    } else if !token.is_empty() {
        Some(format!("bigmodel_token_production={token}"))
    } else {
        None
    }
}

async fn request_balance(
    client: &Client,
    url: &str,
    token: &str,
    cookie: Option<&str>,
) -> Result<Value, RefreshError> {
    let mut last_transport = None;
    for attempt in 0..2 {
        let mut request = client
            .get(url)
            .header("Authorization", token)
            .header("Accept", "application/json")
            .header("User-Agent", WEB_UA)
            .header("Origin", "https://open.bigmodel.cn")
            .header("Referer", REFERER)
            .timeout(Duration::from_secs(15));
        if let Some(cookie) = cookie {
            request = request.header("Cookie", cookie);
        }
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
                    "session_expired",
                    "GLM 网页登录已过期，请重新登录",
                    true,
                    false,
                ));
            }
            StatusCode::TOO_MANY_REQUESTS => {
                return Err(RefreshError::new(
                    "rate_limited",
                    "GLM 余额请求过于频繁，请稍后重试",
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
                    format!("GLM 余额查询失败（HTTP {}）", status.as_u16()),
                    false,
                    status.is_server_error(),
                ));
            }
            _ => {}
        }
        return response.json().await.map_err(|_| {
            RefreshError::new(
                "response_shape_changed",
                "GLM 余额返回格式发生变化",
                false,
                false,
            )
        });
    }
    Err(RefreshError::new(
        "network_error",
        last_transport
            .map(|error| format!("GLM 余额服务暂时无法连接：{error}"))
            .unwrap_or_else(|| "GLM 余额服务暂时无法连接".to_string()),
        false,
        true,
    ))
}

fn json_code(body: &Value) -> Option<i64> {
    let value = body.get("code")?;
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|number| i64::try_from(number).ok()))
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
}

fn business_message(body: &Value) -> &str {
    body.get("msg")
        .or_else(|| body.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn glm_response_ok(body: &Value) -> bool {
    let code = json_code(body);
    if code == Some(1001) {
        return false;
    }
    let message = business_message(body);
    if message.contains("Authorization") || message.contains("未登录") || message.contains("过期")
    {
        return false;
    }
    if body.get("success").and_then(Value::as_bool) == Some(true) {
        return true;
    }
    if matches!(code, Some(0) | Some(200)) {
        return true;
    }
    message.contains("成功") && !message.contains("失败")
}

fn parse(body: &Value) -> Result<Vec<CapabilityData>, RefreshError> {
    let code = json_code(body);
    if code == Some(1001) {
        return Err(RefreshError::new(
            "session_expired",
            "GLM 网页登录已过期，请重新登录",
            true,
            false,
        ));
    }
    if !glm_response_ok(body) {
        let message = business_message(body);
        let message = if message.is_empty() {
            "平台返回业务错误"
        } else {
            message
        };
        if message.contains("Authorization") {
            return Err(RefreshError::new(
                "session_expired",
                "GLM 网页登录已过期，请重新登录",
                true,
                false,
            ));
        }
        return Err(RefreshError::new(
            "http_error",
            format!("GLM 余额接口错误：{message}"),
            false,
            false,
        ));
    }
    let available = pick_balance(body).ok_or_else(|| {
        RefreshError::new(
            "missing_balance",
            "GLM 未返回可解析的余额字段",
            false,
            false,
        )
    })?;
    let gift = pick_decimal(body, &["giveAmount", "giftAmount", "presentAmount"]);
    let recharge = pick_decimal(body, &["rechargeAmount", "cashAmount", "toppedUpBalance"]);
    let mut secondary_parts = Vec::new();
    if let Some(gift) = gift {
        secondary_parts.push(format!("赠送 {}", format_cny(gift)));
    }
    if let Some(recharge) = recharge {
        secondary_parts.push(format!("充值 {}", format_cny(recharge)));
    }
    Ok(vec![CapabilityData {
        capability_id: "balance".into(),
        display_name: "账户余额".into(),
        value_kind: "money".into(),
        primary_value: Some(format_cny(available)),
        secondary_value: if secondary_parts.is_empty() {
            Some("网页个人余额".into())
        } else {
            Some(secondary_parts.join(" · "))
        },
        progress: None,
        trend: vec![],
        window_seconds: None,
        reset_at: None,
    }])
}

fn pick_balance(body: &Value) -> Option<rust_decimal::Decimal> {
    const KEYS: &[&str] = &[
        "availableBalance",
        "available_balance",
        "currentBalance",
        "current_balance",
        "cashBalance",
        "cash_balance",
        "totalBalance",
        "accountBalance",
        "balanceAmount",
        "balance",
    ];
    fn walk(value: &Value, depth: usize) -> Option<rust_decimal::Decimal> {
        if depth > 4 {
            return None;
        }
        match value {
            Value::Object(map) => {
                for key in KEYS {
                    if let Some(amount) = map.get(*key).and_then(super::money::decimal_from_json) {
                        return Some(amount);
                    }
                }
                map.values().find_map(|nested| walk(nested, depth + 1))
            }
            Value::Array(items) => items.iter().find_map(|item| walk(item, depth + 1)),
            _ => None,
        }
    }
    walk(body, 0).or_else(|| pick_decimal(body, KEYS))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_token_from_cookie_or_raw_secret() {
        assert_eq!(
            authorization_token("Hm=1; bigmodel_token_production=abc.def.ghi; x=2").unwrap(),
            "abc.def.ghi"
        );
        assert_eq!(
            authorization_token("eyJhbGciOiJIUzI1NiJ9.payload.sig").unwrap(),
            "eyJhbGciOiJIUzI1NiJ9.payload.sig"
        );
        assert_eq!(
            authorization_token("a=1; access_token=eyJhbGciOiJIUzI1NiJ9.payload.sig").unwrap(),
            "eyJhbGciOiJIUzI1NiJ9.payload.sig"
        );
    }

    #[test]
    fn parses_deeper_nested_wallet() {
        let values = parse(&json!({
            "success": true,
            "code": 0,
            "data": { "profile": { "wallet": { "available_balance": "3.50" } } }
        }))
        .expect("deep glm balance");
        assert_eq!(values[0].primary_value.as_deref(), Some("¥3.50"));
    }

    #[test]
    fn parses_nested_balance_object() {
        let values = parse(&json!({
            "success": true,
            "code": 0,
            "data": { "account": { "currentBalance": "6.22", "giveAmount": 0 } }
        }))
        .expect("nested glm balance");
        assert_eq!(values[0].primary_value.as_deref(), Some("¥6.22"));
    }

    #[test]
    fn parses_financial_overview_fields() {
        let values = parse(&json!({
            "success": true,
            "code": 0,
            "data": {
                "availableBalance": "88.10",
                "giveAmount": 8.1,
                "rechargeAmount": 80
            }
        }))
        .expect("glm balance");
        assert_eq!(values[0].primary_value.as_deref(), Some("¥88.10"));
        assert_eq!(
            values[0].secondary_value.as_deref(),
            Some("赠送 ¥8.10 · 充值 ¥80.00")
        );
    }

    #[test]
    fn treats_code_1001_as_expired_session() {
        let error = parse(&json!({"success": false, "code": 1001, "msg": "未收到Authorization"}))
            .unwrap_err();
        assert_eq!(error.code, "session_expired");
        assert!(error.auth_required);
    }

    #[test]
    fn treats_code_200_operation_success_as_ok() {
        let values = parse(&json!({
            "success": true,
            "code": 200,
            "msg": "操作成功",
            "data": { "availableBalance": "16.80", "giveAmount": 1.2 }
        }))
        .expect("glm code 200");
        assert_eq!(values[0].primary_value.as_deref(), Some("¥16.80"));
    }

    #[test]
    fn treats_string_code_200_as_ok() {
        let values = parse(&json!({
            "code": "200",
            "msg": "操作成功",
            "data": { "currentBalance": "4.00" }
        }))
        .expect("glm string code");
        assert_eq!(values[0].primary_value.as_deref(), Some("¥4.00"));
    }

    #[test]
    fn rejects_missing_balance_instead_of_zero() {
        let error = parse(&json!({"success": true, "code": 0, "data": {"foo": 1}})).unwrap_err();
        assert_eq!(error.code, "missing_balance");
    }
}
