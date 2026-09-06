//! DeepSeek 网页用量 Source。
//!
//! 官网用量页已切到 `usage/by_api_key/{amount,cost}`（start/end/tz）和
//! `users/get_user_summary`（累计消费金额）。旧的 month/year 接口作为回退。
//! amount/cost/summary 独立解析，允许能力级部分成功；金额用 Decimal。
//!
//! 迁移来源：DeepSeekMonitorWindows-final `src-tauri/src/lib.rs` 的 `fetch_usage`
//! （提交 f3ab3ec6，MIT），改造为能力级快照与 RefreshError。

use crate::domain::refresh::{CapabilityData, RefreshError, SourceRefreshOutput, StoredTrendPoint};
use crate::providers::money::format_percent;
use chrono::{Datelike, FixedOffset, TimeZone, Utc};
use reqwest::{Client, StatusCode};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::BTreeMap;
use std::str::FromStr;
use std::time::Duration;

const PLATFORM_ORIGIN: &str = "https://platform.deepseek.com";
const AMOUNT_PATH: &str = "https://platform.deepseek.com/api/v0/usage/by_api_key/amount";
const COST_PATH: &str = "https://platform.deepseek.com/api/v0/usage/by_api_key/cost";
const LEGACY_AMOUNT: &str = "https://platform.deepseek.com/api/v0/usage/amount";
const LEGACY_COST: &str = "https://platform.deepseek.com/api/v0/usage/cost";
const SUMMARY_PATH: &str = "https://platform.deepseek.com/api/v0/users/get_user_summary";
const CNY_TZ_SECS: i32 = 8 * 3600;

#[derive(Default, Debug, Clone, Copy)]
struct TokenBreakdown {
    total: u64,
    requests: u64,
    hit: u64,
    miss: u64,
    response: u64,
    prompt: u64,
}

impl TokenBreakdown {
    fn saturating_add(self, other: Self) -> Self {
        Self {
            total: self.total.saturating_add(other.total),
            requests: self.requests.saturating_add(other.requests),
            hit: self.hit.saturating_add(other.hit),
            miss: self.miss.saturating_add(other.miss),
            response: self.response.saturating_add(other.response),
            prompt: self.prompt.saturating_add(other.prompt),
        }
    }
}

#[derive(Clone)]
struct MonthRange {
    year: i32,
    month: u32,
    start_sec: i64,
    end_sec: i64,
    today: String,
    tz: FixedOffset,
}

pub async fn fetch_current_month(client: &Client, token: &str) -> SourceRefreshOutput {
    let Some(range) = current_month_range() else {
        return SourceRefreshOutput::failure(RefreshError::new(
            "invalid_range",
            "无法计算 DeepSeek 用量查询时间窗",
            false,
            false,
        ));
    };

    let amount = match fetch_usage_json(client, token, true, &range).await {
        Ok(value) => value,
        Err(error) => return SourceRefreshOutput::failure(error),
    };
    let mut capabilities = match amount_capabilities(&amount, &range) {
        Ok(values) => values,
        Err(error) => return SourceRefreshOutput::failure(error),
    };

    // 近 7 日消费趋势：月初时当月窗口不足 7 天，按自然月补查上月窗口并合并
    // （接口按自然月查询最稳，跨月单窗口在官网返回过空 biz_data）；本月消费/今日消费
    // 在解析层按日期过滤，查询窗口不影响 today/month 语义。
    let mut daily_costs = match fetch_usage_json(client, token, false, &range).await {
        Ok(cost) => collect_daily_costs(&cost, range.tz),
        Err(error) => {
            return SourceRefreshOutput {
                capabilities,
                error: Some(error),
            };
        }
    };
    if let Some(prev_range) = previous_month_range(&range) {
        // 上月补查只增强跨月趋势；失败时趋势退化为当月天数，不阻塞其余能力。
        if let Ok(prev) = fetch_usage_json(client, token, false, &prev_range).await {
            daily_costs.extend(collect_daily_costs(&prev, prev_range.tz));
        }
    }
    capabilities.extend(cost_capabilities_from_daily(daily_costs, &range));

    if let Some(total) = fetch_total_spend(client, token).await {
        capabilities.push(money_capability("total_spend", "累计消费", total));
    }
    SourceRefreshOutput::success(capabilities)
}

