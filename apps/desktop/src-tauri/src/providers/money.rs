//! 金额解析共用：JSON 数字按原文转 Decimal，禁止用 f64 汇总。

use rust_decimal::Decimal;
use serde_json::Value;
use std::str::FromStr;

pub const WEB_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36";

pub fn decimal_from_json(value: &Value) -> Option<Decimal> {
    match value {
        Value::Number(number) => Decimal::from_str(&number.to_string()).ok(),
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Decimal::from_str(trimmed).ok()
            }
        }
        _ => None,
    }
}

pub fn pick_decimal(root: &Value, keys: &[&str]) -> Option<Decimal> {
    let data = root.get("data").unwrap_or(root);
    for object in [data, root] {
        for key in keys {
            if let Some(amount) = object.get(*key).and_then(decimal_from_json) {
                return Some(amount);
            }
        }
    }
    None
}

pub fn format_cny(amount: Decimal) -> String {
    format!("¥{amount:.2}")
}

pub fn format_usd(amount: Decimal) -> String {
    format!("${amount:.2}")
}

/// 百分比展示：整数值去掉 `.0`，保留一位有效小数。
pub fn format_percent(value: f64) -> String {
    let text = format!("{value:.1}");
    format!("{}%", text.strip_suffix(".0").unwrap_or(&text))
}

pub fn extract_cookie_value(header: &str, name: &str) -> Option<String> {
    header
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn extract_token_cookie(header: &str) -> Option<String> {
    if let Some(value) = extract_cookie_value(header, "bigmodel_token_production") {
        return Some(value);
    }
    header
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(key, value)| {
            let key = key.to_ascii_lowercase();
            let value = value.trim();
            if value.len() >= 20
                && (key.contains("token") || key.contains("auth"))
                && !key.contains("csrf")
                && !key.contains("expire")
            {
                Some(value.to_string())
            } else {
                None
            }
        })
}

pub fn cookie_named(header: &str, name: &str) -> bool {
    if name == "serviceToken" {
        return header.split(';').any(|part| {
            let (key, value) = match part.trim().split_once('=') {
                Some(pair) => pair,
                None => return false,
            };
            key.contains("serviceToken") && !value.trim().is_empty()
        });
    }
    extract_cookie_value(header, name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_json_number_without_f64() {
        let amount = decimal_from_json(&json!(16.24)).expect("number");
        assert_eq!(format_cny(amount), "¥16.24");
    }

    #[test]
    fn parses_json_string_amount() {
        let amount = decimal_from_json(&json!("4.20")).expect("string");
        assert_eq!(format_cny(amount), "¥4.20");
    }

    #[test]
    fn extracts_named_cookie() {
        let header = "Hm_lvt=abc; bigmodel_token_production=eyJhbGciOiJIUzI1NiJ9.token; acw_tc=xyz";
        assert_eq!(
            extract_cookie_value(header, "bigmodel_token_production").as_deref(),
            Some("eyJhbGciOiJIUzI1NiJ9.token")
        );
        assert!(cookie_named(
            "api-platform_serviceToken=abc; other=1",
            "serviceToken"
        ));
        assert_eq!(
            extract_token_cookie("foo=1; access_token=eyJhbGciOiJIUzI1NiJ9.payload.sig; x=2")
                .as_deref(),
            Some("eyJhbGciOiJIUzI1NiJ9.payload.sig")
        );
    }

    #[test]
    fn drops_trailing_point_zero() {
        assert_eq!(format_percent(40.0), "40%");
        assert_eq!(format_percent(62.5), "62.5%");
        assert_eq!(format_percent(100.0), "100%");
    }
}
