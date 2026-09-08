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
        .unwrap_or(0);

    let is_reply = item
        .activity_type
        .as_deref()
        .map(|t| t.eq_ignore_ascii_case("reply"))
        .unwrap_or(false);

    let raw_category = item
        .tweet_assessment
        .as_ref()
        .and_then(|a| a.category.as_deref())
        .unwrap_or("unknown");

    let (kind, explicit_reset, label) = match raw_category {
        "reset_announced" | "reset_completed" => ("reset".to_string(), true, "可能重置"),
        "banked_reset" => ("signal".to_string(), true, "重置卡"),
        "indirect" | "event_hint" | "release_hint" => ("indirect".to_string(), false, "间接相关"),
        other if other != "none" && other != "unknown" => (other.to_string(), false, "间接相关"),
        _ => {
            let (h_kind, h_explicit, h_label) =
                classify_text_heuristically(&text, item.context.as_deref());
            (h_kind.to_string(), h_explicit, h_label)
        }
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

pub fn classify_text_heuristically(
    text: &str,
    context: Option<&str>,
) -> (&'static str, bool, &'static str) {
    let lower_text = text.to_ascii_lowercase();
    let lower_ctx = context.map(|c| c.to_ascii_lowercase()).unwrap_or_default();

    // 1. Banked reset (重置卡到账/发放)
    if lower_text.contains("banked reset")
        || lower_text.contains("reset card")
        || lower_text.contains("banked usage")
        || lower_text.contains("full banked reset")
        || lower_text.contains("extra reset")
        || lower_ctx.contains("banked reset")
        || lower_ctx.contains("reset card")
    {
        return ("signal", true, "重置卡");
    }

    // 2. Direct Quota Reset (明确表示正在/将要全局或全员重置额度)
    if lower_text.contains("all reset for everyone")
        || lower_text.contains("resets are live")
        || lower_text.contains("reset all paid")
        || lower_text.contains("resetting usage for all")
        || lower_text.contains("quota reset is live")
        || lower_text.contains("quotas have been reset")
        || lower_text.contains("limits have been reset")
        || lower_text.contains("just reset everyone")
        || lower_text.contains("full reset for all")
    {
        return ("reset", true, "可能重置");
    }

    // 3. Indirect / Hint (提及额度重置、使用量重置、排期、讨论等间接相关)
    if lower_text.contains("only resets")
        || lower_text.contains("reset usage")
        || lower_text.contains("quota reset")
        || lower_text.contains("quotas reset")
        || lower_text.contains("limit reset")
        || lower_text.contains("limits reset")
        || lower_text.contains("just reset")
        || lower_text.contains("usage has reset")
        || lower_text.contains("full reset")
        || lower_text.contains("schedule")
        || lower_text.contains("devday")
        || lower_text.contains("rate limit")
        || lower_text.contains("weekly limit")
        || lower_text.contains("monthly limit")
        || lower_text.contains("cooldown")
        || lower_text.contains("cool down")
        || lower_text.contains("rolling out")
        || lower_text.contains("rollout")
        || lower_text.contains("hold on to your codex")
        || lower_text.contains("milestone")
        || lower_ctx.contains("quota reset")
        || lower_ctx.contains("weekly limit")
    {
        return ("indirect", false, "间接相关");
    }

    ("none", false, "无重置信号")
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
            },
            {
                "activityType": "post",
                "context": "",
                "guid": "2097174560412246215",
                "link": "https://x.com/thsottiaux/status/2097174560412246215",
                "pubDate": "2026-09-08T04:05:00.000Z",
                "title": "All reset for everyone. Enjoy the week with Astra."
            },
            {
                "activityType": "post",
                "context": "",
                "guid": "2097175062566846501",
                "link": "https://x.com/thsottiaux/status/2097175062566846501",
                "pubDate": "2026-09-08T04:07:00.000Z",
                "title": "There is no schedule, only resets"
            },
            {
                "activityType": "reply",
                "context": "Can we get more allowance?",
                "guid": "2096035748130795560",
                "link": "https://x.com/thsottiaux/status/2096035748130795560",
                "pubDate": "2026-09-05T01:23:00.000Z",
                "title": "we will do the full banked reset today too for all Plus, Pro and Business users."
            }
        ]
    }"#;

    #[test]
    fn parses_sample_willcodex_posts() {
        let posts = parse_posts(SAMPLE_JSON, 1000).expect("should parse");
        assert_eq!(posts.len(), 5);
        assert_eq!(posts[0].id, "2096717905614524491");
        assert!(!posts[0].is_reply);
        assert!(posts[0].text.contains("improvements"));

        assert_eq!(posts[1].id, "2096847737153012204");
        assert!(posts[1].is_reply);
        assert!(posts[1].extra_json.contains("Previous context"));

        // Post 2: All reset for everyone -> reset & explicit_reset
        assert_eq!(posts[2].id, "2097174560412246215");
        assert_eq!(posts[2].kind, "reset");
        assert!(posts[2].explicit_reset);
        assert_eq!(posts[2].tibo_lane.as_deref(), Some("可能重置"));

        // Post 3: only resets -> indirect & not explicit_reset
        assert_eq!(posts[3].id, "2097175062566846501");
        assert_eq!(posts[3].kind, "indirect");
        assert!(!posts[3].explicit_reset);
        assert_eq!(posts[3].tibo_lane.as_deref(), Some("间接相关"));

        // Post 4: full banked reset -> signal & explicit_reset
        assert_eq!(posts[4].id, "2096035748130795560");
        assert_eq!(posts[4].kind, "signal");
        assert!(posts[4].explicit_reset);
        assert_eq!(posts[4].tibo_lane.as_deref(), Some("重置卡"));
    }

    #[test]
    fn heuristic_classification_matches_known_phrases() {
        assert_eq!(
            classify_text_heuristically("All reset for everyone. Enjoy the week with Astra.", None),
            ("reset", true, "可能重置")
        );
        assert_eq!(
            classify_text_heuristically("There is no schedule, only resets", None),
            ("indirect", false, "间接相关")
        );
        assert_eq!(
            classify_text_heuristically("You forgot the part where I reset usage twice in the middle", None),
            ("indirect", false, "间接相关")
        );
        assert_eq!(
            classify_text_heuristically("we will do the full banked reset today", None),
            ("signal", true, "重置卡")
        );
        assert_eq!(
            classify_text_heuristically("See you at DevDay tomorrow", None),
            ("indirect", false, "间接相关")
        );
        assert_eq!(
            classify_text_heuristically("Let the potato rest a little", None),
            ("none", false, "无重置信号")
        );
    }
}