fn current_month_range() -> Option<MonthRange> {
    let tz = FixedOffset::east_opt(CNY_TZ_SECS)?;
    let now = Utc::now().with_timezone(&tz);
    let start = tz
        .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()?;
    let end = if now.month() == 12 {
        tz.with_ymd_and_hms(now.year() + 1, 1, 1, 0, 0, 0)
            .single()?
    } else {
        tz.with_ymd_and_hms(now.year(), now.month() + 1, 1, 0, 0, 0)
            .single()?
    };
    Some(MonthRange {
        year: now.year(),
        month: now.month(),
        start_sec: start.timestamp(),
        end_sec: end.timestamp(),
        today: now.format("%Y-%m-%d").to_string(),
        tz,
    })
}

/// 月初「今天 -6 天」早于本月 1 日时返回上月查询窗口（上月 1 日 → 本月 1 日），
/// 用于合并出滚动 7 日消费趋势；近 7 天都在本月内时返回 None，不补查。
fn previous_month_range(range: &MonthRange) -> Option<MonthRange> {
    let now = Utc::now().with_timezone(&range.tz);
    let week_start = now - chrono::Duration::days(6);
    let week_start_sec = range
        .tz
        .with_ymd_and_hms(week_start.year(), week_start.month(), week_start.day(), 0, 0, 0)
        .single()
        .map(|instant| instant.timestamp())?;
    previous_month_window(range, week_start_sec)
}

fn previous_month_window(range: &MonthRange, week_start_sec: i64) -> Option<MonthRange> {
    if week_start_sec >= range.start_sec {
        return None;
    }
    let (prev_year, prev_month) = if range.month == 1 {
        (range.year - 1, 12)
    } else {
        (range.year, range.month - 1)
    };
    let start = range
        .tz
        .with_ymd_and_hms(prev_year, prev_month, 1, 0, 0, 0)
        .single()?;
    Some(MonthRange {
        year: prev_year,
        month: prev_month,
        start_sec: start.timestamp(),
        end_sec: range.start_sec,
        today: range.today.clone(),
        tz: range.tz,
    })
}

async fn fetch_usage_json(
    client: &Client,
    token: &str,
    amount: bool,
    range: &MonthRange,
) -> Result<Value, RefreshError> {
    let new_url = if amount {
        format!(
            "{AMOUNT_PATH}?start={}&end={}&tz={CNY_TZ_SECS}",
            range.start_sec, range.end_sec
        )
    } else {
        format!(
            "{COST_PATH}?start={}&end={}&tz={CNY_TZ_SECS}",
            range.start_sec, range.end_sec
        )
    };
    match get_json_value(client, &new_url, token).await {
        Ok(json) if has_biz_data(&json) => Ok(json),
        Err(error) if error.auth_required => Err(error),
        Ok(_) | Err(_) => {
            let legacy = if amount {
                format!("{LEGACY_AMOUNT}?month={}&year={}", range.month, range.year)
            } else {
                format!("{LEGACY_COST}?month={}&year={}", range.month, range.year)
            };
            get_json_value(client, &legacy, token).await
        }
    }
}

async fn fetch_total_spend(client: &Client, token: &str) -> Option<Decimal> {
    let json = get_json_value(client, SUMMARY_PATH, token).await.ok()?;
    parse_total_spend(&json)
}

async fn get_json_value(client: &Client, url: &str, token: &str) -> Result<Value, RefreshError> {
    for attempt in 0..2 {
        let response = client
            .get(url)
            .bearer_auth(token.trim())
            .header("x-app-version", "1.0.0")
            .header("Accept", "application/json, text/plain, */*")
            .header("Origin", PLATFORM_ORIGIN)
            .header("Referer", "https://platform.deepseek.com/usage")
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
        let body = response.text().await.map_err(|_| {
            RefreshError::new(
                "network_error",
                "DeepSeek 网页用量响应读取失败",
                false,
                true,
            )
        })?;
        let json: Value = serde_json::from_str(&body).map_err(|_| {
            RefreshError::new(
                "response_shape_changed",
                "DeepSeek 网页用量返回格式发生变化",
                false,
                false,
            )
        })?;
        if let Some(error) = business_error(&json) {
            return Err(error);
        }
        return Ok(json);
    }
    unreachable!()
}

