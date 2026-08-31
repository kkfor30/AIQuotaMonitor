//! MiMo 网页会话余额。没有官方 API Key 余额接口。
//!
//! GET https://platform.xiaomimimo.com/api/v1/balance，鉴权为浏览器 Cookie
//!（serviceToken / api-platform_serviceToken，常为 httpOnly）。
//! 金额按 JSON 原文转 Decimal。查询字段参考 DeepSeekMonitorWindows `providers/mimo.rs`。
//! 仅 HTTP 401/403 或响应体明确表示会话/登录无效时视为鉴权失败；普通非零业务码不标成过期。

use super::money::{cookie_named, format_cny, pick_decimal, WEB_UA};
use crate::domain::refresh::{CapabilityData, RefreshError, SourceRefreshOutput};
use reqwest::{header::SET_COOKIE, Client, StatusCode};
use serde_json::Value;
use std::time::Duration;

pub const SOURCE_ID: &str = "mimo-web-session";
const BALANCE_URL: &str = "https://platform.xiaomimimo.com/api/v1/balance";
const MIMO_HOST_SUFFIX: &str = "xiaomimimo.com";

pub struct MimoFetchResult {
    pub output: SourceRefreshOutput,
    /// 仅在余额查询成功后给出的同域 Cookie 合并结果，失败响应不得回写。
    pub updated_cookie: Option<String>,
}

pub async fn fetch_session(client: &Client, cookie: &str) -> MimoFetchResult {
    match fetch_inner(client, cookie).await {
        Ok((capabilities, updated_cookie)) => MimoFetchResult {
            output: SourceRefreshOutput::success(capabilities),
            updated_cookie,
        },
        Err(error) => MimoFetchResult {
            output: SourceRefreshOutput::failure(error),
            updated_cookie: None,
        },
    }
}

pub fn cookie_looks_logged_in(cookie: &str) -> bool {
    let trimmed = cookie.trim();
    trimmed.len() >= 40 && cookie_named(trimmed, "serviceToken")
}

pub fn needs_silent_restore(output: &SourceRefreshOutput) -> bool {
    output.error.as_ref().is_some_and(|error| {
        error.auth_required && matches!(error.code.as_str(), "session_expired" | "credential_expired")
    })
}

