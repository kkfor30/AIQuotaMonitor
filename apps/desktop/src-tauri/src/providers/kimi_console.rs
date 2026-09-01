//! Kimi 控制台消费会话 Source。
//!
//! 官方余额 API（api.moonshot.cn）不含任何消费字段；今日消费/本月消费/总消费
//! 来自 platform.kimi.com 控制台内部接口（接口与字段逆向自控制台前端，2026-09）：
//! 1. GET /api?endpoint=refreshToken，header `Msh-Authorization: {rtoken}` 换 access_token；
//! 2. GET /api?endpoint=userInfo 取组织 oid（organizations[0].organization.id）；
//! 3. GET /api?endpoint=organizationAccountInfo&oid=… 读 today_consume（今日消费）、
//!    use（总消费）；cur/voucher_cur 是余额，归 kimi-balance-api，此处不产出；
//! 4. GET /api?endpoint=organizationMonthlyBills&oid=… 当月记录 recharge_fee+voucher_fee
//!    为本月消费（与控制台同口径）；当月无账单记录时控制台显示“-”，此处不产出能力。
//! 金额单位为 1e-5 元（控制台除以 100000 后保留 5 位小数），全链路 Decimal。
//! rtoken 存 Windows Credential Manager；若平台轮换 refresh_token 导致失效，需重新登录捕获。

use super::money::{decimal_from_json, format_cny, pick_decimal, WEB_UA};
use crate::domain::refresh::{CapabilityData, RefreshError, SourceRefreshOutput};
use chrono::{Datelike, FixedOffset, Utc};
use reqwest::{Client, StatusCode};
use rust_decimal::Decimal;
use serde_json::Value;
use std::time::Duration;

pub const CONSOLE_SOURCE_ID: &str = "kimi-console-session";
const CONSOLE_ORIGIN: &str = "https://platform.kimi.com";
const CN_TZ_SECS: i32 = 8 * 3600;

pub async fn fetch(client: &Client, secret: &str) -> SourceRefreshOutput {
    match fetch_inner(client, secret).await {
        Ok(capabilities) => SourceRefreshOutput::success(capabilities),
        Err(error) => SourceRefreshOutput::failure(error),
    }
}

async fn fetch_inner(
    client: &Client,
    secret: &str,
) -> Result<Vec<CapabilityData>, RefreshError> {
    let rtoken = secret.trim().trim_start_matches("Bearer ").trim();
    if rtoken.is_empty() {
        return Err(RefreshError::new(
            "auth_required",
            "Kimi 网页会话未配置",
            true,
            false,
        ));
    }
    let access_token = refresh_access_token(client, rtoken).await?;
    let oid = discover_organization_id(client, &access_token).await?;

    let mut capabilities = Vec::new();
    let mut last_error = None;
    match fetch_console_json(
        client,
        &access_token,
        &format!("/api?endpoint=organizationAccountInfo&oid={oid}"),
    )
    .await
    {
        Ok(body) => {
            // 今日消费为 0 也是真实值，正常产出；仅缺字段时不补零。
            if let Some(today) = scaled_amount(&body, &["today_consume"]) {
                capabilities.push(money_capability("today_spend", "今日消费", today));
            }
            if let Some(total) = scaled_amount(&body, &["use"]) {
                capabilities.push(money_capability("total_spend", "总消费", total));
            }
        }
        Err(error) => last_error = Some(error),
    }
    match fetch_console_json(
        client,
        &access_token,
        &format!("/api?endpoint=organizationMonthlyBills&oid={oid}"),
    )
    .await
    {
        Ok(body) => {
            // 当月账单记录存在才产出本月消费（recharge_fee + voucher_fee，控制台同口径）；
            // 无记录时控制台显示“-”，此处同样缺省，不用 0 冒充。
            if let Some(month) = current_month_amount(&body) {
                capabilities.push(money_capability("month_spend", "本月消费", month));
            }
        }
        Err(error) if last_error.is_none() => last_error = Some(error),
        Err(_) => {}
    }

    if capabilities.is_empty() {
        return Err(last_error.unwrap_or_else(|| {
            RefreshError::new(
                "response_shape_changed",
                "Kimi 控制台未返回可解析的消费字段",
                false,
                false,
            )
        }));
    }
    // 单接口失败保留另一接口的成功数据（能力级部分成功）。
    Ok(capabilities)
}