fn business_error(json: &Value) -> Option<RefreshError> {
    let code = json
        .get("code")
        .or_else(|| json.get("status_code"))
        .or_else(|| json.get("status"));
    let ok = match code {
        None => true,
        Some(Value::Number(n)) => matches!(n.as_i64(), Some(0 | 200)),
        Some(Value::String(s)) => matches!(s.as_str(), "0" | "200" | "success" | "SUCCESS"),
        Some(Value::Bool(true)) => true,
        _ => false,
    };
    if ok {
        return None;
    }
    let message = json
        .get("message")
        .or_else(|| json.get("msg"))
        .or_else(|| json.get("error_msg"))
        .and_then(Value::as_str)
        .unwrap_or("业务接口返回失败");
    let numeric = code.and_then(Value::as_i64);
    let expired = matches!(numeric, Some(401 | 403 | 40002))
        || message.to_ascii_lowercase().contains("invalid token")
        || message.to_ascii_lowercase().contains("expired")
        || message.contains("401")
        || message.contains("403");
    if expired {
        return Some(RefreshError::new(
            "session_expired",
            "DeepSeek 网页会话已过期，请重新登录",
            true,
            false,
        ));
    }
    Some(RefreshError::new(
        "http_error",
        format!("DeepSeek 网页用量查询失败：{message}"),
        false,
        false,
    ))
}

fn amount_capabilities(
    amount: &Value,
    _range: &MonthRange,
) -> Result<Vec<CapabilityData>, RefreshError> {
    let mut per_model = BTreeMap::<String, TokenBreakdown>::new();
    let mut all = TokenBreakdown::default();
    for (model, usage) in collect_model_usages(amount) {
        let values = token_breakdown(&usage);
        all = all.saturating_add(values);
        let entry = per_model.entry(model).or_default();
        *entry = entry.saturating_add(values);
    }
    let flash = per_model
        .get("deepseek-v4-flash")
        .copied()
        .unwrap_or_default();
    let pro = per_model
        .get("deepseek-v4-pro")
        .copied()
        .unwrap_or_default();
    // V4 Flash Vision（实验版 deepseek-v4-flash-vision-exp 等）：按模型名前缀聚合，
    // 官方后续转正去掉 -exp 后缀也继续命中；没有官方单价，不推导消费。
    let flash_vision = per_model
        .iter()
        .filter(|(name, _)| name.starts_with("deepseek-v4-flash-vision"))
        .map(|(_, breakdown)| *breakdown)
        .fold(TokenBreakdown::default(), TokenBreakdown::saturating_add);
    let cache_total = all.hit.saturating_add(all.miss);
    let prompt = if all.prompt > 0 {
        all.prompt
    } else {
        cache_total
    };
    let cache_ratio = if cache_total == 0 {
        None
    } else {
        Some(all.hit as f64 / cache_total as f64)
    };
    let mut capabilities = vec![
        tokens_capability("model_usage_v4_flash", "V4 Flash 用量", flash.total),
        tokens_capability("model_usage_v4_pro", "V4 Pro 用量", pro.total),
        tokens_capability(
            "model_usage_v4_flash_vision",
            "V4 Flash Vision 用量",
            flash_vision.total,
        ),
        tokens_capability("request_count", "请求数", all.requests),
        tokens_capability("prompt_tokens", "输入 Token", prompt),
        tokens_capability("cache_hit_tokens", "输入（命中缓存）", all.hit),
        tokens_capability("cache_miss_tokens", "输入（未命中缓存）", all.miss),
        tokens_capability("response_tokens", "输出 Token", all.response),
        CapabilityData {
            capability_id: "cache_hit_rate".into(),
            display_name: "缓存命中率".into(),
            value_kind: "percent".into(),
            primary_value: cache_ratio.map(|ratio| format_percent(ratio * 100.0)),
            secondary_value: Some(format!(
                "命中 {} / 输入 {}",
                format_count(all.hit),
                format_count(cache_total)
            )),
            progress: cache_ratio,
            trend: vec![],
            window_seconds: None,
            reset_at: None,
        },
    ];
    // 按模型调用与缓存效率：每个模型的命中率 + 请求/输入/输出明细；
    // 从未调用的模型 primary/secondary 均为空，前端显示「未调用」，不补零。
    for (id, name, breakdown) in [
        (
            "model_usage_v4_flash_cache_hit_rate",
            "V4 Flash 缓存命中率",
            flash,
        ),
        (
            "model_usage_v4_flash_vision_cache_hit_rate",
            "V4 Flash Vision 缓存命中率",
            flash_vision,
        ),
        (
            "model_usage_v4_pro_cache_hit_rate",
            "V4 Pro 缓存命中率",
            pro,
        ),
    ] {
        capabilities.push(model_cache_rate_capability(id, name, breakdown));
    }
    Ok(capabilities)
}

