//! DeepSeek 网页用量 Source。内部 amount/cost 请求独立解析，允许能力级部分成功。

use crate::domain::refresh::{CapabilityData, RefreshError, SourceRefreshOutput, StoredTrendPoint};
use chrono::{Datelike, Local};
use reqwest::{Client, StatusCode};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct Entry {
    #[serde(rename = "type")]
    kind: String,
    amount: String,
}

#[derive(Debug, Deserialize)]
struct ModelUsage {
    model: String,
    usage: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
struct DayUsage {
    date: String,
    data: Vec<ModelUsage>,
}

#[derive(Debug, Deserialize)]
struct AmountBiz {
    #[serde(default)]
    total: Vec<ModelUsage>,
    #[serde(default)]
    days: Vec<DayUsage>,
}

#[derive(Debug, Deserialize)]
struct AmountResp {
    data: AmountData,
}

#[derive(Debug, Deserialize)]
struct AmountData {
    biz_data: AmountBiz,
}

#[derive(Debug, Deserialize)]
struct CostResp {
    data: CostData,
}

#[derive(Debug, Deserialize)]
struct CostData {
    biz_data: Vec<CostBiz>,
}

#[derive(Debug, Deserialize)]
struct CostBiz {
    total: Vec<ModelUsage>,
    days: Vec<DayUsage>,
}

#[derive(Default, Debug, Clone, Copy)]
struct TokenBreakdown {
    total: u64,
    requests: u64,
    hit: u64,
    miss: u64,
    response: u64,
}

pub async fn fetch_current_month(client: &Client, token: &str) -> SourceRefreshOutput {
    let now = Local::now();
    let month = now.month();
    let year = now.year();
    let amount_url = format!("https://platform.deepseek.com/api/v0/usage/amount?month={month}&year={year}");
    let amount = match get_json::<AmountResp>(client, &amount_url, token).await {
        Ok(amount) => amount,
        Err(error) => return SourceRefreshOutput::failure(error),
    };
    let mut capabilities = match amount_capabilities(&amount) {
        Ok(values) => values,
        Err(error) => return SourceRefreshOutput::failure(error),
    };

    let cost_url = format!("https://platform.deepseek.com/api/v0/usage/cost?month={month}&year={year}");
    match get_json::<CostResp>(client, &cost_url, token).await {
        Ok(cost) => match cost_capabilities(&cost, &now.format("%Y-%m-%d").to_string()) {
            Ok(values) => {
                capabilities.extend(values);
                SourceRefreshOutput::success(capabilities)
            }
            Err(error) => SourceRefreshOutput {
                capabilities,
                error: Some(error),
            },
        },
        Err(error) => SourceRefreshOutput {
            capabilities,
            error: Some(error),
        },
    }
}

async fn get_json<T: DeserializeOwned>(client: &Client, url: &str, token: &str) -> Result<T, RefreshError> {
    for attempt in 0..2 {
        let response = client
            .get(url)
            .bearer_auth(token.trim())
            .header("x-app-version", "1.0.0")
            .header("Accept", "*/*")
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36",
            )
            .timeout(Duration::from_secs(15))
            .send()
            .await;
        let response = match response {
            Ok(response) => response,
            Err(error) if attempt == 0 => {
                let _ = error;
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
            Err(error) => {
                return Err(RefreshError::new(
                    "network_error",
                    format!("DeepSeek 网页用量服务暂时无法连接：{error}"),
                    false,
                    true,
                ));
            }
        };
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(RefreshError::new(
                    "session_expired",
                    "DeepSeek 网页会话已过期，请重新登录",
                    true,
                    false,
                ));
            }
            StatusCode::TOO_MANY_REQUESTS => {
                return Err(RefreshError::new(
                    "rate_limited",
                    "DeepSeek 网页用量请求过于频繁，请稍后重试",
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
                    format!("DeepSeek 网页用量查询失败（HTTP {}）", status.as_u16()),
                    false,
                    status.is_server_error(),
                ));
            }
            _ => {}
        }
        return response.json::<T>().await.map_err(|_| {
            RefreshError::new(
                "response_shape_changed",
                "DeepSeek 网页用量返回格式发生变化",
                false,
                false,
            )
        });
    }
    unreachable!()
}

