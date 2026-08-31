//! 硅基流动、StepFun、OpenRouter、Novita 官方余额 Source。
//!
//! 迁移来源：cc-switch `src-tauri/src/services/balance.rs`（提交 6243e20a）。
//! 只迁移官方 URL、鉴权头与响应字段路径；改造点：
//! - 金额按 JSON 原文转 Decimal（`providers/money.rs`），Novita 0.0001 USD 单位
//!   用 Decimal 除以 10000，OpenRouter 剩余用 Decimal 减法，禁止 f64 汇总
//! - 错误改为结构化 RefreshError（401/403 → credential_expired）
//! - 字段缺失返回 missing_balance 错误，不照搬 `unwrap_or(0.0)` 补零
//! - 每平台独立 Source，返回 Capability 快照而非 cc-switch 的 UsageResult

use super::money::{decimal_from_json, format_cny, format_usd, pick_decimal};
use crate::domain::refresh::{CapabilityData, RefreshError, SourceRefreshOutput};
use reqwest::{Client, StatusCode};
use rust_decimal::Decimal;
use serde_json::Value;
use std::time::Duration;

pub const SILICONFLOW_SOURCE_ID: &str = "siliconflow-balance-api";
pub const SILICONFLOW_INTL_SOURCE_ID: &str = "siliconflow-intl-balance-api";
pub const STEPFUN_SOURCE_ID: &str = "stepfun-balance-api";
pub const OPENROUTER_SOURCE_ID: &str = "openrouter-balance-api";
pub const NOVITA_SOURCE_ID: &str = "novita-balance-api";

/// SiliconFlow 开放平台业务码，20000 为成功。
const SILICONFLOW_OK_CODE: i64 = 20000;
/// Novita 余额字段单位是 0.0001 USD。
const NOVITA_UNIT_SCALE: u64 = 10_000;

pub fn source_id_for_platform(platform_id: &str) -> Option<&'static str> {
    match platform_id {
        "siliconflow" => Some(SILICONFLOW_SOURCE_ID),
        "siliconflow_intl" => Some(SILICONFLOW_INTL_SOURCE_ID),
        "stepfun" => Some(STEPFUN_SOURCE_ID),
        "openrouter" => Some(OPENROUTER_SOURCE_ID),
        "novita" => Some(NOVITA_SOURCE_ID),
        _ => None,
    }
}

pub fn is_balance_source(source_id: &str) -> bool {
    source_id_for_platform(source_id_to_platform(source_id)).is_some()
}

fn source_id_to_platform(source_id: &str) -> &str {
    match source_id {
        SILICONFLOW_SOURCE_ID => "siliconflow",
        SILICONFLOW_INTL_SOURCE_ID => "siliconflow_intl",
        STEPFUN_SOURCE_ID => "stepfun",
        OPENROUTER_SOURCE_ID => "openrouter",
        NOVITA_SOURCE_ID => "novita",
        _ => "",
    }
}

struct EndpointSpec {
    default_base: &'static str,
    path: &'static str,
    label: &'static str,
}

fn spec_for(source_id: &str) -> Option<EndpointSpec> {
    match source_id {
        SILICONFLOW_SOURCE_ID => Some(EndpointSpec {
            default_base: "https://api.siliconflow.cn",
            path: "v1/user/info",
            label: "硅基流动",
        }),
        SILICONFLOW_INTL_SOURCE_ID => Some(EndpointSpec {
            default_base: "https://api.siliconflow.com",
            path: "v1/user/info",
            label: "硅基流动国际",
        }),
        STEPFUN_SOURCE_ID => Some(EndpointSpec {
            default_base: "https://api.stepfun.com",
            path: "v1/accounts",
            label: "StepFun",
        }),
        OPENROUTER_SOURCE_ID => Some(EndpointSpec {
            default_base: "https://openrouter.ai",
            path: "api/v1/credits",
            label: "OpenRouter",
        }),
        NOVITA_SOURCE_ID => Some(EndpointSpec {
            default_base: "https://api.novita.ai",
            path: "v3/user/balance",
            label: "Novita",
        }),
        _ => None,
    }
}

pub async fn fetch(
    client: &Client,
    source_id: &str,
    api_key: &str,
    base_url: Option<&str>,
) -> SourceRefreshOutput {
    let Some(spec) = spec_for(source_id) else {
        return SourceRefreshOutput::failure(RefreshError::new(
            "unsupported_source",
            "当前版本尚未实现此数据来源",
            false,
            false,
        ));
    };
    match fetch_inner(client, source_id, &spec, api_key, base_url).await {
        Ok(capabilities) => SourceRefreshOutput::success(capabilities),
        Err(error) => SourceRefreshOutput::failure(error),
    }
}