/// 单模型命中率能力：命中率为主值与进度，请求/输入/输出明细进 secondary。
fn model_cache_rate_capability(id: &str, name: &str, breakdown: TokenBreakdown) -> CapabilityData {
    let cache_total = breakdown.hit.saturating_add(breakdown.miss);
    let ratio = if breakdown.requests == 0 || cache_total == 0 {
        None
    } else {
        Some(breakdown.hit as f64 / cache_total as f64)
    };
    let (primary_value, secondary_value) = if breakdown.requests == 0 {
        (None, None)
    } else {
        (
            ratio.map(|value| format_percent(value * 100.0)),
            Some(format!(
                "请求 {} · 输入 {} · 输出 {}",
                format_count(breakdown.requests),
                format_count(cache_total),
                format_count(breakdown.response)
            )),
        )
    };
    CapabilityData {
        capability_id: id.into(),
        display_name: name.into(),
        value_kind: "percent".into(),
        primary_value,
        secondary_value,
        progress: ratio,
        trend: vec![],
        window_seconds: None,
        reset_at: None,
    }
}

fn cost_capabilities(
    cost: &Value,
    range: &MonthRange,
) -> Result<Vec<CapabilityData>, RefreshError> {
    Ok(cost_capabilities_from_daily(
        collect_daily_costs(cost, range.tz),
        range,
    ))
}

/// 从「当月 + 可选上月补查」合并出的每日消费构建能力：
/// 本月消费只累加当月日期（旧接口无日期合计保持计入本月），
/// 趋势按日期序保留最近 7 天，可跨自然月，不插值、不补零。
fn cost_capabilities_from_daily(
    daily: Vec<(String, Decimal)>,
    range: &MonthRange,
) -> Vec<CapabilityData> {
    let mut by_day: BTreeMap<String, Decimal> = BTreeMap::new();
    let mut month_cost = Decimal::ZERO;
    let month_start_date = format!("{:04}-{:02}-01", range.year, range.month);
    for (date, amount) in daily {
        if date.is_empty() || date >= month_start_date {
            month_cost += amount;
        }
        if !date.is_empty() {
            *by_day.entry(date).or_insert(Decimal::ZERO) += amount;
        }
    }
    let today_cost = by_day.get(&range.today).copied().unwrap_or(Decimal::ZERO);
    let mut trend = by_day
        .into_iter()
        .map(|(label, value)| StoredTrendPoint {
            label: label.get(5..).unwrap_or(&label).to_string(),
            value: value.to_string(),
        })
        .collect::<Vec<_>>();
    if trend.len() > 7 {
        trend = trend.split_off(trend.len() - 7);
    }
    vec![
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
            window_seconds: None,
            reset_at: None,
        },
    ]
}

