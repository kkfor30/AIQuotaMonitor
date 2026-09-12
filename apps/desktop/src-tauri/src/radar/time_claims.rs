//! Tibo 原帖中的时间声明解析。
//! 只对高置信规则返回北京时间；无法确定日期、时区或 am/pm 时主动降级 ambiguous。

use chrono::{
    Datelike, Duration, FixedOffset, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone,
    Weekday,
};
use chrono_tz::America::Los_Angeles;
use regex::Regex;
use std::sync::OnceLock;

pub const TIMEZONE_POLICY_VERSION: &str = "time-v5";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTimeClaim {
    pub post_id: String,
    pub raw_text: String,
    pub clock_hour: Option<u32>,
    pub clock_minute: Option<u32>,
    pub date_relation: Option<String>,
    pub timezone_kind: Option<String>,
    pub timezone_assumed: bool,
    /// resolved | ambiguous
    pub parse_status: String,
    pub resolved_at: Option<i64>,
    /// exact | assumed | ambiguous
    pub precision: String,
    /// grant | deadline | historical | unknown：时间语义归属。
    /// 预计重置时间只允许 grant/unknown；截止时间与历史时间不得冒充预告。
    pub claim_kind: String,
}

#[derive(Debug, Clone, Copy)]
enum ZoneSpec {
    Pst,
    Pdt,
    Pacific,
}

#[derive(Debug)]
struct Candidate {
    raw: String,
    hour: Option<u32>,
    minute: Option<u32>,
    relation: Option<String>,
    weekday: Option<Weekday>,
    explicit_date: Option<(u32, u32)>,
    zone: ZoneSpec,
    assumed: bool,
    ambiguous_clock: bool,
}

/// 原文与译文都解析：官方帖常把 tonight 放在原文、把「午夜」放在译文。
pub fn parse_post_time_claims_with_translation(
    post_id: &str,
    text: &str,
    translated_text: Option<&str>,
    posted_at: i64,
) -> Vec<ParsedTimeClaim> {
    let mut claims = parse_post_time_claims(post_id, text, posted_at);
    let Some(translated) = translated_text.map(str::trim).filter(|value| !value.is_empty()) else {
        return claims;
    };
    for claim in parse_post_time_claims(post_id, translated, posted_at) {
        if !claims.iter().any(|existing| {
            existing.raw_text == claim.raw_text && existing.resolved_at == claim.resolved_at
        }) {
            claims.push(claim);
        }
    }
    claims
}