/// 用户填写的 `api_base_url` 可覆盖默认 host，但路径必须仍是官方余额路径：
/// base 已带官方路径前缀（如 `/v1`、`/api/v1`）时只补剩余段。
fn endpoint_url(base_url: Option<&str>, spec: &EndpointSpec) -> String {
    let base = base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(spec.default_base)
        .trim_end_matches('/');
    let segments: Vec<&str> = spec.path.split('/').collect();
    for take in (1..=segments.len()).rev() {
        let prefix = format!("/{}", segments[..take].join("/"));
        if base.ends_with(&prefix) {
            let rest = segments[take..].join("/");
            return if rest.is_empty() {
                base.to_string()
            } else {
                format!("{base}/{rest}")
            };
        }
    }
    format!("{base}/{}", spec.path)
}

async fn fetch_inner(
    client: &Client,
    source_id: &str,
    spec: &EndpointSpec,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<Vec<CapabilityData>, RefreshError> {
    let endpoint = endpoint_url(base_url, spec);
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
                    format!("{} API Key 无效或已过期", spec.label),
                    true,
                    false,
                ));
            }
            StatusCode::TOO_MANY_REQUESTS => {
                return Err(RefreshError::new(
                    "rate_limited",
                    format!("{}余额请求过于频繁，请稍后重试", spec.label),
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
                    format!("{}余额查询失败（HTTP {}）", spec.label, status.as_u16()),
                    false,
                    status.is_server_error(),
                ));
            }
            _ => {}
        }
        let body: Value = response.json().await.map_err(|_| {
            RefreshError::new(
                "response_shape_changed",
                format!("{}余额返回格式发生变化", spec.label),
                false,
                false,
            )
        })?;
        return parse(source_id, spec, &body);
    }
    Err(RefreshError::new(
        "network_error",
        last_transport
            .map(|error| format!("{}余额服务暂时无法连接：{error}", spec.label))
            .unwrap_or_else(|| format!("{}余额服务暂时无法连接", spec.label)),
        false,
        true,
    ))
}

fn parse(
    source_id: &str,
    spec: &EndpointSpec,
    body: &Value,
) -> Result<Vec<CapabilityData>, RefreshError> {
    match source_id {
        SILICONFLOW_SOURCE_ID => parse_siliconflow(body, spec, false),
        SILICONFLOW_INTL_SOURCE_ID => parse_siliconflow(body, spec, true),
        STEPFUN_SOURCE_ID => parse_stepfun(body, spec),
        OPENROUTER_SOURCE_ID => parse_openrouter(body, spec),
        NOVITA_SOURCE_ID => parse_novita(body, spec),
        _ => Err(RefreshError::new(
            "unsupported_source",
            "当前版本尚未实现此数据来源",
            false,
            false,
        )),
    }
}

fn balance_capability(primary: String, secondary: String) -> Vec<CapabilityData> {
    vec![CapabilityData {
        capability_id: "balance".into(),
        display_name: "账户余额".into(),
        value_kind: "money".into(),
        primary_value: Some(primary),
        secondary_value: Some(secondary),
        progress: None,
        trend: vec![],
        window_seconds: None,
        reset_at: None,
    }]
}

fn business_error(label: &str, body: &Value) -> RefreshError {
    let message = body
        .get("message")
        .or_else(|| body.get("msg"))
        .and_then(Value::as_str)
        .unwrap_or("平台返回业务错误");
    RefreshError::new(
        "http_error",
        format!("{label}余额接口错误：{message}"),
        false,
        false,
    )
}

/// 响应：`{ code, message, data: { balance, chargeBalance, totalBalance } }`。
/// `balance` 是赠送余额，`chargeBalance` 是充值余额，单位随站点为 CNY / USD。
fn parse_siliconflow(
    body: &Value,
    spec: &EndpointSpec,
    usd: bool,
) -> Result<Vec<CapabilityData>, RefreshError> {
    if let Some(code) = body.get("code").and_then(Value::as_i64) {
        if code != SILICONFLOW_OK_CODE {
            return Err(business_error(spec.label, body));
        }
    }
    let total = pick_decimal(body, &["totalBalance", "total_balance"]).ok_or_else(|| {
        RefreshError::new(
            "missing_balance",
            format!("{}未返回可解析的余额字段", spec.label),
            false,
            false,
        )
    })?;
    let format = if usd { format_usd } else { format_cny };
    let mut secondary_parts = Vec::new();
    if let Some(charge) = pick_decimal(body, &["chargeBalance", "charge_balance"]) {
        secondary_parts.push(format!("充值 {}", format(charge)));
    }
    if let Some(gift) = pick_decimal(body, &["balance"]) {
        secondary_parts.push(format!("赠送 {}", format(gift)));
    }
    let secondary = if secondary_parts.is_empty() {
        format!("{}开放平台", spec.label)
    } else {
        secondary_parts.join(" · ")
    };
    Ok(balance_capability(format(total), secondary))
}