fn collect_model_usages(root: &Value) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    let Some(biz) = biz_data(root) else {
        return out;
    };
    if let Some(series) = biz.get("series").and_then(Value::as_array) {
        for item in series {
            let model = model_name(item);
            for bucket in as_array(item.get("buckets").or_else(|| item.get("data"))) {
                if let Some(usage) = first_value(bucket, &["usage", "usages"]) {
                    out.push((model.clone(), usage.clone()));
                } else {
                    out.push((model.clone(), bucket.clone()));
                }
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    for model in totals_list(biz) {
        let usage = first_value(model, &["usage", "usages"])
            .cloned()
            .unwrap_or_else(|| model.clone());
        out.push((model_name(model), usage));
    }
    if out.is_empty() {
        for day in days_list(biz) {
            for model in as_array(day.get("data").or_else(|| day.get("models"))) {
                let usage = first_value(model, &["usage", "usages"])
                    .cloned()
                    .unwrap_or_else(|| model.clone());
                out.push((model_name(model), usage));
            }
        }
    }
    out
}

fn collect_daily_costs(root: &Value, tz: FixedOffset) -> Vec<(String, Decimal)> {
    let mut out = Vec::new();
    let Some(biz) = biz_data(root) else {
        return out;
    };
    let blocks = cost_blocks(biz);
    for block in blocks {
        if let Some(currency) = first_value(block, &["currency", "currency_code", "currencyCode"])
            .and_then(Value::as_str)
        {
            if !currency.is_empty() && !currency.eq_ignore_ascii_case("CNY") {
                continue;
            }
        }
        if let Some(series) = block.get("series").and_then(Value::as_array) {
            for item in series {
                for bucket in as_array(item.get("buckets").or_else(|| item.get("data"))) {
                    let Some(date) = first_value(bucket, &["time", "date", "day", "timestamp"])
                        .and_then(|value| bucket_date(value, tz))
                    else {
                        continue;
                    };
                    let amount = bucket_cost(bucket);
                    if amount != Decimal::ZERO {
                        out.push((date, amount));
                    }
                }
            }
            continue;
        }
        for day in as_array(first_value(
            block,
            &["days", "daily", "daily_cost", "dailyCost"],
        )) {
            let Some(date) = first_value(day, &["date", "day"])
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                continue;
            };
            let mut amount = first_value(day, &["amount", "value", "cost", "total"])
                .and_then(json_decimal)
                .unwrap_or(Decimal::ZERO);
            if amount == Decimal::ZERO {
                for model in as_array(first_value(day, &["data", "models", "costs"])) {
                    amount += model_cost(model);
                }
            }
            out.push((date, amount));
        }
        if out.is_empty() {
            for model in as_array(first_value(block, &["total", "totals", "models"])) {
                out.push((String::new(), model_cost(model)));
            }
        }
    }
    out
}

fn cost_blocks(biz: &Value) -> Vec<&Value> {
    if let Some(list) = biz.get("data").and_then(Value::as_array) {
        return list.iter().collect();
    }
    if biz.as_array().is_some() {
        return as_array(Some(biz));
    }
    if biz.get("series").is_some() || biz.get("total").is_some() || biz.get("days").is_some() {
        return vec![biz];
    }
    vec![biz]
}

fn token_breakdown(usage: &Value) -> TokenBreakdown {
    let mut values = TokenBreakdown::default();
    let mut prompt = None;
    for (kind, amount) in usage_entries(usage) {
        let Some(value) = amount.round().to_u64() else {
            continue;
        };
        match kind.as_str() {
            "REQUEST" => values.requests = values.requests.saturating_add(value),
            "PROMPT_CACHE_HIT_TOKEN" => values.hit = values.hit.saturating_add(value),
            "PROMPT_CACHE_MISS_TOKEN" => values.miss = values.miss.saturating_add(value),
            "RESPONSE_TOKEN" | "COMPLETION_TOKEN" => {
                values.response = values.response.saturating_add(value)
            }
            "PROMPT_TOKEN" => {
                prompt = Some(prompt.unwrap_or(0u64).saturating_add(value));
                values.prompt = values.prompt.saturating_add(value);
            }
            _ => {}
        }
    }
    values.total = prompt
        .unwrap_or_else(|| values.hit.saturating_add(values.miss))
        .saturating_add(values.response);
    values
}

fn usage_entries(usage: &Value) -> Vec<(String, Decimal)> {
    if let Some(list) = usage.as_array() {
        return list
            .iter()
            .filter_map(|item| {
                let kind = first_value(item, &["type", "usage_type", "usageType", "name", "key"])?
                    .as_str()?
                    .to_string();
                let amount = first_value(item, &["amount", "value", "count", "total"])
                    .and_then(json_decimal)?;
                Some((kind, amount))
            })
            .collect();
    }
    let Some(object) = usage.as_object() else {
        return Vec::new();
    };
    if let Some(nested) = object.get("usage").or_else(|| object.get("usages")) {
        if nested.is_array() || nested.is_object() {
            return usage_entries(nested);
        }
    }
    object
        .iter()
        .filter_map(|(key, value)| json_decimal(value).map(|amount| (key.clone(), amount)))
        .collect()
}

fn model_cost(model: &Value) -> Decimal {
    let usage = first_value(model, &["usage", "usages"]).unwrap_or(model);
    let mut total = Decimal::ZERO;
    let mut has_entries = false;
    for (kind, amount) in usage_entries(usage) {
        if kind == "REQUEST" {
            continue;
        }
        has_entries = true;
        total += amount;
    }
    if has_entries {
        return total;
    }
    first_value(model, &["amount", "value", "cost"])
        .and_then(json_decimal)
        .unwrap_or(Decimal::ZERO)
}

fn bucket_cost(bucket: &Value) -> Decimal {
    let cost = first_value(bucket, &["cost", "amount", "value"]).unwrap_or(bucket);
    if let Some(amount) = json_decimal(cost) {
        return amount;
    }
    if cost.is_object() {
        if let Some(amount) =
            first_value(cost, &["amount", "value", "cost", "total"]).and_then(json_decimal)
        {
            return amount;
        }
        return usage_entries(cost)
            .into_iter()
            .filter(|(kind, _)| kind != "REQUEST")
            .fold(Decimal::ZERO, |total, (_, amount)| total + amount);
    }
    Decimal::ZERO
}

fn parse_total_spend(json: &Value) -> Option<Decimal> {
    let keys = [
        "total_usage",
        "totalUsage",
        "total_spend",
        "totalSpend",
        "accumulated_cost",
        "total_cost",
    ];
    for root in [biz_data(json), json.get("data"), Some(json)]
        .into_iter()
        .flatten()
    {
        for key in keys {
            if let Some(amount) = root.get(key).and_then(money_value) {
                return Some(amount);
            }
        }
    }
    None
}

fn money_value(value: &Value) -> Option<Decimal> {
    if let Some(amount) = json_decimal(value) {
        return Some(amount);
    }
    first_value(value, &["amount", "value", "balance", "cost", "total"]).and_then(json_decimal)
}

fn has_biz_data(json: &Value) -> bool {
    biz_data(json).is_some()
}

fn biz_data(json: &Value) -> Option<&Value> {
    let mut current = json;
    for _ in 0..6 {
        if let Some(biz) = current.get("biz_data").or_else(|| current.get("bizData")) {
            return Some(biz);
        }
        match current.get("data") {
            Some(next) if next.is_object() || next.is_array() => current = next,
            _ => break,
        }
    }
    None
}

fn totals_list(biz: &Value) -> Vec<&Value> {
    if let Some(list) = biz.get("total").and_then(Value::as_array) {
        return list.iter().collect();
    }
    if let Some(list) = biz.as_array() {
        let mut totals = Vec::new();
        for item in list {
            if let Some(inner) = item.get("total").and_then(Value::as_array) {
                totals.extend(inner.iter());
            }
        }
        return totals;
    }
    Vec::new()
}

fn days_list(biz: &Value) -> Vec<&Value> {
    if let Some(list) = biz.get("days").and_then(Value::as_array) {
        return list.iter().collect();
    }
    if let Some(list) = biz.as_array() {
        let mut days = Vec::new();
        for item in list {
            if let Some(inner) = item.get("days").and_then(Value::as_array) {
                days.extend(inner.iter());
            }
        }
        return days;
    }
    Vec::new()
}

fn bucket_date(value: &Value, tz: FixedOffset) -> Option<String> {
    if let Some(text) = value.as_str() {
        if text.len() >= 10 && text.as_bytes()[4] == b'-' {
            return Some(text[..10].to_string());
        }
        if let Ok(number) = text.parse::<i64>() {
            return timestamp_date(number, tz);
        }
        return None;
    }
    let number = value
        .as_i64()
        .or_else(|| value.as_u64().map(|n| n as i64))?;
    timestamp_date(number, tz)
}

fn timestamp_date(number: i64, tz: FixedOffset) -> Option<String> {
    let secs = if number > 1_000_000_000_000 {
        number / 1000
    } else {
        number
    };
    Utc.timestamp_opt(secs, 0)
        .single()
        .map(|instant| instant.with_timezone(&tz).format("%Y-%m-%d").to_string())
}

fn model_name(item: &Value) -> String {
    first_value(item, &["model", "model_name", "modelName", "name", "id"])
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string()
}

fn first_value<'a>(object: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    let map = object.as_object()?;
    for key in keys {
        if let Some(value) = map.get(*key) {
            return Some(value);
        }
    }
    None
}