/// 用 rtoken 换短期 access_token；401/业务 401 视为会话过期。
async fn refresh_access_token(client: &Client, rtoken: &str) -> Result<String, RefreshError> {
    let response = client
        .get(format!("{CONSOLE_ORIGIN}/api?endpoint=refreshToken"))
        .header("Msh-Authorization", rtoken)
        .header("Accept", "application/json")
        .header("User-Agent", WEB_UA)
        .header("Origin", CONSOLE_ORIGIN)
        .header("Referer", format!("{CONSOLE_ORIGIN}/console/account"))
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|error| {
            RefreshError::new(
                "network_error",
                format!("Kimi 控制台暂时无法连接：{error}"),
                false,
                true,
            )
        })?;
    let status = response.status();
    let body: Value = response.json().await.map_err(|_| {
        RefreshError::new(
            "response_shape_changed",
            "Kimi 控制台登录态返回格式发生变化",
            false,
            false,
        )
    })?;
    if auth_rejected(&status, &body) {
        return Err(RefreshError::new(
            "credential_expired",
            "Kimi 网页会话已过期，请重新完成控制台登录",
            true,
            false,
        ));
    }
    ensure_ok(&body)?;
    let token = body
        .pointer("/data/access_token")
        .or_else(|| body.get("access_token"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            RefreshError::new(
                "response_shape_changed",
                "Kimi 控制台未返回 access_token",
                false,
                false,
            )
        })?;
    Ok(token.to_string())
}

/// userInfo.organizations[].organization.id 取第一个组织（控制台默认组织）。
async fn discover_organization_id(
    client: &Client,
    access_token: &str,
) -> Result<String, RefreshError> {
    let body = fetch_console_json(client, access_token, "/api?endpoint=userInfo").await?;
    let data = body.get("data").unwrap_or(&body);
    let organizations = data.get("organizations").and_then(Value::as_array).ok_or_else(
        || {
            RefreshError::new(
                "response_shape_changed",
                "Kimi 控制台未返回组织信息",
                false,
                false,
            )
        },
    )?;
    for organization in organizations {
        let id = organization.pointer("/organization/id");
        if let Some(id) = json_id_string(id) {
            return Ok(id);
        }
    }
    Err(RefreshError::new(
        "response_shape_changed",
        "Kimi 控制台组织列表为空，无法查询消费",
        false,
        false,
    ))
}

async fn fetch_console_json(
    client: &Client,
    access_token: &str,
    path: &str,
) -> Result<Value, RefreshError> {
    let response = console_get(client, path, Some(access_token)).await?;
    let status = response.status();
    let body: Value = response.json().await.map_err(|_| {
        RefreshError::new(
            "response_shape_changed",
            "Kimi 控制台返回格式发生变化",
            false,
            false,
        )
    })?;
    if auth_rejected(&status, &body) {
        return Err(RefreshError::new(
            "credential_expired",
            "Kimi 网页会话已过期，请重新完成控制台登录",
            true,
            false,
        ));
    }
    ensure_ok(&body)?;
    Ok(body)
}

async fn console_get(
    client: &Client,
    path: &str,
    access_token: Option<&str>,
) -> Result<reqwest::Response, RefreshError> {
    let mut request = client
        .get(format!("{CONSOLE_ORIGIN}{path}"))
        .header("Accept", "application/json")
        .header("User-Agent", WEB_UA)
        .header("Origin", CONSOLE_ORIGIN)
        .header("Referer", format!("{CONSOLE_ORIGIN}/console/account"))
        .timeout(Duration::from_secs(15));
    if let Some(access_token) = access_token {
        request = request.bearer_auth(access_token);
    }
    request.send().await.map_err(|error| {
        RefreshError::new(
            "network_error",
            format!("Kimi 控制台暂时无法连接：{error}"),
            false,
            true,
        )
    })
}

/// HTTP 401/403 或业务 code 401 都视为会话失效。
fn auth_rejected(status: &StatusCode, body: &Value) -> bool {
    *status == StatusCode::UNAUTHORIZED
        || *status == StatusCode::FORBIDDEN
        || body.get("code").and_then(Value::as_i64) == Some(401)
}

/// 控制台业务码：0 成功，非 0 带message。
fn ensure_ok(body: &Value) -> Result<(), RefreshError> {
    if let Some(code) = body.get("code").and_then(Value::as_i64) {
        if code != 0 {
            let message = body
                .get("message")
                .or_else(|| body.get("msg"))
                .and_then(Value::as_str)
                .unwrap_or("平台返回业务错误");
            return Err(RefreshError::new(
                "http_error",
                format!("Kimi 控制台接口错误：{message}"),
                false,
                false,
            ));
        }
    }
    Ok(())
}