pub fn parse_post_time_claims(post_id: &str, text: &str, posted_at: i64) -> Vec<ParsedTimeClaim> {
    if posted_at <= 0 { return Vec::new(); }
    let mut candidates = Vec::new();
    let explicit_date = find_explicit_date(text);

    for captures in ampm_regex().captures_iter(text) {
        let Some(full) = captures.get(0) else {
            continue;
        };
        let hour = captures
            .get(3)
            .and_then(|value| value.as_str().parse::<u32>().ok());
        let minute = captures
            .get(4)
            .and_then(|value| value.as_str().parse::<u32>().ok())
            .unwrap_or(0);
        let meridiem = captures
            .get(5)
            .map(|value| value.as_str().to_ascii_lowercase().replace('.', ""));
        let converted = match (hour, meridiem.as_deref()) {
            (Some(12), Some("am")) => Some(0),
            (Some(12), Some("pm")) => Some(12),
            (Some(value @ 1..=11), Some("pm")) => Some(value + 12),
            (Some(value @ 1..=11), Some("am")) => Some(value),
            _ => None,
        };
        let (zone, assumed) = zone_from(captures.get(6).map(|value| value.as_str()));
        candidates.push(Candidate {
            raw: full.as_str().to_string(),
            hour: converted,
            minute: Some(minute),
            relation: captures
                .get(1)
                .map(|value| value.as_str().to_ascii_lowercase()),
            weekday: captures
                .get(2)
                .and_then(|value| parse_weekday(value.as_str())),
            explicit_date,
            zone,
            assumed,
            ambiguous_clock: converted.is_none(),
        });
    }

    for captures in clock24_regex().captures_iter(text) {
        let Some(full) = captures.get(0) else {
            continue;
        };
        if candidates
            .iter()
            .any(|candidate| overlaps(text, &candidate.raw, full.as_str()))
        {
            continue;
        }
        let raw_hour = captures
            .get(3)
            .map(|value| value.as_str())
            .unwrap_or_default();
        let hour = raw_hour.parse::<u32>().ok();
        let minute = captures
            .get(4)
            .and_then(|value| value.as_str().parse::<u32>().ok());
        let (zone, assumed) = zone_from(captures.get(5).map(|value| value.as_str()));
        let unambiguous_24h = raw_hour.starts_with('0') || hour.is_some_and(|value| value > 12);
        candidates.push(Candidate {
            raw: full.as_str().to_string(),
            hour,
            minute,
            relation: captures
                .get(1)
                .map(|value| value.as_str().to_ascii_lowercase()),
            weekday: captures
                .get(2)
                .and_then(|value| parse_weekday(value.as_str())),
            explicit_date,
            zone,
            assumed,
            ambiguous_clock: !unambiguous_24h,
        });
    }

    for captures in named_clock_regex().captures_iter(text) {
        let Some(full) = captures.get(0) else {
            continue;
        };
        let named = captures
            .get(3)
            .map(|value| value.as_str().to_ascii_lowercase());
        let (hour, minute) = match named.as_deref() {
            Some("noon") => (Some(12), Some(0)),
            Some("midnight") => (Some(0), Some(0)),
            _ => (None, None),
        };
        let (zone, assumed) = zone_from(captures.get(4).map(|value| value.as_str()));
        candidates.push(Candidate {
            raw: full.as_str().to_string(),
            hour,
            minute,
            relation: captures
                .get(1)
                .map(|value| value.as_str().to_ascii_lowercase()),
            weekday: captures
                .get(2)
                .and_then(|value| parse_weekday(value.as_str())),
            explicit_date,
            zone,
            assumed,
            ambiguous_clock: hour.is_none(),
        });
    }

    push_midnight_relations(text, explicit_date, &mut candidates);

    let mut claims = candidates
        .into_iter()
        .map(|mut candidate| {
            if candidate.relation.is_none() {
                if let Some(pos) = text.find(&candidate.raw) {
                    let after = text[pos + candidate.raw.len()..].trim_start().to_ascii_lowercase();
                    for relation in ["today", "tomorrow", "tonight"] {
                        if after.starts_with(relation) { candidate.relation = Some(relation.into()); break; }
                    }
                }
            }
            resolve_candidate(post_id, candidate, posted_at)
        })
        .collect::<Vec<_>>();
    if text.to_ascii_lowercase().contains(" or ") {
        let resolved = claims
            .iter()
            .filter_map(|claim| claim.resolved_at)
            .collect::<std::collections::HashSet<_>>();
        if resolved.len() > 1 {
            for claim in &mut claims {
                claim.parse_status = "ambiguous".into();
                claim.resolved_at = None;
                claim.precision = "ambiguous".into();
            }
        }
    }
    let lowered = text.to_ascii_lowercase();
    for claim in &mut claims {
        if let Some(pos) = lowered.find(&claim.raw_text.to_ascii_lowercase()) {
            let prefix: String = lowered[..pos].chars().rev().take(20).collect::<String>().chars().rev().collect();
            let after: String = lowered[pos + claim.raw_text.len()..].chars().take(12).collect();
            if claim.resolved_at.is_some()
                && ["around", "about", "approximately", "左右", "大约", "大概", "约"]
                    .iter()
                    .any(|marker| prefix.contains(marker) || after.contains(marker))
            {
                claim.precision = "approximate".into();
            }
        }
        claim.claim_kind = classify_claim_kind(&lowered, &claim.raw_text.to_ascii_lowercase()).into();
    }
    claims
}