fn as_array(value: Option<&Value>) -> Vec<&Value> {
    match value {
        Some(Value::Array(list)) => list.iter().collect(),
        Some(other) if other.is_object() => vec![other],
        _ => Vec::new(),
    }
}

fn json_decimal(value: &Value) -> Option<Decimal> {
    match value {
        Value::Number(number) => Decimal::from_str(&number.to_string()).ok(),
        Value::String(text) => Decimal::from_str(text.trim()).ok(),
        _ => None,
    }
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
        window_seconds: None,
        reset_at: None,
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
        window_seconds: None,
        reset_at: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn gmt8() -> FixedOffset {
        FixedOffset::east_opt(CNY_TZ_SECS).expect("offset")
    }

    #[test]
    fn parses_legacy_amount_object() {
        let json = serde_json::json!({
            "data": { "biz_data": {
                "total": [{
                    "model": "deepseek-v4-flash",
                    "usage": [
                        { "type": "REQUEST", "amount": "2" },
                        { "type": "PROMPT_CACHE_HIT_TOKEN", "amount": "100" },
                        { "type": "PROMPT_CACHE_MISS_TOKEN", "amount": "50" },
                        { "type": "RESPONSE_TOKEN", "amount": "20" }
                    ]
                }],
                "days": []
            }}
        });
        let range = MonthRange {
            year: 2026,
            month: 8,
            start_sec: 0,
            end_sec: 1,
            today: "2026-08-29".into(),
            tz: gmt8(),
        };
        let caps = amount_capabilities(&json, &range).expect("parse");
        assert_eq!(
            caps.iter()
                .find(|c| c.capability_id == "request_count")
                .unwrap()
                .primary_value
                .as_deref(),
            Some("2")
        );
        assert_eq!(
            caps.iter()
                .find(|c| c.capability_id == "cache_hit_tokens")
                .unwrap()
                .primary_value
                .as_deref(),
            Some("100")
        );
        assert_eq!(
            caps.iter()
                .find(|c| c.capability_id == "response_tokens")
                .unwrap()
                .primary_value
                .as_deref(),
            Some("20")
        );
    }

    #[test]
    fn parses_new_by_api_key_amount_and_numeric_fields() {
        let json = serde_json::json!({
            "data": { "biz_data": {
                "bucket": "1d",
                "series": [{
                    "model": "deepseek-v4-pro",
                    "buckets": [{
                        "time": 1756425600i64,
                        "usage": {
                            "REQUEST": 3,
                            "PROMPT_CACHE_HIT_TOKEN": 10,
                            "PROMPT_CACHE_MISS_TOKEN": 90,
                            "COMPLETION_TOKEN": 40
                        }
                    }]
                },
                {
                    "model": "deepseek-v4-flash-vision-exp",
                    "buckets": [{
                        "time": 1756425600i64,
                        "usage": {
                            "REQUEST": 1,
                            "PROMPT_CACHE_MISS_TOKEN": 210,
                            "COMPLETION_TOKEN": 30
                        }
                    }]
                }]
            }}
        });
        let range = MonthRange {
            year: 2026,
            month: 8,
            start_sec: 0,
            end_sec: 1,
            today: "2026-08-29".into(),
            tz: gmt8(),
        };
        let caps = amount_capabilities(&json, &range).expect("parse");
        assert_eq!(
            caps.iter()
                .find(|c| c.capability_id == "model_usage_v4_pro")
                .unwrap()
                .primary_value
                .as_deref(),
            Some("140")
        );
        // vision 按模型名前缀聚合：210 miss + 30 completion = 240
        assert_eq!(
            caps.iter()
                .find(|c| c.capability_id == "model_usage_v4_flash_vision")
                .unwrap()
                .primary_value
                .as_deref(),
            Some("240")
        );
        // 按模型命中率：pro 命中 10/输入 100 = 10%，vision 命中 0/输入 210 = 0%
        let pro_rate = caps
            .iter()
            .find(|c| c.capability_id == "model_usage_v4_pro_cache_hit_rate")
            .unwrap();
        assert_eq!(pro_rate.primary_value.as_deref(), Some("10%"));
        assert_eq!(
            pro_rate.secondary_value.as_deref(),
            Some("请求 3 · 输入 100 · 输出 40")
        );
        assert_eq!(pro_rate.progress, Some(0.1));
        let vision_rate = caps
            .iter()
            .find(|c| c.capability_id == "model_usage_v4_flash_vision_cache_hit_rate")
            .unwrap();
        assert_eq!(vision_rate.primary_value.as_deref(), Some("0%"));
        assert_eq!(
            vision_rate.secondary_value.as_deref(),
            Some("请求 1 · 输入 210 · 输出 30")
        );
        assert_eq!(
            caps.iter()
                .find(|c| c.capability_id == "request_count")
                .unwrap()
                .primary_value
                .as_deref(),
            Some("4")
        );
    }

    #[test]
    fn parses_new_cost_series_and_legacy_cost_array() {
        let tz = gmt8();
        let new_cost = serde_json::json!({
            "data": { "biz_data": { "data": [{
                "currency": "CNY",
                "series": [{
                    "model": "deepseek-v4-flash",
                    "buckets": [
                        { "time": "2026-08-28", "cost": "1.20" },
                        { "time": "2026-08-29", "cost": "2.30" }
                    ]
                }]
            }]}}
        });
        let range = MonthRange {
            year: 2026,
            month: 8,
            start_sec: 0,
            end_sec: 1,
            today: "2026-08-29".into(),
            tz,
        };
        let caps = cost_capabilities(&new_cost, &range).expect("new cost");
        assert_eq!(
            caps.iter()
                .find(|c| c.capability_id == "today_spend")
                .unwrap()
                .primary_value
                .as_deref(),
            Some("¥2.30")
        );
        assert_eq!(
            caps.iter()
                .find(|c| c.capability_id == "month_spend")
                .unwrap()
                .primary_value
                .as_deref(),
            Some("¥3.50")
        );

        let legacy = serde_json::json!({
            "data": { "biz_data": [{
                "total": [{ "model": "deepseek-v4-flash", "usage": [{ "type": "COST", "amount": "4.00" }] }],
                "days": [{
                    "date": "2026-08-29",
                    "data": [{ "model": "deepseek-v4-flash", "usage": [{ "type": "COST", "amount": "4.00" }] }]
                }]
            }]}
        });
        let caps = cost_capabilities(&legacy, &range).expect("legacy cost");
        assert_eq!(
            caps.iter()
                .find(|c| c.capability_id == "today_spend")
                .unwrap()
                .primary_value
                .as_deref(),
            Some("¥4.00")
        );
    }

    #[test]
    fn cost_covers_cross_month_week_but_month_spend_filters_prior_days() {
        let tz = gmt8();
        // 月初的滚动 7 日窗口会带回上月末的消费；只有当月日期计入本月消费。
        let json = serde_json::json!({
            "data": { "biz_data": { "data": [{
                "currency": "CNY",
                "series": [{
                    "model": "deepseek-v4-flash",
                    "buckets": [
                        { "time": "2026-08-28", "cost": "2.00" },
                        { "time": "2026-08-31", "cost": "5.00" },
                        { "time": "2026-09-01", "cost": "0.50" }
                    ]
                }]
            }]}}
        });
        let range = MonthRange {
            year: 2026,
            month: 9,
            start_sec: 0,
            end_sec: 1,
            today: "2026-09-01".into(),
            tz,
        };
        let caps = cost_capabilities(&json, &range).expect("cross month cost");
        assert_eq!(
            caps.iter()
                .find(|c| c.capability_id == "today_spend")
                .unwrap()
                .primary_value
                .as_deref(),
            Some("¥0.50")
        );
        assert_eq!(
            caps.iter()
                .find(|c| c.capability_id == "month_spend")
                .unwrap()
                .primary_value
                .as_deref(),
            Some("¥0.50")
        );
        let trend = caps
            .iter()
            .find(|c| c.capability_id == "usage_trend")
            .unwrap()
            .trend
            .clone();
        let labels: Vec<&str> = trend.iter().map(|point| point.label.as_str()).collect();
        assert_eq!(labels, vec!["08-28", "08-31", "09-01"]);
        let values: Vec<&str> = trend.iter().map(|point| point.value.as_str()).collect();
        assert_eq!(values, vec!["2.00", "5.00", "0.50"]);
    }

    #[test]
    fn previous_month_window_only_crosses_at_month_start() {
        // start_sec 取 2026-09-01 00:00 UTC（本地时区偏移不影响相对比较）
        let sep_start = 1_788_220_800i64;
        let make_range = |year: i32, month: u32| MonthRange {
            year,
            month,
            start_sec: sep_start,
            end_sec: sep_start + 30 * 86_400,
            today: String::new(),
            tz: gmt8(),
        };
        // 9 月 1 日附近：近 7 天跨到 8 月 → 补查 8 月窗口，end 为本月起点
        let crossed = previous_month_window(&make_range(2026, 9), sep_start - 5 * 86_400)
            .expect("cross month");
        assert_eq!((crossed.year, crossed.month), (2026, 8));
        assert!(crossed.start_sec < crossed.end_sec);
        assert_eq!(crossed.end_sec, sep_start);
        // 月中：近 7 天都在本月内 → 不补查
        assert!(previous_month_window(&make_range(2026, 9), sep_start + 10 * 86_400).is_none());
        // 1 月边界：上月是上一年 12 月
        let january = previous_month_window(&make_range(2027, 1), sep_start - 5 * 86_400)
            .expect("cross year");
        assert_eq!((january.year, january.month), (2026, 12));
    }

    #[test]
    fn parses_official_summary_total_usage() {
        let json = serde_json::json!({
            "code": 0,
            "data": { "biz_data": { "total_usage": "68.50", "monthly_usage": 18.16 } }
        });
        assert_eq!(parse_total_spend(&json).unwrap().to_string(), "68.50");
        let nested = serde_json::json!({ "data": { "totalUsage": { "amount": "12.50", "currency": "CNY" } } });
        assert_eq!(parse_total_spend(&nested).unwrap().to_string(), "12.50");
    }
}