/// 响应：`{ balance, total_cash_balance, total_voucher_balance }`，单位 CNY。
fn parse_stepfun(body: &Value, spec: &EndpointSpec) -> Result<Vec<CapabilityData>, RefreshError> {
    let total = pick_decimal(body, &["balance"]).ok_or_else(|| {
        RefreshError::new(
            "missing_balance",
            format!("{}未返回可解析的余额字段", spec.label),
            false,
            false,
        )
    })?;
    let mut secondary_parts = Vec::new();
    if let Some(cash) = pick_decimal(body, &["total_cash_balance"]) {
        secondary_parts.push(format!("现金 {}", format_cny(cash)));
    }
    if let Some(voucher) = pick_decimal(body, &["total_voucher_balance"]) {
        secondary_parts.push(format!("代金券 {}", format_cny(voucher)));
    }
    let secondary = if secondary_parts.is_empty() {
        format!("{}开放平台", spec.label)
    } else {
        secondary_parts.join(" · ")
    };
    Ok(balance_capability(format_cny(total), secondary))
}

/// 响应：`{ data: { total_credits, total_usage } }`，剩余 = total_credits - total_usage（Decimal 减法）。
/// total_usage 是官方累计消费字段，额外输出 total_spend 能力供消费趋势使用。
fn parse_openrouter(
    body: &Value,
    spec: &EndpointSpec,
) -> Result<Vec<CapabilityData>, RefreshError> {
    let data = body.get("data").unwrap_or(body);
    let total_credits = data
        .get("total_credits")
        .and_then(decimal_from_json)
        .ok_or_else(|| {
            RefreshError::new(
                "missing_balance",
                format!("{}未返回可解析的 Credits 字段", spec.label),
                false,
                false,
            )
        })?;
    let total_usage = data
        .get("total_usage")
        .and_then(decimal_from_json)
        .ok_or_else(|| {
            RefreshError::new(
                "missing_balance",
                format!("{}未返回可解析的已用额度字段", spec.label),
                false,
                false,
            )
        })?;
    let remaining = total_credits - total_usage;
    Ok(vec![
        CapabilityData {
            capability_id: "balance".into(),
            display_name: "账户余额".into(),
            value_kind: "money".into(),
            primary_value: Some(format_usd(remaining)),
            secondary_value: Some(format!(
                "总额度 {} · 已用 {}",
                format_usd(total_credits),
                format_usd(total_usage)
            )),
            progress: None,
            trend: vec![],
            window_seconds: None,
            reset_at: None,
        },
        CapabilityData {
            capability_id: "total_spend".into(),
            display_name: "累计消费".into(),
            value_kind: "money".into(),
            primary_value: Some(format_usd(total_usage)),
            secondary_value: Some("OpenRouter 官方已用额度".into()),
            progress: None,
            trend: vec![],
            window_seconds: None,
            reset_at: None,
        },
    ])
}