/// 控制台金额以 1e-5 元为单位的整数/字符串存储，除以 100000 得元。
fn scaled_amount(body: &Value, keys: &[&str]) -> Option<Decimal> {
    pick_decimal(body, keys).map(|value| value / Decimal::from(100_000))
}

/// 月账单数组中找当月记录，消费 = recharge_fee + voucher_fee（与控制台一致，缺省为 0）。
fn current_month_amount(body: &Value) -> Option<Decimal> {
    let data = body.get("data").unwrap_or(body);
    let records = data
        .as_array()
        .or_else(|| data.get("records").and_then(Value::as_array))?;
    let tz = FixedOffset::east_opt(CN_TZ_SECS)?;
    let now = Utc::now().with_timezone(&tz);
    let month_prefix = format!("{:04}-{:02}", now.year(), now.month());
    for record in records {
        let Some(date) = record.get("date").and_then(Value::as_str) else {
            continue;
        };
        if !date.starts_with(&month_prefix) {
            continue;
        }
        let recharge = record
            .get("recharge_fee")
            .and_then(decimal_from_json)
            .unwrap_or(Decimal::ZERO);
        let voucher = record
            .get("voucher_fee")
            .and_then(decimal_from_json)
            .unwrap_or(Decimal::ZERO);
        return Some((recharge + voucher) / Decimal::from(100_000));
    }
    None
}

fn json_id_string(value: Option<&Value>) -> Option<String> {
    let text = match value? {
        Value::String(text) => text.trim().to_string(),
        Value::Number(number) => number.to_string(),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

fn money_capability(id: &str, name: &str, amount: Decimal) -> CapabilityData {
    CapabilityData {
        capability_id: id.into(),
        display_name: name.into(),
        value_kind: "money".into(),
        primary_value: Some(format_cny(amount)),
        secondary_value: None,
        progress: None,
        trend: vec![],
        window_seconds: None,
        reset_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scales_console_amounts_to_yuan() {
        let body = json!({
            "code": 0,
            "data": { "today_consume": 3000000, "use": "12345" }
        });
        let today = scaled_amount(&body, &["today_consume"]).expect("today");
        assert_eq!(format_cny(today), "¥30.00");
        let total = scaled_amount(&body, &["use"]).expect("total");
        assert_eq!(format_cny(total), "¥0.12");
    }

    #[test]
    fn month_amount_sums_current_month_record_only() {
        // 用当前系统月份动态构造记录，避免测试随月份漂移
        let tz = FixedOffset::east_opt(CN_TZ_SECS).expect("tz");
        let now = Utc::now().with_timezone(&tz);
        let current = format!("{:04}-{:02}", now.year(), now.month());
        let previous = if now.month() == 1 {
            format!("{:04}-12", now.year() - 1)
        } else {
            format!("{:04}-{:02}", now.year(), now.month() - 1)
        };
        let stacked = json!({
            "code": 0,
            "data": [
                { "date": previous, "recharge_fee": 100000, "voucher_fee": 0 },
                { "date": current, "recharge_fee": 250000, "voucher_fee": 50000 }
            ]
        });
        let month = current_month_amount(&stacked).expect("month");
        assert_eq!(format_cny(month), "¥3.00");
        // 仅上月记录：不产出本月消费
        let only_previous = json!({
            "code": 0,
            "data": [ { "date": previous, "recharge_fee": 100000, "voucher_fee": 0 } ]
        });
        assert!(current_month_amount(&only_previous).is_none());
    }

    #[test]
    fn extracts_access_token_from_envelope() {
        let body = json!({
            "code": 0,
            "data": { "refresh_token": "r", "access_token": "a".repeat(24) }
        });
        let token = body
            .pointer("/data/access_token")
            .and_then(Value::as_str)
            .expect("token");
        assert_eq!(token.len(), 24);
    }

    #[test]
    fn reads_organization_id_from_user_info() {
        let body = json!({
            "code": 0,
            "data": {
                "organizations": [
                    { "organization": { "id": "org-123", "name": "个人组织" } }
                ]
            }
        });
        let data = body.get("data").unwrap_or(&body);
        let organizations = data.get("organizations").and_then(Value::as_array).expect("orgs");
        let id = json_id_string(organizations[0].pointer("/organization/id"));
        assert_eq!(id.as_deref(), Some("org-123"));
    }
}