async fn fetch_inner(
    client: &Client,
    cookie: &str,
) -> Result<(Vec<CapabilityData>, Option<String>), RefreshError> {
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
        let status = response.status();
        let final_host = response
            .url()
            .host_str()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let set_cookies = response
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(RefreshError::new(
                    "session_expired",
                    "MiMo 登录已过期，请从来源行重新登录",
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
        let body_text = response.text().await.map_err(|_| {
            RefreshError::new(
                "response_shape_changed",
                "MiMo 余额返回格式发生变化",
                false,
                false,
            )
        })?;
        let body: Value = serde_json::from_str(&body_text).map_err(|_| {
            RefreshError::new(
                "response_shape_changed",
                "MiMo 余额返回格式发生变化",
                false,
                false,
            )
        })?;
        let capabilities = parse(&body)?;
        let updated_cookie = if final_host.ends_with(MIMO_HOST_SUFFIX) {
            merge_set_cookies(cookie, &set_cookies)
                .filter(|merged| merged != cookie && cookie_looks_logged_in(merged))
        } else {
            None
        };
        return Ok((capabilities, updated_cookie));
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

fn json_code(body: &Value) -> Option<i64> {
    let value = body.get("code")?;
    value
        .as_i64()
        .or_else(|| value.as_u64().map(|code| code as i64))
        .or_else(|| value.as_str().and_then(|code| code.trim().parse().ok()))
}

fn json_message(body: &Value) -> String {
    body.get("message")
        .or_else(|| body.get("msg"))
        .or_else(|| body.get("error"))
        .or_else(|| body.get("errorMessage"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn is_auth_code(code: i64) -> bool {
    matches!(code, 401 | 403)
}

fn is_explicit_auth_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    const MARKS: &[&str] = &[
        "unauthorized",
        "unauthenticated",
        "not login",
        "not logged",
        "please login",
        "login expired",
        "token expired",
        "token invalid",
        "invalid token",
        "session expired",
        "session invalid",
        "invalid session",
        "auth fail",
        "authentication failed",
        "未登录",
        "登录过期",
        "登录失效",
        "登录已过期",
        "登录已失效",
        "请重新登录",
        "重新登录",
        "token无效",
        "token 无效",
        "token过期",
        "token 过期",
        "会话过期",
        "会话失效",
        "会话无效",
        "鉴权失败",
        "凭证失效",
        "凭证无效",
        "身份验证失败",
    ];
    MARKS.iter().any(|mark| lower.contains(mark) || message.contains(mark))
}

fn parse(body: &Value) -> Result<Vec<CapabilityData>, RefreshError> {
    let Some(code) = json_code(body) else {
        return Err(RefreshError::new(
            "response_shape_changed",
            "MiMo 余额返回缺少 code 字段",
            false,
            false,
        ));
    };
    if code != 0 {
        let message = json_message(body);
        if is_auth_code(code) || is_explicit_auth_message(&message) {
            let detail = if message.is_empty() {
                "登录已过期".to_string()
            } else {
                message
            };
            return Err(RefreshError::new(
                "session_expired",
                format!("MiMo 登录已过期：{detail}。请从来源行重新登录"),
                true,
                false,
            ));
        }
        let detail = if message.is_empty() {
            format!("业务错误 {code}")
        } else {
            message
        };
        return Err(RefreshError::new(
            "api_error",
            format!("MiMo 余额查询失败：{detail}"),
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

fn cookie_pairs(header: &str) -> Vec<(String, String)> {
    header
        .split(';')
        .filter_map(|part| {
            let (name, value) = part.trim().split_once('=')?;
            let name = name.trim();
            let value = value.trim();
            (!name.is_empty()).then(|| (name.to_string(), value.to_string()))
        })
        .collect()
}

fn set_cookie_is_foreign(header: &str) -> bool {
    header.split(';').skip(1).any(|part| {
        let (name, value) = match part.trim().split_once('=') {
            Some(pair) => pair,
            None => return false,
        };
        name.eq_ignore_ascii_case("domain")
            && !value.trim().trim_start_matches('.').to_ascii_lowercase().ends_with(MIMO_HOST_SUFFIX)
    })
}

fn set_cookie_deleted(header: &str) -> bool {
    let first = header.split(';').next().unwrap_or_default();
    let value = first.split_once('=').map(|(_, value)| value.trim()).unwrap_or("");
    if value.is_empty() {
        return true;
    }
    header.split(';').skip(1).any(|part| {
        let part = part.trim();
        let (name, value) = match part.split_once('=') {
            Some(pair) => pair,
            None => return part.eq_ignore_ascii_case("expired"),
        };
        (name.eq_ignore_ascii_case("max-age") && value.trim().trim_start_matches('-') == "0")
            || (name.eq_ignore_ascii_case("max-age") && value.trim().starts_with('-'))
    })
}

fn merge_set_cookies(existing: &str, set_cookies: &[String]) -> Option<String> {
    if set_cookies.is_empty() {
        return None;
    }
    let mut pairs = cookie_pairs(existing);
    let mut changed = false;
    for header in set_cookies {
        if set_cookie_is_foreign(header) {
            continue;
        }
        let Some(first) = header.split(';').next() else {
            continue;
        };
        let Some((name, value)) = first.split_once('=') else {
            continue;
        };
        let name = name.trim();
        let value = value.trim();
        if name.is_empty() {
            continue;
        }
        if set_cookie_deleted(header) {
            let before = pairs.len();
            pairs.retain(|(existing_name, _)| existing_name != name);
            changed |= pairs.len() != before;
            continue;
        }
        if let Some(existing) = pairs.iter_mut().find(|(existing_name, _)| existing_name == name) {
            if existing.1 != value {
                existing.1 = value.to_string();
                changed = true;
            }
        } else {
            pairs.push((name.to_string(), value.to_string()));
            changed = true;
        }
    }
    changed.then(|| {
        pairs
            .into_iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    })
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
    fn treats_explicit_auth_code_as_expired_session() {
        let error = parse(&json!({"code": 401, "message": "unauthorized"})).unwrap_err();
        assert_eq!(error.code, "session_expired");
        assert!(error.auth_required);

        let error = parse(&json!({"code": "403", "msg": "登录已过期"})).unwrap_err();
        assert_eq!(error.code, "session_expired");
        assert!(error.auth_required);
    }

    #[test]
    fn treats_business_error_as_api_error_not_auth() {
        let error = parse(&json!({"code": 50001, "message": "系统繁忙"})).unwrap_err();
        assert_eq!(error.code, "api_error");
        assert!(!error.auth_required);
        assert!(error.message.contains("系统繁忙"));
    }

    #[test]
    fn explicit_auth_message_wins_over_business_code() {
        let error = parse(&json!({"code": 1000, "message": "token expired"})).unwrap_err();
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

    #[test]
    fn merges_same_origin_set_cookie_and_skips_deleted_or_foreign() {
        let existing = "serviceToken=old-service-token-abcdefghijklmnopqrstuvwxyz; keep=1";
        let merged = merge_set_cookies(
            existing,
            &[
                "serviceToken=new-service-token-abcdefghijklmnopqrstuvwxyz; Path=/; HttpOnly".into(),
                "keep=1; Path=/".into(),
                "foreign=x; Domain=account.xiaomi.com".into(),
                "gone=1; Max-Age=0".into(),
            ],
        )
        .expect("merged");
        assert!(merged.contains("serviceToken=new-service-token-abcdefghijklmnopqrstuvwxyz"));
        assert!(merged.contains("keep=1"));
        assert!(!merged.contains("foreign="));
        assert!(!merged.contains("gone="));
        assert!(cookie_looks_logged_in(&merged));
    }

    #[test]
    fn silent_restore_only_for_auth_failures() {
        let expired = SourceRefreshOutput::failure(RefreshError::new(
            "session_expired",
            "expired",
            true,
            false,
        ));
        assert!(needs_silent_restore(&expired));
        let busy = SourceRefreshOutput::failure(RefreshError::new(
            "api_error",
            "busy",
            false,
            false,
        ));
        assert!(!needs_silent_restore(&busy));
        let missing = SourceRefreshOutput::failure(RefreshError::new(
            "auth_required",
            "未配置",
            true,
            false,
        ));
        assert!(!needs_silent_restore(&missing));
    }
}