/// 响应：`{ availableBalance, cashBalance }`，字段单位是 0.0001 USD，Decimal 除以 10000 展示。
fn parse_novita(body: &Value, spec: &EndpointSpec) -> Result<Vec<CapabilityData>, RefreshError> {
    let unit = Decimal::from(NOVITA_UNIT_SCALE);
    let available = body
        .get("availableBalance")
        .and_then(decimal_from_json)
        .ok_or_else(|| {
            RefreshError::new(
                "missing_balance",
                format!("{}未返回可解析的余额字段", spec.label),
                false,
                false,
            )
        })?
        / unit;
    let mut secondary_parts = Vec::new();
    if let Some(cash) = body.get("cashBalance").and_then(decimal_from_json) {
        secondary_parts.push(format!("现金 {}", format_usd(cash / unit)));
    }
    let secondary = if secondary_parts.is_empty() {
        format!("{}开放平台", spec.label)
    } else {
        secondary_parts.join(" · ")
    };
    Ok(balance_capability(format_usd(available), secondary))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn spec(source_id: &str) -> EndpointSpec {
        spec_for(source_id).expect("spec")
    }

    #[test]
    fn honors_base_override_but_keeps_official_path() {
        assert_eq!(
            endpoint_url(None, &spec(SILICONFLOW_SOURCE_ID)),
            "https://api.siliconflow.cn/v1/user/info"
        );
        assert_eq!(
            endpoint_url(
                Some("https://api.siliconflow.cn/v1/"),
                &spec(SILICONFLOW_SOURCE_ID)
            ),
            "https://api.siliconflow.cn/v1/user/info"
        );
        assert_eq!(
            endpoint_url(
                Some("https://mirror.example.com"),
                &spec(OPENROUTER_SOURCE_ID)
            ),
            "https://mirror.example.com/api/v1/credits"
        );
        assert_eq!(
            endpoint_url(
                Some("https://mirror.example.com/api/v1"),
                &spec(OPENROUTER_SOURCE_ID)
            ),
            "https://mirror.example.com/api/v1/credits"
        );
        assert_eq!(
            endpoint_url(None, &spec(NOVITA_SOURCE_ID)),
            "https://api.novita.ai/v3/user/balance"
        );
    }

    #[test]
    fn parses_siliconflow_total_balance_with_breakdown() {
        let values = parse_siliconflow(
            &json!({
                "code": 20000,
                "message": "OK",
                "data": { "balance": "2.00", "chargeBalance": "8.50", "totalBalance": "10.50" }
            }),
            &spec(SILICONFLOW_SOURCE_ID),
            false,
        )
        .expect("siliconflow balance");
        assert_eq!(values[0].primary_value.as_deref(), Some("¥10.50"));
        assert_eq!(
            values[0].secondary_value.as_deref(),
            Some("充值 ¥8.50 · 赠送 ¥2.00")
        );

        let usd = parse_siliconflow(
            &json!({ "code": 20000, "data": { "totalBalance": "12.34" } }),
            &spec(SILICONFLOW_INTL_SOURCE_ID),
            true,
        )
        .expect("siliconflow intl");
        assert_eq!(usd[0].primary_value.as_deref(), Some("$12.34"));
    }

    #[test]
    fn siliconflow_business_code_failure_is_error() {
        let error = parse_siliconflow(
            &json!({ "code": 30001, "message": "Api key is invalid" }),
            &spec(SILICONFLOW_SOURCE_ID),
            false,
        )
        .expect_err("business error");
        assert_eq!(error.code, "http_error");
        assert!(error.message.contains("Api key is invalid"));
    }

    #[test]
    fn missing_siliconflow_balance_is_not_zero() {
        let error = parse_siliconflow(
            &json!({ "code": 20000, "data": { "name": "someone" } }),
            &spec(SILICONFLOW_SOURCE_ID),
            false,
        )
        .expect_err("missing");
        assert_eq!(error.code, "missing_balance");
    }

    #[test]
    fn parses_stepfun_balance_with_breakdown() {
        let values = parse_stepfun(
            &json!({ "object": "account", "balance": 66.6, "total_cash_balance": "60.00", "total_voucher_balance": "6.60" }),
            &spec(STEPFUN_SOURCE_ID),
        )
        .expect("stepfun");
        assert_eq!(values[0].primary_value.as_deref(), Some("¥66.60"));
        assert_eq!(
            values[0].secondary_value.as_deref(),
            Some("现金 ¥60.00 · 代金券 ¥6.60")
        );
    }

    #[test]
    fn openrouter_remaining_uses_decimal_subtraction() {
        let values = parse_openrouter(
            &json!({ "data": { "total_credits": "10.30", "total_usage": 0.1 } }),
            &spec(OPENROUTER_SOURCE_ID),
        )
        .expect("openrouter");
        assert_eq!(values[0].primary_value.as_deref(), Some("$10.20"));
        assert_eq!(
            values[0].secondary_value.as_deref(),
            Some("总额度 $10.30 · 已用 $0.10")
        );
    }

    #[test]
    fn openrouter_missing_usage_is_error_not_zero() {
        let error = parse_openrouter(
            &json!({ "data": { "total_credits": "10.00" } }),
            &spec(OPENROUTER_SOURCE_ID),
        )
        .expect_err("missing usage");
        assert_eq!(error.code, "missing_balance");
    }

    #[test]
    fn novita_divides_unit_by_decimal() {
        let values = parse_novita(
            &json!({ "availableBalance": 100000, "cashBalance": "60000" }),
            &spec(NOVITA_SOURCE_ID),
        )
        .expect("novita");
        assert_eq!(values[0].primary_value.as_deref(), Some("$10.00"));
        assert_eq!(values[0].secondary_value.as_deref(), Some("现金 $6.00"));
    }

    #[test]
    fn platform_and_source_ids_map_both_ways() {
        for platform in [
            "siliconflow",
            "siliconflow_intl",
            "stepfun",
            "openrouter",
            "novita",
        ] {
            let source = source_id_for_platform(platform).expect("source id");
            assert!(is_balance_source(source));
        }
        assert!(!is_balance_source("kimi-balance-api"));
    }
}