/// 时间声明语义分类：只在时间表达所在的句子内做保守关键词匹配，
/// 相邻句子的截止/历史措辞不污染本句预告时间；识别不了保持 unknown。
fn classify_claim_kind(lowered_text: &str, raw_lower: &str) -> &'static str {
    let Some(position) = lowered_text.find(raw_lower) else {
        return "unknown";
    };
    // 句子边界（. ! ? ; 换行）内的局部窗口，两侧再各限 80 字符防止超长句误伤。
    const BREAKS: [char; 5] = ['.', '!', '?', ';', '\n'];
    let sentence_start = lowered_text[..position]
        .rfind(|ch: char| BREAKS.contains(&ch))
        .map(|index| index + 1)
        .unwrap_or(0)
        .max(position.saturating_sub(80));
    let sentence_end = lowered_text[position..]
        .find(|ch: char| BREAKS.contains(&ch))
        .map(|offset| position + offset)
        .unwrap_or(lowered_text.len())
        .min(position + raw_lower.chars().count() + 80);
    let mut safe_start = sentence_start;
    let mut safe_end = sentence_end;
    while !lowered_text.is_char_boundary(safe_start) {
        safe_start += 1;
    }
    while !lowered_text.is_char_boundary(safe_end) {
        safe_end -= 1;
    }
    let window = &lowered_text[safe_start..safe_end];
    const DEADLINE_MARKERS: &[&str] = &[
        "deadline", "last chance", "ends at", "ends on", "ends today", "ends tonight",
        "cutoff", "cut off", "expires", "last day", "final day", "last call",
    ];
    const HISTORICAL_MARKERS: &[&str] = &[
        "already", "yesterday", "was reset", "has been reset", "has reset",
        "last reset", "earlier today", "last week", "last month",
    ];
    if DEADLINE_MARKERS.iter().any(|marker| window.contains(marker)) {
        return "deadline";
    }
    if HISTORICAL_MARKERS.iter().any(|marker| window.contains(marker)) {
        return "historical";
    }
    if ["reset", "lands", "landing", "will receive", "grant", "roll out", "rollout", "available", "重置"].iter().any(|m|window.contains(m)) { return "grant"; }
    "unknown"
}

fn resolve_candidate(post_id: &str, candidate: Candidate, posted_at: i64) -> ParsedTimeClaim {
    let zone_kind = match candidate.zone {
        ZoneSpec::Pst => "PST",
        ZoneSpec::Pdt => "PDT",
        ZoneSpec::Pacific => "PT",
    };
    let ambiguous = || ParsedTimeClaim {
        post_id: post_id.to_string(),
        raw_text: candidate.raw.clone(),
        clock_hour: candidate.hour,
        clock_minute: candidate.minute,
        date_relation: candidate.relation.clone(),
        timezone_kind: Some(zone_kind.into()),
        timezone_assumed: candidate.assumed,
        parse_status: "ambiguous".into(),
        resolved_at: None,
        precision: "ambiguous".into(),
        claim_kind: "unknown".into(),
    };
    if candidate.ambiguous_clock {
        return ambiguous();
    }
    let (Some(hour), Some(minute)) = (candidate.hour, candidate.minute) else {
        return ambiguous();
    };
    let Some(time) = NaiveTime::from_hms_opt(hour, minute, 0) else {
        return ambiguous();
    };
    let published_local = match candidate.zone {
        ZoneSpec::Pst => timestamp_in_fixed(posted_at, -8),
        ZoneSpec::Pdt => timestamp_in_fixed(posted_at, -7),
        ZoneSpec::Pacific => chrono::DateTime::from_timestamp_millis(posted_at)
            .map(|value| value.with_timezone(&Los_Angeles).naive_local()),
    };
    let Some(published_local) = published_local else {
        return ambiguous();
    };
    let Some(date) = resolve_date(
        &candidate,
        published_local.date(),
        published_local.time(),
        time,
    ) else {
        return ambiguous();
    };
    let naive = NaiveDateTime::new(date, time);
    let timestamp = match candidate.zone {
        ZoneSpec::Pst => fixed_local_timestamp(naive, -8),
        ZoneSpec::Pdt => fixed_local_timestamp(naive, -7),
        ZoneSpec::Pacific => match Los_Angeles.from_local_datetime(&naive) {
            LocalResult::Single(value) => Some(value.timestamp_millis()),
            LocalResult::Ambiguous(_, _) | LocalResult::None => None,
        },
    };
    let Some(timestamp) = timestamp else {
        return ambiguous();
    };
    let beijing = FixedOffset::east_opt(8 * 3600).expect("UTC+8 is valid");
    let resolved_at = chrono::DateTime::from_timestamp_millis(timestamp)
        .map(|value| value.with_timezone(&beijing).timestamp_millis());
    ParsedTimeClaim {
        post_id: post_id.to_string(),
        raw_text: candidate.raw,
        clock_hour: Some(hour),
        clock_minute: Some(minute),
        date_relation: candidate
            .relation
            .or_else(|| candidate.weekday.map(|value| format!("weekday:{value:?}"))),
        timezone_kind: Some(zone_kind.into()),
        timezone_assumed: candidate.assumed,
        parse_status: if resolved_at.is_some() {
            "resolved".into()
        } else {
            "ambiguous".into()
        },
        resolved_at,
        precision: if resolved_at.is_none() {
            "ambiguous".into()
        } else if candidate.assumed {
            "assumed".into()
        } else {
            "exact".into()
        },
        claim_kind: "unknown".into(),
    }
}

