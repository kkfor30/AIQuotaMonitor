//! DeepSeek 官方余额 Source。
//!
//! 接口契约参考 DeepSeekMonitorWindows-final 的 `providers/deepseek.rs`，
//! 金额解析改为 Decimal，错误改为结构化 RefreshError。

use crate::domain::refresh::{CapabilityData, RefreshError, SourceRefreshOutput};
use reqwest::{Client, StatusCode};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;
use std::time::Duration;

const DEFAULT_BASE: &str = "https://api.deepseek.com";

#[derive(Debug, Deserialize)]
struct BalanceResponse {
    is_available: bool,
    balance_infos: Vec<BalanceInfo>,
}

#[derive(Debug, Deserialize)]
struct BalanceInfo {
    currency: String,
    topped_up_balance: String,
}

pub async fn fetch(client: &Client, api_key: &str, base_url: Option<&str>) -> SourceRefreshOutput {
    match fetch_inner(client, api_key, base_url).await {
        Ok(capabilities) => SourceRefreshOutput::success(capabilities),
        Err(error) => SourceRefreshOutput::failure(error),
    }
}

fn balance_url(base_url: Option<&str>) -> String {
    let base = base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_BASE)
        .trim_end_matches('/');
    if base.ends_with("/user/balance") {
        base.to_string()
    } else {
        format!("{base}/user/balance")
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
                    "DeepSeek API Key 无效或已过期",
                    true,
                    false,
                ));
            }
            StatusCode::TOO_MANY_REQUESTS => {
                return Err(RefreshError::new(
                    "rate_limited",
                    "DeepSeek 余额请求过于频繁，请稍后重试",
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
                    format!("DeepSeek 余额查询失败（HTTP {}）", status.as_u16()),
                    false,
                    status.is_server_error(),
                ));
            }
            _ => {}
        }
        let body = response.json::<BalanceResponse>().await.map_err(|_| {
            RefreshError::new(
                "response_shape_changed",
                "DeepSeek 余额返回格式发生变化",
                false,
                false,
            )
        })?;
        return parse(body);
    }
    Err(RefreshError::new(
        "network_error",
        last_transport
            .map(|error| format!("DeepSeek 余额服务暂时无法连接：{error}"))
            .unwrap_or_else(|| "DeepSeek 余额服务暂时无法连接".to_string()),
        false,
        true,
    ))
}

fn parse(body: BalanceResponse) -> Result<Vec<CapabilityData>, RefreshError> {
    let info = body
        .balance_infos
        .iter()
        .find(|item| item.currency.eq_ignore_ascii_case("CNY"))
        .or_else(|| body.balance_infos.first())
        .ok_or_else(|| {
            RefreshError::new("missing_balance", "DeepSeek 未返回余额明细", false, false)
        })?;
    // 官网口径：账号余额即充值余额；累计消费由网页用量 Source 的 total_spend 提供。
    let topped_up = parse_decimal(&info.topped_up_balance, "充值余额")?;
    let symbol = currency_symbol(&info.currency);
    Ok(vec![CapabilityData {
        capability_id: "balance".into(),
        display_name: "充值余额".into(),
        value_kind: "money".into(),
        primary_value: Some(format!("{symbol}{topped_up:.2}")),
        secondary_value: if body.is_available {
            None
        } else {
            Some("当前不可用".to_string())
        },
        progress: None,
        trend: vec![],
        window_seconds: None,
        reset_at: None,
    }])
}

fn parse_decimal(value: &str, field: &str) -> Result<Decimal, RefreshError> {
    Decimal::from_str(value).map_err(|_| {
        RefreshError::new(
            "invalid_decimal",
            format!("DeepSeek {field}不是有效定点数"),
            false,
            false,
        )
    })
}

fn currency_symbol(currency: &str) -> String {
    match currency.to_ascii_uppercase().as_str() {
        "CNY" => "¥".into(),
        "USD" => "$".into(),
        value => format!("{value} "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_balance_without_float_math() {
        let values = parse(BalanceResponse {
            is_available: true,
            balance_infos: vec![BalanceInfo {
                currency: "CNY".into(),
                topped_up_balance: "10.345".into(),
            }],
        })
        .expect("balance should parse");
        // 官网口径：主值 = 充值余额；Decimal 原文解析后按 half-even 显示两位。
        assert_eq!(values[0].primary_value.as_deref(), Some("¥10.34"));
        assert_eq!(values[0].display_name, "充值余额");
        assert!(values[0].secondary_value.is_none());
    }
}
