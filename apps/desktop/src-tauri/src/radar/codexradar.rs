//! 解析 https://codexradar.com/ 公开首页中的 Tibo 雷达区与顶部公告。
//! 没有 JSON 接口，按稳定的 class / data-* 抽取，避免依赖易变的样式顺序。

use super::RadarNotice;
use crate::storage::repository::TiboPostRecord;
use serde_json::json;

pub fn parse_page(html: &str, synced_at: i64) -> Result<(Vec<TiboPostRecord>, Option<RadarNotice>), String> {
    let notice = parse_notice(html);
    let posts = parse_posts(html, synced_at)?;
    if posts.is_empty() {
        return Err("CodexRadar 未返回可展示的 Tibo 动态".into());
    }
    Ok((posts, notice))
}

fn parse_notice(html: &str) -> Option<RadarNotice> {
    let start = html.find(r#"class="site-announcement"#)?;
    let rest = html.get(start..)?;
    let end = rest.find("</section>").unwrap_or_else(|| floor_char_boundary(rest, 12_000.min(rest.len())));
    let section = rest.get(..end)?;
    let headline = class_text(section, "site-announcement-headline")?;
    if headline.is_empty() {
        return None;
    }
    let lead = class_text(section, "site-announcement-lead").filter(|value| !value.is_empty());
    let items = list_items(section, "site-announcement-items");
    Some(RadarNotice { headline, lead, items })
}

fn parse_posts(html: &str, synced_at: i64) -> Result<Vec<TiboPostRecord>, String> {
    let start = html
        .find(r#"class="reset-tibo-posts""#)
        .ok_or_else(|| "CodexRadar 页面未找到 Tibo 动态".to_string())?;
    let ol = html
        .get(start..)
        .and_then(|rest| rest.split("</ol>").next())
        .ok_or_else(|| "CodexRadar Tibo 列表不完整".to_string())?;
    let mut posts = Vec::new();
    for chunk in ol.split(r#"class="reset-tibo-post""#).skip(1) {
        if let Some(post) = parse_one_post(chunk, synced_at) {
            posts.push(post);
        }
    }
    Ok(posts)
}

fn parse_one_post(chunk: &str, synced_at: i64) -> Option<TiboPostRecord> {
    let id = attr(chunk, "data-tibo-post-id").or_else(|| status_id(chunk))?;
    let url = attr(chunk, "href").filter(|value| value.contains("status/")).unwrap_or_else(|| {
        format!("https://x.com/thsottiaux/status/{id}")
    });
    let relevance = attr(chunk, "data-reset-relevance").unwrap_or_else(|| "none".into());
    let label = class_text(chunk, "reset-tibo-post-relevance").unwrap_or_else(|| label_for(&relevance));
    let original = labeled_paragraph(chunk, "reset-tibo-post-original")?;
    if original.is_empty() {
        return None;
    }
    let translation = labeled_paragraph(chunk, "reset-tibo-post-translation");
    let summary = labeled_paragraph(chunk, "reset-tibo-post-preview");
    let analysis = labeled_paragraph(chunk, "reset-tibo-post-analysis");
    let posted_at = class_attr(chunk, "time", "datetime")
        .and_then(|value| parse_datetime(&value))
        .unwrap_or(synced_at);
    let extra_json = json!({
        "summary": summary,
        "analysis": analysis,
        "relevance": relevance,
        "signalLabel": label,
    })
    .to_string();
    let explicit_reset = is_explicit_reset(&relevance, &label);
    Some(TiboPostRecord {
        id,
        url,
        text: original,
        posted_at,
        kind: relevance.clone(),
        tibo_lane: Some(label.clone()),
        explicit_reset,
        verification_status: None,
        is_reply: false,
        replies: 0,
        reposts: 0,
        likes: 0,
        extra_json,
        synced_at,
        translated_text: translation.clone(),
        translated_at: translation.as_ref().map(|_| synced_at),
        translation_source: translation.as_ref().map(|_| "codexradar".into()),
    })
}

fn is_explicit_reset(relevance: &str, label: &str) -> bool {
    matches!(relevance, "direct" | "reset" | "signal")
        || (label.contains("重置") && !label.contains("无重置") && !label.contains("间接"))
}

fn label_for(relevance: &str) -> String {
    match relevance {
        "none" => "无重置信号".into(),
        "indirect" => "间接相关".into(),
        "direct" | "reset" | "signal" => "重置相关".into(),
        other => other.to_string(),
    }
}

fn labeled_paragraph(block: &str, class: &str) -> Option<String> {
    class_text(block, class).filter(|value| !value.is_empty())
}

fn class_text(block: &str, class: &str) -> Option<String> {
    let needle = format!("class=\"{class}\"");
    let class_at = block.find(&needle)?;
    let tag_start = block.get(..class_at)?.rfind('<')?;
    let tag_name: String = block
        .get(tag_start + 1..)?
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric())
        .collect();
    if tag_name.is_empty() {
        return None;
    }
    let after_class = block.get(class_at..)?;
    let tag_end = after_class.find('>')?;
    let inner_start = class_at + tag_end + 1;
    let rest = block.get(inner_start..)?;
    let close = format!("</{tag_name}>");
    let end = rest.find(&close)?;
    Some(strip_leading_label(&html_text(&rest[..end])))
}

fn class_attr(block: &str, tag: &str, name: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = block.find(&open)?;
    let end = floor_char_boundary(block, start.saturating_add(400).min(block.len()));
    attr(block.get(start..end)?, name)
}

fn attr(block: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let start = block.find(&key)? + key.len();
    let rest = block.get(start..)?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn status_id(block: &str) -> Option<String> {
    let start = block.find("/status/")?;
    let rest = &block[start + "/status/".len()..];
    let id: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

fn list_items(block: &str, class: &str) -> Vec<String> {
    let Some(start) = block.find(&format!("class=\"{class}\"")) else {
        return Vec::new();
    };
    let rest = &block[start..];
    rest.split("<li")
        .skip(1)
        .filter_map(|item| {
            let inner_start = item.find('>')? + 1;
            let inner = item.get(inner_start..)?;
            let end = inner.find("</li>")?;
            let text = strip_leading_label(&html_text(&inner[..end]));
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        })
        .collect()
}

fn strip_leading_label(text: &str) -> String {
    let labels = [
        "内容摘要",
        "英文原文",
        "中文翻译",
        "关键词句",
        "模型语境解读 · 参考比利时法语语感",
        "模型语境解读",
    ];
    let trimmed = text.trim();
    for label in labels {
        if let Some(rest) = trimmed.strip_prefix(label) {
            return rest.trim().to_string();
        }
    }
    trimmed.to_string()
}

fn html_text(raw: &str) -> String {
    let with_breaks = raw
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");
    let mut out = String::new();
    let mut in_tag = false;
    for ch in with_breaks.chars() {
        if ch == '<' {
            in_tag = true;
            continue;
        }
        if ch == '>' {
            in_tag = false;
            continue;
        }
        if !in_tag {
            out.push(ch);
        }
    }
    decode_entities(&out)
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn decode_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn floor_char_boundary(value: &str, index: usize) -> usize {
    if index >= value.len() {
        return value.len();
    }
    let mut i = index;
    while i > 0 && !value.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn parse_datetime(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
<section class="site-announcement site-announcement-news" aria-label="重要公告">
  <strong class="site-announcement-headline">Tibo：明天可能迎来 Codex 新里程碑</strong>
  <span class="site-announcement-lead">请关注 Codex 仪表板</span>
  <ol class="site-announcement-items">
    <li><span>这是新的官方弱信号，但尚未直接确认新一轮重置。</span></li>
  </ol>
</section>
<ol class="reset-tibo-posts">
  <li class="reset-tibo-post" data-tibo-post-id="2093573991965557198" data-reset-relevance="indirect">
    <a href="https://x.com/thsottiaux/status/2093573991965557198">Tibo X</a>
    <time datetime="2026-08-29T13:38:31+08:00">8月29日 13:38</time>
    <span class="reset-tibo-post-relevance">间接相关</span>
    <p class="reset-tibo-post-preview"><b>内容摘要</b>看了一下仪表盘。</p>
    <p class="reset-tibo-post-original" lang="en"><b>英文原文</b>Looking at the dashboard</p>
    <p class="reset-tibo-post-translation" lang="zh-CN"><b>中文翻译</b>看了一下仪表盘。</p>
    <p class="reset-tibo-post-analysis"><b>模型语境解读 · 参考比利时法语语感</b>不能据此开启窗口。</p>
  </li>
  <li class="reset-tibo-post" data-tibo-post-id="2093573575869698091" data-reset-relevance="none">
    <a href="https://x.com/thsottiaux/status/2093573575869698091">Tibo X</a>
    <time datetime="2026-08-29T13:36:51+08:00">8月29日 13:36</time>
    <span class="reset-tibo-post-relevance">无重置信号</span>
    <p class="reset-tibo-post-original" lang="en"><b>英文原文</b>This will continue to be supported</p>
    <p class="reset-tibo-post-translation" lang="zh-CN"><b>中文翻译</b>这确实会继续得到支持。</p>
  </li>
</ol>
"#;

    #[test]
    fn parses_notice_and_posts() {
        let (posts, notice) = parse_page(SAMPLE, 1).expect("page");
        let notice = notice.expect("notice");
        assert!(notice.headline.contains("里程碑"));
        assert_eq!(posts.len(), 2);
        assert_eq!(posts[0].id, "2093573991965557198");
        assert_eq!(posts[0].translated_text.as_deref(), Some("看了一下仪表盘。"));
        assert_eq!(posts[0].translation_source.as_deref(), Some("codexradar"));
        assert!(!posts[0].explicit_reset);
        assert_eq!(posts[1].kind, "none");
    }

    #[test]
    fn parses_saved_homepage_if_present() {
        let path = std::env::temp_dir().join("codexradar.html");
        let Ok(html) = std::fs::read_to_string(&path) else {
            return;
        };
        if !html.contains("reset-tibo-posts") {
            return;
        }
        let (posts, notice) = parse_page(&html, 1).expect("live page");
        assert!(!posts.is_empty());
        assert!(posts.iter().all(|post| !post.text.is_empty()));
        assert!(notice.is_some());
    }
}