fn resolve_date(
    candidate: &Candidate,
    published_date: NaiveDate,
    published_time: NaiveTime,
    announced_time: NaiveTime,
) -> Option<NaiveDate> {
    if let Some((month, day)) = candidate.explicit_date {
        let current = NaiveDate::from_ymd_opt(published_date.year(), month, day)?;
        return Some(if current < published_date {
            NaiveDate::from_ymd_opt(published_date.year() + 1, month, day)?
        } else {
            current
        });
    }
    if let Some(relation) = candidate.relation.as_deref() {
        return match relation {
            "today" => {
                // "midnight today" / "by midnight today" after local midnight already started
                // means the upcoming midnight, not 00:00 of the current local date.
                if announced_time == NaiveTime::from_hms_opt(0, 0, 0).expect("midnight")
                    && announced_time <= published_time
                {
                    published_date.checked_add_signed(Duration::days(1))
                } else {
                    Some(published_date)
                }
            }
            // tonight + midnight is the upcoming midnight, not 00:00 of the already-started local day.
            "tonight" => {
                if announced_time <= published_time {
                    published_date.checked_add_signed(Duration::days(1))
                } else {
                    Some(published_date)
                }
            }
            "tomorrow" => published_date.checked_add_signed(Duration::days(1)),
            _ => None,
        };
    }
    if let Some(weekday) = candidate.weekday {
        let mut days = (weekday.num_days_from_monday() as i64
            - published_date.weekday().num_days_from_monday() as i64)
            .rem_euclid(7);
        if days == 0 && announced_time < published_time {
            days = 7;
        }
        return published_date.checked_add_signed(Duration::days(days));
    }
    // 单独的 midnight（无 today/tonight）按即将到来的午夜理解。
    if announced_time == NaiveTime::from_hms_opt(0, 0, 0).expect("midnight") {
        return if announced_time <= published_time {
            published_date.checked_add_signed(Duration::days(1))
        } else {
            Some(published_date)
        };
    }
    // 没有日期表达时仅在钟点尚未过去的情况下按发帖当地同日解析；
    // 钟点已过去时无法确定是在回顾还是预告次日，主动降级。
    (announced_time >= published_time).then_some(published_date)
}

fn timestamp_in_fixed(ms: i64, hours: i32) -> Option<NaiveDateTime> {
    let offset = FixedOffset::east_opt(hours * 3600)?;
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|value| value.with_timezone(&offset).naive_local())
}

fn fixed_local_timestamp(naive: NaiveDateTime, hours: i32) -> Option<i64> {
    let offset = FixedOffset::east_opt(hours * 3600)?;
    offset
        .from_local_datetime(&naive)
        .single()
        .map(|value| value.timestamp_millis())
}

fn zone_from(raw: Option<&str>) -> (ZoneSpec, bool) {
    match raw.map(|value| value.to_ascii_lowercase()) {
        Some(value) if value == "pst" => (ZoneSpec::Pst, false),
        Some(value) if value == "pdt" => (ZoneSpec::Pdt, false),
        Some(_) => (ZoneSpec::Pacific, false),
        None => (ZoneSpec::Pacific, true),
    }
}

fn parse_weekday(raw: &str) -> Option<Weekday> {
    match raw.to_ascii_lowercase().as_str() {
        "monday" => Some(Weekday::Mon),
        "tuesday" => Some(Weekday::Tue),
        "wednesday" => Some(Weekday::Wed),
        "thursday" => Some(Weekday::Thu),
        "friday" => Some(Weekday::Fri),
        "saturday" => Some(Weekday::Sat),
        "sunday" => Some(Weekday::Sun),
        _ => None,
    }
}

fn find_explicit_date(text: &str) -> Option<(u32, u32)> {
    let captures = month_date_regex().captures(text)?;
    let month = match captures.get(1)?.as_str().to_ascii_lowercase().as_str() {
        "jan" | "january" => 1,
        "feb" | "february" => 2,
        "mar" | "march" => 3,
        "apr" | "april" => 4,
        "may" => 5,
        "jun" | "june" => 6,
        "jul" | "july" => 7,
        "aug" | "august" => 8,
        "sep" | "sept" | "september" => 9,
        "oct" | "october" => 10,
        "nov" | "november" => 11,
        "dec" | "december" => 12,
        _ => return None,
    };
    let day = captures.get(2)?.as_str().parse::<u32>().ok()?;
    Some((month, day))
}

