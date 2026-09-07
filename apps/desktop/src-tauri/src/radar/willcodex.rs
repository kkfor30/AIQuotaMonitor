//! 解析 https://www.willcodexquotareset.com/api/forecast 返回的结构化 JSON。
//! 包含 30 分钟级实时刷新的 Tibo 推文列表（tiboPosts）、回复上下文及官方重置研判。

use crate::storage::repository::TiboPostRecord;
use chrono::DateTime;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct WillCodexForecastResponse {
    pub tibo_posts: Option<Vec<WillCodexPostItem>>,
    pub fetched_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct WillCodexPostItem {
    pub guid: Option<String>,
    pub id: Option<String>,
    pub link: Option<String>,
    pub title: Option<String>,
    pub text: Option<String>,
    pub pub_date: Option<String>,
    pub published_at: Option<String>,
    pub activity_type: Option<String>,
    pub context: Option<String>,
    pub reply_to_author: Option<String>,
    pub reply_to_guid: Option<String>,
    pub tweet_assessment: Option<WillCodexAssessment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct WillCodexAssessment {
    pub category: Option<String>,
    pub reason: Option<String>,
    pub reset_signal_strength: Option<i64>,
}

pub fn parse_posts(body: &str, synced_at: i64) -> Result<Vec<TiboPostRecord>, String> {
    let resp: WillCodexForecastResponse = serde_json::from_str(body)
        .map_err(|err| format!("WillCodex JSON 反序列化失败: {err}"))?;
    let items = resp.tibo_posts.unwrap_or_default();
    if items.is_empty() {
        return Err("WillCodex 未返回有效的 Tibo 动态".into());
    }

    let mut records = Vec::with_capacity(items.len());
    for item in items {
        if let Some(record) = convert_item(item, synced_at) {
            records.push(record);
        }
    }
    if records.is_empty() {
        return Err("WillCodex 未能解析出有效的 Tibo 记录".into());
    }
    Ok(records)
}

fn convert_item(item: WillCodexPostItem, synced_at: i64) -> Option<TiboPostRecord> {
    let id = item.guid.or(item.id)?;
    if id.is_empty() {
        return None;
    }
    let text = item
        .title
        .filter(|s| !s.trim().is_empty())
        .or_else(|| item.text.filter(|s| !s.trim().is_empty()))?;

    let url = item
        .link
        .filter(|s| s.contains("status/"))
        .unwrap_or_else(|| format!("https://x.com/thsottiaux/status/{id}"));

    let posted_at = item
        .pub_date
        .as_deref()
        .or(item.published_at.as_deref())
        .and_then(parse_datetime)
        .unwrap_or(synced_at);

    let is_reply = item
        .activity_type
        .as_deref()
        .map(|t| t.eq_ignore_ascii_case("reply"))
        .unwrap_or(false);

    let category = item
        .tweet_assessment
        .as_ref()
        .and_then(|a| a.category.as_deref())
        .unwrap_or("none");

    let explicit_reset = matches!(
        category,
        "reset_announced" | "reset_completed" | "banked_reset"
    );

    let kind = match category {
        "reset_announced" | "reset_completed" => "reset".to_string(),
        "banked_reset" => "signal".to_string(),
        "indirect" | "event_hint" | "release_hint" => "indirect".to_string(),
        other if other != "none" => other.to_string(),
        _ => {
            if explicit_reset {
                "reset".to_string()
            } else {
                "none".to_string()
            }
        }
    };

    let label = match kind.as_str() {
        "reset" => "可能重置",
        "signal" => "重置卡",
        "indirect" => "间接相关",
        _ => "无重置信号",
    };

    let mut extra = serde_json::Map::new();
    extra.insert("source".into(), json!("willcodex"));
    extra.insert("relevance".into(), json!(kind));
    extra.insert("signalLabel".into(), json!(label));

    if let Some(ctx) = item.context.filter(|s| !s.trim().is_empty()) {
        extra.insert("context".into(), json!(ctx));
    }
    if let Some(assessment) = item.tweet_assessment {
        if let Some(reason) = assessment.reason {
            extra.insert("analysis".into(), json!(reason));
        }
    }
    if let Some(author) = item.reply_to_author.filter(|s| !s.is_empty()) {
        extra.insert("replyToAuthor".into(), json!(author));
    }

    Some(TiboPostRecord {
        id,
        url,
        text,
        posted_at,
        kind,
        tibo_lane: Some(label.to_string()),
        explicit_reset,
        verification_status: None,
        is_reply,
        replies: 0,
        reposts: 0,
        likes: 0,
        extra_json: serde_json::Value::Object(extra).to_string(),
        synced_at,
        translated_text: None,
        translated_at: None,
        translation_source: None,
        lifecycle_consumed_at: None,
    })
}

fn parse_datetime(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_JSON: &str = r#"{
        "fetchedAt": "2026-09-07T07:30:23.147Z",
        "tiboPosts": [
            {
                "activityType": "post",
                "context": "",
                "guid": "2096717905614524491",
                "link": "https://x.com/thsottiaux/status/2096717905614524491",
                "pubDate": "2026-09-06T21:51:18.000Z",
                "replyToAuthor": "",
                "title": "We've made some improvements that improve usage on the long tail for power users of Astra when logged in with your ChatGPT account."
            },
            {
                "activityType": "reply",
                "context": "Previous context tweet text",
                "guid": "2096847737153012204",
                "link": "https://x.com/thsottiaux/status/2096847737153012204",
                "pubDate": "2026-09-07T06:27:12.000Z",
                "replyToAuthor": "someone",
                "title": "Reply content here"
            }
        ]
    }"#;

    #[test]
    fn parses_sample_willcodex_posts() {
        let posts = parse_posts(SAMPLE_JSON, 1000).expect("should parse");
        assert_eq!(posts.len(), 2);
        assert_eq!(posts[0].id, "2096717905614524491");
        assert!(!posts[0].is_reply);
        assert!(posts[0].text.contains("improvements"));

        assert_eq!(posts[1].id, "2096847737153012204");
        assert!(posts[1].is_reply);
        assert!(posts[1].extra_json.contains("Previous context"));
    }
}