fn amount_capabilities(amount: &AmountResp) -> Result<Vec<CapabilityData>, RefreshError> {
    let mut per_model = HashMap::<String, TokenBreakdown>::new();
    let mut all = TokenBreakdown::default();
    let models = if amount.data.biz_data.total.is_empty() {
        amount
            .data
            .biz_data
            .days
            .iter()
            .flat_map(|day| day.data.iter())
            .collect::<Vec<_>>()
    } else {
        amount.data.biz_data.total.iter().collect::<Vec<_>>()
    };
    for model in models {
        let values = token_breakdown(&model.usage)?;
        all.total = all.total.saturating_add(values.total);
        all.requests = all.requests.saturating_add(values.requests);
        all.hit = all.hit.saturating_add(values.hit);
        all.miss = all.miss.saturating_add(values.miss);
        all.response = all.response.saturating_add(values.response);
        let entry = per_model.entry(model.model.clone()).or_default();
        entry.total = entry.total.saturating_add(values.total);
        entry.requests = entry.requests.saturating_add(values.requests);
        entry.hit = entry.hit.saturating_add(values.hit);
        entry.miss = entry.miss.saturating_add(values.miss);
        entry.response = entry.response.saturating_add(values.response);
    }
    let flash = per_model.get("deepseek-v4-flash").copied().unwrap_or_default();
    let pro = per_model.get("deepseek-v4-pro").copied().unwrap_or_default();
    let cache_total = all.hit.saturating_add(all.miss);
    let cache_ratio = if cache_total == 0 { None } else { Some(all.hit as f64 / cache_total as f64) };
    Ok(vec![
        tokens_capability("model_usage_v4_flash", "V4 Flash 用量", flash.total),
        tokens_capability("model_usage_v4_pro", "V4 Pro 用量", pro.total),
        tokens_capability("request_count", "请求数", all.requests),
        tokens_capability("response_tokens", "输出 Token", all.response),
        CapabilityData {
            capability_id: "cache_hit_rate".into(),
            display_name: "缓存命中率".into(),
            value_kind: "percent".into(),
            primary_value: cache_ratio.map(|ratio| format!("{:.1}%", ratio * 100.0)),
            secondary_value: Some(format!("命中 {} · 未命中 {}", format_count(all.hit), format_count(all.miss))),
            progress: cache_ratio,
            trend: vec![],
        },
    ])
}

fn cost_capabilities(cost: &CostResp, today: &str) -> Result<Vec<CapabilityData>, RefreshError> {
    let Some(period) = cost.data.biz_data.first() else {
        return Err(RefreshError::new("missing_cost", "DeepSeek 未返回费用明细", false, false));
    };
    let month_cost = period
        .total
        .iter()
        .try_fold(Decimal::ZERO, |total, model| Ok::<_, RefreshError>(total + cost_sum(&model.usage)?))?;
    let mut today_cost = Decimal::ZERO;
    let mut trend = Vec::new();
    for day in &period.days {
        let day_cost = day
            .data
            .iter()
            .try_fold(Decimal::ZERO, |total, model| Ok::<_, RefreshError>(total + cost_sum(&model.usage)?))?;
        if day.date == today {
            today_cost = day_cost;
        }
        trend.push(StoredTrendPoint {
            label: day.date.get(5..).unwrap_or(&day.date).to_string(),
            value: day_cost.to_string(),
        });
    }
    if trend.len() > 7 {
        trend = trend.split_off(trend.len() - 7);
    }
    Ok(vec![
        money_capability("today_spend", "今日消费", today_cost),
        money_capability("month_spend", "本月消费", month_cost),
        CapabilityData {
            capability_id: "usage_trend".into(),
            display_name: "近 7 日消费趋势".into(),
            value_kind: "trend".into(),
            primary_value: None,
            secondary_value: Some("人民币定点金额".into()),
            progress: None,
            trend,
        },
    ])
}

fn token_breakdown(entries: &[Entry]) -> Result<TokenBreakdown, RefreshError> {
    let mut values = TokenBreakdown::default();
    let mut prompt = None;
    for entry in entries {
        let value = Decimal::from_str(&entry.amount)
            .ok()
            .and_then(|value| value.round().to_u64())
            .ok_or_else(|| RefreshError::new("invalid_amount", "DeepSeek Token 数量格式异常", false, false))?;
        match entry.kind.as_str() {
            "REQUEST" => values.requests = value,
            "PROMPT_CACHE_HIT_TOKEN" => values.hit = value,
            "PROMPT_CACHE_MISS_TOKEN" => values.miss = value,
            "RESPONSE_TOKEN" => values.response = value,
            "PROMPT_TOKEN" => prompt = Some(value),
            _ => {}
        }
    }
    values.total = prompt
        .unwrap_or_else(|| values.hit.saturating_add(values.miss))
        .saturating_add(values.response);
    Ok(values)
}

fn cost_sum(entries: &[Entry]) -> Result<Decimal, RefreshError> {
    entries
        .iter()
        .filter(|entry| entry.kind != "REQUEST")
        .try_fold(Decimal::ZERO, |total, entry| {
            Decimal::from_str(&entry.amount)
                .map(|value| total + value)
                .map_err(|_| RefreshError::new("invalid_decimal", "DeepSeek 费用格式异常", false, false))
        })
}

fn tokens_capability(id: &str, name: &str, value: u64) -> CapabilityData {
    CapabilityData {
        capability_id: id.into(),
        display_name: name.into(),
        value_kind: "tokens".into(),
        primary_value: Some(format_count(value)),
        secondary_value: None,
        progress: None,
        trend: vec![],
    }
}

fn money_capability(id: &str, name: &str, value: Decimal) -> CapabilityData {
    CapabilityData {
        capability_id: id.into(),
        display_name: name.into(),
        value_kind: "money".into(),
        primary_value: Some(format!("¥{value:.2}")),
        secondary_value: None,
        progress: None,
        trend: vec![],
    }
}

fn format_count(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.2}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}