fn overlaps(_text: &str, existing: &str, next: &str) -> bool {
    existing.contains(next) || next.contains(existing)
}

fn ampm_regex() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| Regex::new(r"(?i)\b(?:(today|tonight|tomorrow)\s+(?:at\s+)?)?(?:(monday|tuesday|wednesday|thursday|friday|saturday|sunday)\s+(?:at\s+)?)?(\d{1,2})(?::(\d{2}))?\s*(a\.?m\.?|p\.?m\.?)\b(?:\s*(pst|pdt|pt|pacific(?:\s+time)?))?").expect("valid ampm regex"))
}

fn clock24_regex() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| Regex::new(r"(?i)\b(?:(today|tonight|tomorrow)\s+(?:at\s+)?)?(?:(monday|tuesday|wednesday|thursday|friday|saturday|sunday)\s+(?:at\s+)?)?([01]?\d|2[0-3]):([0-5]\d)\b(?:\s*(pst|pdt|pt|pacific(?:\s+time)?))?").expect("valid 24h regex"))
}

fn push_midnight_relations(text: &str, explicit_date: Option<(u32, u32)>, candidates: &mut Vec<Candidate>) {
    for captures in tonight_regex().captures_iter(text) {
        let Some(full) = captures.get(0) else {
            continue;
        };
        if candidates
            .iter()
            .any(|candidate| overlaps(text, &candidate.raw, full.as_str()))
        {
            continue;
        }
        candidates.push(Candidate {
            raw: full.as_str().to_string(),
            hour: Some(0),
            minute: Some(0),
            relation: Some("tonight".into()),
            weekday: None,
            explicit_date,
            zone: ZoneSpec::Pacific,
            assumed: true,
            ambiguous_clock: false,
        });
    }
    for captures in midnight_zh_regex().captures_iter(text) {
        let Some(full) = captures.get(0) else {
            continue;
        };
        if candidates
            .iter()
            .any(|candidate| overlaps(text, &candidate.raw, full.as_str()))
        {
            continue;
        }
        let prefix = captures.get(1).map(|value| value.as_str()).unwrap_or("");
        let relation = match prefix {
            "明天" | "明日" => Some("tomorrow".into()),
            "今天" | "今晚" | "" => Some("tonight".into()),
            _ => Some("tonight".into()),
        };
        candidates.push(Candidate {
            raw: full.as_str().to_string(),
            hour: Some(0),
            minute: Some(0),
            relation,
            weekday: None,
            explicit_date,
            zone: ZoneSpec::Pacific,
            assumed: true,
            ambiguous_clock: false,
        });
    }
}

fn tonight_regex() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| {
        Regex::new(r"(?i)\btonight\b").expect("valid tonight regex")
    })
}

fn midnight_zh_regex() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| {
        Regex::new(r"(今天|今晚|明天|明日)?午夜").expect("valid zh midnight regex")
    })
}

fn named_clock_regex() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| Regex::new(r"(?i)\b(?:(today|tonight|tomorrow)\s+(?:at\s+)?)?(?:(monday|tuesday|wednesday|thursday|friday|saturday|sunday)\s+(?:at\s+)?)?(noon|midnight)\b(?:\s*(pst|pdt|pt|pacific(?:\s+time)?))?").expect("valid named clock regex"))
}

fn month_date_regex() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| Regex::new(r"(?i)\b(jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|jun(?:e)?|jul(?:y)?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)\s+(\d{1,2})\b").expect("valid month date regex"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn unicode_context_boundaries_do_not_panic() {
        for prefix in ["🔥", "中文", "We’re"] {
            for padding in 0..100 {
                let text = format!("{}{} reset lands at 8pm PT {}{}", prefix, "a".repeat(padding), "b".repeat(padding), prefix);
                let claims = super::parse_post_time_claims("unicode", &text, 1788809097000);
                assert!(!claims.is_empty());
            }
        }
    }
    use super::*;

    fn ts(value: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(value)
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn explicit_pst_and_pdt_differ_in_summer() {
        let posted = ts("2026-08-30T11:24:00-08:00");
        let pst = parse_post_time_claims("p", "tomorrow at 6pm PST", posted);
        let pdt = parse_post_time_claims("p", "tomorrow at 6pm PDT", posted);
        assert_eq!(pst[0].resolved_at, Some(ts("2026-09-01T10:00:00+08:00")));
        assert_eq!(pdt[0].resolved_at, Some(ts("2026-09-01T09:00:00+08:00")));
    }

    #[test]
    fn unlabelled_time_uses_pacific_dst_and_marks_assumption() {
        // 北京 08-31 03:24 = 太平洋 08-30 12:24（PDT）；同日 18:00 → 北京 08-31 09:00。
        let posted = ts("2026-08-31T03:24:00+08:00");
        let claims = parse_post_time_claims("p", "reset will land at 6pm", posted);
        assert_eq!(claims[0].resolved_at, Some(ts("2026-08-31T09:00:00+08:00")));
        assert!(claims[0].timezone_assumed);
        assert_eq!(claims[0].precision, "assumed");
    }

    #[test]
    fn parses_minutes_and_rejects_ambiguous_clock() {
        // tomorrow 相对发帖当地日（太平洋 08-30）= 08-31，17:30 PDT → 北京 09-01 08:30。
        let posted = ts("2026-08-31T03:24:00+08:00");
        let exact = parse_post_time_claims("p", "tomorrow at 17:30 PT", posted);
        assert_eq!(exact[0].resolved_at, Some(ts("2026-09-01T08:30:00+08:00")));
        let ambiguous = parse_post_time_claims("p", "tomorrow at 6:30", posted);
        assert_eq!(ambiguous[0].parse_status, "ambiguous");
        assert_eq!(ambiguous[0].resolved_at, None);
    }

    #[test]
    fn pst_early_hours_do_not_force_next_beijing_day() {
        let posted = ts("2026-01-10T00:00:00-08:00");
        let claims = parse_post_time_claims("p", "today at 6am PST", posted);
        assert_eq!(claims[0].resolved_at, Some(ts("2026-01-10T22:00:00+08:00")));
    }

    #[test]
    fn tonight_reset_converts_author_midnight_to_beijing_afternoon() {
        // Tibo 发帖时太平洋为 09-11 20:20；tonight = 当地即将到来的午夜 09-12 00:00 PDT = 北京 09-12 15:00。
        let posted = ts("2026-09-12T11:20:00+08:00");
        let en = parse_post_time_claims(
            "p",
            "Of course, there will also be a reset tonight.",
            posted,
        );
        assert_eq!(en[0].claim_kind, "grant");
        assert_eq!(en[0].resolved_at, Some(ts("2026-09-12T15:00:00+08:00")));
        let zh = parse_post_time_claims("p", "今天午夜也会进行一次重置", posted);
        assert_eq!(zh[0].claim_kind, "grant");
        assert_eq!(zh[0].resolved_at, Some(ts("2026-09-12T15:00:00+08:00")));
        let both = parse_post_time_claims_with_translation(
            "p",
            "Of course, there will also be a reset tonight.",
            Some("今天午夜也会重置"),
            posted,
        );
        assert!(both.iter().any(|claim| claim.resolved_at == Some(ts("2026-09-12T15:00:00+08:00"))));
        let actual = parse_post_time_claims(
            "p",
            "And of course, a reset is also landing by midnight today.",
            posted,
        );
        assert_eq!(actual[0].claim_kind, "grant");
        assert_eq!(actual[0].resolved_at, Some(ts("2026-09-12T15:00:00+08:00")));
    }
}

#[cfg(test)]
mod regression {
    use super::*;
    #[test]
    fn actual_announcement_pst_today_and_approximate() {
        let claims=parse_post_time_claims("2097043464538264003","Lands around 6pm PST today.",1788809097000);
        assert_eq!(claims[0].resolved_at,Some(1788832800000));
        assert_eq!(claims[0].precision,"approximate");
        assert_eq!(claims[0].claim_kind,"grant");
        let tomorrow=parse_post_time_claims("id","Lands around 6pm PST tomorrow.",1788809097000);
        assert_eq!(tomorrow[0].resolved_at,Some(1788832800000+86400000));
        assert!(parse_post_time_claims("id","6pm PST today",0).is_empty());
        let zh = parse_post_time_claims("id", "重置大约 6pm PST 左右", 1788809097000);
        assert_eq!(zh[0].precision, "approximate");
    }
}
