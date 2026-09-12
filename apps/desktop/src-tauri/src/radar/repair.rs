//! Targeted recovery for validated analyses that never created events.
//! Reuses saved results; does not call the model; repeat runs stay idempotent.

use super::*;
use rusqlite::{params, OptionalExtension};

const REPAIR_BATCH: &str = "unapplied-positive-72h-v2";
const KNOWN_MISSED_POST: &str = "2097043464538264003";

#[derive(Debug, Default)]
pub struct RepairReport {
    pub recovered: Vec<String>,
    pub retried: Vec<String>,
}

pub fn repair_unapplied_signals(database: &Database, now: i64) -> Result<RepairReport, String> {
    database.atomic(|db| {
        // Correct timestamps on recovery records made by the earlier development build.
        db.connect()?.execute("UPDATE radar_analyses AS repaired SET created_at=(SELECT MIN(original.created_at) FROM radar_analyses original WHERE original.id NOT LIKE 'repair-%' AND original.input_hash=repaired.input_hash AND original.source_id IS repaired.source_id AND original.model IS repaired.model AND original.prompt_version=repaired.prompt_version)
            WHERE repaired.id LIKE 'repair-%' AND EXISTS(SELECT 1 FROM radar_analyses original WHERE original.id NOT LIKE 'repair-%' AND original.input_hash=repaired.input_hash AND original.source_id IS repaired.source_id AND original.model IS repaired.model AND original.prompt_version=repaired.prompt_version AND original.created_at<repaired.created_at)",[]).map_err(|e|e.to_string())?;
        let mut report = RepairReport::default();
        let window_start = now.saturating_sub(MONITOR_WINDOW_MS);
        let posts = pending_unapplied_posts(db, window_start)?;
        for post in posts {
            let key = format!("post:{REPAIR_BATCH}:{}", post.id);
            if repair_already_done(db, &key)? {
                continue;
            }
            match recover_one(db, &post, now) {
                Ok(Recovered::Applied(event_id)) => {
                    save_repair(db, &key, now, &format!("applied:{event_id}"))?;
                    report.recovered.push(post.id);
                }
                Ok(Recovered::Retry) => {
                    db.connect()?.execute(
                        "UPDATE tibo_posts SET lifecycle_consumed_at=NULL WHERE id=?1",
                        [&post.id],
                    ).map_err(|e| e.to_string())?;
                    save_repair(db, &key, now, "retry")?;
                    report.retried.push(post.id);
                }
                Ok(Recovered::Skip) => {
                    save_repair(db, &key, now, "skip")?;
                }
                Err(error) => return Err(error),
            }
        }
        save_repair(db, REPAIR_BATCH, now, &format!(
            "recovered={} retried={}",
            report.recovered.len(),
            report.retried.len()
        ))?;
        Ok(report)
    })
}

enum Recovered {
    Applied(String),
    Retry,
    Skip,
}

fn pending_unapplied_posts(db: &Database, window_start: i64) -> Result<Vec<TiboPostView>, String> {
    let ids = db.unapplied_tibo_post_ids_since(window_start, KNOWN_MISSED_POST)?;
    Ok(db.tibo_posts_by_ids(&ids)?.into_iter().map(to_view).collect())
}

fn recover_one(db: &Database, post: &TiboPostView, now: i64) -> Result<Recovered, String> {
    if post.posted_at <= 0 {
        return Ok(Recovered::Skip);
    }
    if already_on_closed_event(db, &post.id)? {
        return Ok(Recovered::Skip);
    }
    let inputs = repair_inputs(db, post)?;
    let candidate = load_saved_result(db, post)?;
    let record = match candidate {
        Some((_, record, _)) => Some(record),
        None => latest_positive_analysis_for(db, post)?,
    };
    let Some(record) = record else { return Ok(Recovered::Skip); };
    // Legacy recovery is intentionally limited to a fully verifiable single NEW post.
    if !verified_single_input(db, post, &record, &inputs)? { return Ok(Recovered::Retry); }
    let Some(mut parsed) = rebuild_from_analysis(&record, post) else { return Ok(Recovered::Retry); };
    let delivery: Vec<_> = inputs.time_claims.iter().filter(|c|c.claim_kind=="grant" && c.resolved_at.is_some()).collect();
    if delivery.len()==1 {
        parsed.event_updates[0].time_post_id=Some(post.id.clone());
        parsed.event_updates[0].time_raw=Some(delivery[0].raw_text.clone());
    }
    application::validate(&mut parsed,&inputs)?;
    if db.active_radar_event_of_type(&record.signal_type)?.is_some() {
        // An unlinked older announcement cannot replace a newer live event.
        return Ok(Recovered::Retry);
    }
    let expected = if delivery.len()==1 { delivery[0].resolved_at } else { None };
    let analysis_id=format!("repair-{:x}",simple_hash(&format!("{}:{}",record.id,post.id)));
    let event_id=create_radar_event(db,&record.signal_type,record.analysis_basis.as_deref(),
        record.event_phase.as_deref().unwrap_or("upcoming"),&[post],&[],expected,&analysis_id)?;
    let mut saved=record.clone();
    saved.id=analysis_id.clone();
    saved.event_id=Some(event_id.clone());
    // Keep the original model timestamp; repair time belongs to the repair/application ledger.
    saved.created_at=record.created_at;
    db.insert_radar_analysis(&saved)?;
    let normalized=serde_json::to_string(&parsed).map_err(|e|e.to_string())?;
    let reconstructed=delta_post_block(post,&inputs.time_claims,"N1");
    db.connect()?.execute("INSERT INTO radar_analysis_inputs VALUES(?1,?2,?3)",params![analysis_id,reconstructed,normalized]).map_err(|e|e.to_string())?;
    db.connect()?.execute("INSERT OR IGNORE INTO radar_analysis_events VALUES(?1,?2)",params![analysis_id,event_id]).map_err(|e|e.to_string())?;
    db.connect()?.execute("INSERT INTO radar_applications SELECT ?1,id,material_version,?2,'applied','verified_legacy_recovery',?3 FROM tibo_posts WHERE id=?4",params![analysis_id,event_id,now,post.id]).map_err(|e|e.to_string())?;
    if let Some(claim)=delivery.first().filter(|_|delivery.len()==1) {
        db.connect()?.execute("INSERT OR REPLACE INTO radar_event_time_basis VALUES(?1,?2,?3,?4,?5,?6)",params![event_id,post.id,claim.raw_text,claim.precision,claim.timezone_kind,claim.timezone_assumed]).map_err(|e|e.to_string())?;
    }
    db.mark_tibo_posts_consumed(&[post.id.clone()],now)?;
    let mut event=db.radar_event(&event_id)?.ok_or("恢复事件不存在")?;
    reconcile_one_event(db,&mut event,now)?;
    Ok(Recovered::Applied(event_id))
}

fn verified_single_input(db: &Database, post: &TiboPostView, record: &RadarAnalysisRecord, inputs: &DeltaInputs) -> Result<bool,String> {
    if json_list(&record.new_post_ids_json) != vec![post.id.clone()]
        || !json_list(&record.citations_json).contains(&post.id)
        || !matches!(record.signal_level.as_deref(),Some("weak"|"strong")) {
        return Ok(false);
    }
    let mut claims=inputs.time_claims.clone();
    if record.prompt_version=="radar-v20" {
        // v20 input used time-v1: approximate delivery was exact and unclassified.
        // Accept this compatibility reconstruction only for the audited, immutable input hash.
        if post.id!=KNOWN_MISSED_POST || record.input_hash!="383659fe1b3a59dd" { return Ok(false); }
        for c in &mut claims { if c.precision=="approximate" {c.precision="exact".into();} if c.claim_kind=="grant" {c.claim_kind="unknown".into();} }
    } else if record.prompt_version != PROMPT_VERSION { return Ok(false); }
    let version:i64=db.connect()?.query_row("SELECT material_version FROM tibo_posts WHERE id=?1",[&post.id],|r|r.get(0)).map_err(|e|e.to_string())?;
    Ok(version==1 && format!("{:x}",simple_hash(&delta_post_block(post,&claims,"N1")))==record.input_hash)
}

fn repair_inputs(db: &Database, post: &TiboPostView) -> Result<DeltaInputs, String> {
    let claims = time_claims::parse_post_time_claims(&post.id,&post.text,post.posted_at).into_iter().map(|c| RadarTimeClaimRecord {
        post_id:c.post_id,raw_text:c.raw_text,clock_hour:c.clock_hour.map(i64::from),clock_minute:c.clock_minute.map(i64::from),date_relation:c.date_relation,timezone_kind:c.timezone_kind,timezone_assumed:c.timezone_assumed,parse_status:c.parse_status,resolved_at:c.resolved_at,precision:c.precision,parser_version:time_claims::TIMEZONE_POLICY_VERSION.into(),claim_kind:c.claim_kind
    }).collect();
    let active = db.active_radar_events()?;
    let event_status = if active.is_empty() {
        None
    } else {
        Some(serde_json::json!({
            "events": active.iter().map(|record| serde_json::json!({
                "event_id": record.id,
                "event_type": record.event_type,
                "state_revision": record.state_revision,
            })).collect::<Vec<_>>()
        }).to_string())
    };
    Ok(DeltaInputs {
        mode: "live_delta",
        delta: vec![post.clone()],
        context: Vec::new(),
        historical: Vec::new(),
        aliases: std::collections::HashMap::from([(post.id.clone(), "N1".into())]),
        event_status,
        time_claims: claims,
        temporal_phase: "timeless".into(),
        valid_until: None,
        state_revision: active.iter().map(|item| item.state_revision).sum(),
    })
}

fn load_saved_result(db: &Database, post: &TiboPostView) -> Result<Option<(ModelJson, RadarAnalysisRecord, String)>, String> {
    let connection = db.connect()?;
    let mut prepared = connection.prepare(
        "SELECT record_json, input_json, result_json FROM radar_prepared_analyses",
    ).map_err(|e| e.to_string())?;
    let prepared_rows = prepared
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
        .map_err(|e| e.to_string())?;
    for item in prepared_rows {
        let (record, input, result) = item.map_err(|e| e.to_string())?;
        let (Ok(record), Ok(parsed)) = (
            serde_json::from_str::<RadarAnalysisRecord>(&record),
            serde_json::from_str::<ModelJson>(&result),
        ) else { continue; };
        if cites_post(&parsed, post) {
            return Ok(Some((parsed, record, input)));
        }
    }
    Ok(None)
}

fn cites_post(parsed: &ModelJson, post: &TiboPostView) -> bool {
    parsed.event_updates.iter().any(|update| {
        update.citations.iter().any(|id| id == &post.id)
            && matches!(update.operation.as_str(), "create" | "reinforce" | "advance_phase" | "update_time")
    }) || parsed.citations.iter().any(|id| id == &post.id)
}

fn rebuild_from_analysis(record: &RadarAnalysisRecord, post: &TiboPostView) -> Option<ModelJson> {
    let event_type = match record.signal_type.as_str() {
        "quota_reset" | "banked_reset" => record.signal_type.clone(),
        _ => return None,
    };
    if !matches!(record.event_phase.as_deref(),Some("watching"|"upcoming"|"landed_claimed")) { return None; }
    if !matches!(record.event_relation.as_deref(), Some("new_event" | "same_event")) {
        return None;
    }
    let conclusion = record.conclusion.clone().filter(|value| !value.trim().is_empty())?;
    Some(ModelJson {
        conclusion: Some(conclusion.clone()),
        analysis_basis: record.analysis_basis.clone(),
        signal_type: Some(event_type.clone()),
        signal_level: record.signal_level.clone().or(Some("strong".into())),
        event_relation: record.event_relation.clone(),
        event_phase: record.event_phase.clone().or(Some("upcoming".into())),
        citations: vec![post.id.clone()],
        post_outcomes: vec![application::PostOutcome { post_id: post.id.clone(), outcome: "signal".into() }],
        event_updates: vec![application::EventUpdate {
            event_type,
            event_id: None,
            operation: "create".into(),
            phase: record.event_phase.clone().unwrap_or_else(|| "upcoming".into()),
            signal_level: record.signal_level.clone().unwrap_or_else(|| "strong".into()),
            citations: vec![post.id.clone()],
            time_post_id: None,
            time_raw: None,
            clear_time: false,
            conclusion,
        }],
        ..ModelJson::default()
    })
}

fn latest_positive_analysis_for(db: &Database, post: &TiboPostView) -> Result<Option<RadarAnalysisRecord>, String> {
    let connection = db.connect()?;
    let id: Option<String> = connection.query_row(
        "SELECT id FROM radar_analyses
         WHERE error_message IS NULL AND signal_type IN ('quota_reset','banked_reset')
           AND EXISTS (SELECT 1 FROM json_each(citations_json) WHERE value=?1)
         ORDER BY created_at DESC LIMIT 1",
        [&post.id],
        |row| row.get(0),
    ).optional().map_err(|e| e.to_string())?;
    drop(connection);
    let Some(id) = id else { return Ok(None); };
    db.radar_analysis_by_id(&id)
}

fn already_on_closed_event(db: &Database, post_id: &str) -> Result<bool, String> {
    let found: i64 = db.connect()?.query_row(
        "SELECT COUNT(*) FROM radar_event_evidence ee JOIN radar_events ev ON ev.id=ee.event_id
         WHERE ee.post_id=?1 AND ev.closed_at IS NOT NULL",
        [post_id],
        |row| row.get(0),
    ).map_err(|e| e.to_string())?;
    Ok(found > 0)
}

fn repair_already_done(db: &Database, key: &str) -> Result<bool, String> {
    Ok(db.connect()?.query_row(
        "SELECT EXISTS(SELECT 1 FROM radar_repairs WHERE repair_key=?1)",
        [key],
        |row| row.get(0),
    ).map_err(|e| e.to_string())?)
}

fn save_repair(db: &Database, key: &str, now: i64, detail: &str) -> Result<(), String> {
    db.connect()?.execute(
        "INSERT OR REPLACE INTO radar_repairs VALUES(?1,?2,?3)",
        params![key, now, detail],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

const CONCLUSION_REPAIR_KEY: &str = "repair:damaged_conclusions:v4";
const UNSUPPORTED_CONCLUSION: &str = "官方已宣布额度全面重置，窗口已实际刷新。";
const UNSUPPORTED_BASIS: &str = "官方宣布全员额度重置，后续补充提及中途刷新两次；确认窗口已实际刷新，非未来重置卡。";
const UNKNOWN_LEGACY_TEXT: &str = "历史文案原始依据缺失，请以事件状态与原帖为准。";

/// Undo only the fixed factual statements written by v1/v2. Never infer landing
/// from signal strength, and leave the event/evidence/material lifecycle untouched.
/// If top-level conclusion was historically truncated into a bare post-label list,
/// restore the genuine business conclusion preserved in event_updates.
pub fn repair_damaged_conclusions(database: &Database, now: i64) -> Result<(), String> {
    if repair_already_done(database, CONCLUSION_REPAIR_KEY)? { return Ok(()); }
    database.atomic(|db| {
        {
            let conn = db.connect()?;
            let mut stmt = conn.prepare(
                "SELECT id, conclusion, analysis_basis FROM radar_analyses
                 WHERE conclusion=?1 OR analysis_basis=?2
                    OR conclusion LIKE '%最新帖%'
                    OR conclusion = '12:05 动态、12:07 动态'
                    OR (conclusion LIKE '%12:05%' AND conclusion LIKE '%12:07%')"
            ).map_err(|e|e.to_string())?;
            let rows = stmt.query_map(params![UNSUPPORTED_CONCLUSION, UNSUPPORTED_BASIS], |r| {
                Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,Option<String>>(2)?))
            }).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
            for (id, before_conclusion, before_basis) in rows {
                let original: Option<String> = conn.query_row(
                    "SELECT result_json FROM radar_analysis_inputs WHERE analysis_id=?1
                     UNION ALL SELECT result_json FROM radar_prepared_analyses WHERE json_extract(record_json,'$.id')=?1 LIMIT 1", [&id], |r|r.get(0)
                ).optional().map_err(|e|e.to_string())?;
                let original = original.and_then(|raw|serde_json::from_str::<Value>(&raw).ok());
                let restore = |key: &str| -> String {
                    if let Some(v) = &original {
                        if key == "conclusion" {
                            // 优先读取 event_updates 中的业务结论（如模型输出的 "对所有付费用户执行全球额度重置"）
                            if let Some(updates) = v.get("event_updates").and_then(Value::as_array) {
                                for u in updates {
                                    if let Some(c) = u.get("conclusion").and_then(Value::as_str) {
                                        let trimmed = c.trim();
                                        if !trimmed.is_empty() && !trimmed.ends_with("最新帖") && !trimmed.ends_with("最新帖子") {
                                            return trimmed.to_string();
                                        }
                                    }
                                }
                            }
                            if let Some(c) = v.get("conclusion").and_then(Value::as_str) {
                                let trimmed = c.trim();
                                if !trimmed.is_empty() && !trimmed.ends_with("最新帖") && !trimmed.ends_with("最新帖子") && !trimmed.ends_with("最新帖子/") {
                                    return trimmed.to_string();
                                }
                            }
                        } else if let Some(s) = v.get(key).and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
                            return s.to_string();
                        }
                    }
                    UNKNOWN_LEGACY_TEXT.to_string()
                };
                let is_truncated_conclusion = before_conclusion.as_deref().is_some_and(|c| {
                    c.contains("最新帖")
                        || c == "12:05 动态、12:07 动态"
                        || (c.contains("12:05") && c.contains("12:07") && !c.contains("重置") && !c.contains("刷新"))
                });
                let conclusion = if before_conclusion.as_deref()==Some(UNSUPPORTED_CONCLUSION) || is_truncated_conclusion {
                    Some(restore("conclusion"))
                } else { before_conclusion.clone() };
                let basis = if before_basis.as_deref()==Some(UNSUPPORTED_BASIS) {
                    Some(restore("analysis_basis"))
                } else { before_basis.clone() };
                // Preserve a reversible per-row audit trail before changing display text.
                conn.execute("INSERT OR IGNORE INTO radar_repairs VALUES(?1,?2,?3)", params![
                    format!("{CONCLUSION_REPAIR_KEY}:{id}"), now,
                    json!({"analysisId":id,"beforeConclusion":before_conclusion,"beforeBasis":before_basis,
                           "afterConclusion":conclusion,"afterBasis":basis,"originalAvailable":original.is_some()}).to_string()
                ]).map_err(|e|e.to_string())?;
                conn.execute("UPDATE radar_analyses SET conclusion=?2,analysis_basis=?3 WHERE id=?1",
                    params![id,conclusion,basis]).map_err(|e|e.to_string())?;
            }
        }
        save_repair(db, CONCLUSION_REPAIR_KEY, now, "restored original wording; recovered truncated conclusions")?;
        Ok(())
    })
}

/// 活动事件的预告时间已过，但最新证据帖给出了新的发放时间（或完全没有时间）时，
/// 用最新证据覆盖过期时钟。不会给 expected_at 为空的事件填时间。
pub fn repair_stale_announced_time(database: &Database, now: i64) -> Result<(), String> {
    let events = database.active_radar_events()?;
    for mut event in events {
        if event.observed_reset_at.is_some() || event.phase == "closed" {
            continue;
        }
        let Some(old_at) = event.expected_at else {
            continue;
        };
        if now < old_at {
            continue;
        }
        let ids = database.radar_event_post_ids(&event.id)?;
        if ids.is_empty() {
            continue;
        }
        let posts = database.tibo_posts_by_ids(&ids)?;
        let Some(latest) = posts.into_iter().max_by_key(|post| post.posted_at) else {
            continue;
        };
        if latest.posted_at <= old_at {
            continue;
        }
        let claims = time_claims::parse_post_time_claims(&latest.id, &latest.text, latest.posted_at);
        let grants: std::collections::HashSet<i64> = claims
            .iter()
            .filter(|claim| claim.claim_kind == "grant")
            .filter_map(|claim| claim.resolved_at)
            .collect();
        let next = if grants.len() == 1 {
            grants.into_iter().next()
        } else {
            None
        };
        if next == event.expected_at {
            continue;
        }
        event.expected_at = next;
        event.expires_at = event_expiry(&event);
        event.state_revision += 1;
        database.update_radar_event(&event)?;
        if let Some(claim) = claims.iter().find(|item| item.resolved_at == next && item.claim_kind == "grant") {
            database.connect()?.execute(
                "INSERT INTO radar_event_time_basis VALUES(?1,?2,?3,?4,?5,?6)
                 ON CONFLICT(event_id) DO UPDATE SET post_id=excluded.post_id,raw_text=excluded.raw_text,precision=excluded.precision,timezone_kind=excluded.timezone_kind,timezone_assumed=excluded.timezone_assumed",
                rusqlite::params![
                    event.id,
                    claim.post_id,
                    claim.raw_text,
                    claim.precision,
                    claim.timezone_kind,
                    claim.timezone_assumed,
                ],
            ).map_err(|e| e.to_string())?;
        } else {
            database
                .connect()?
                .execute("DELETE FROM radar_event_time_basis WHERE event_id=?1", [&event.id])
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Explicit opt-in maintenance entry; never reads user data in ordinary test runs.
    #[test]
    #[ignore]
    fn repair_explicit_database() {
        let path=std::env::var("AQM_REPAIR_DB").expect("explicit database path required");
        let db=Database::initialize_at(path.into()).unwrap();
        let now=std::env::var("AQM_REPAIR_NOW").ok().and_then(|v|v.parse().ok()).unwrap_or_else(epoch_ms);
        let report=repair_unapplied_signals(&db,now).unwrap();
        println!("repair recovered={:?} retried={:?}",report.recovered,report.retried);
        for event in db.radar_events_recent(10).unwrap() {
            if db.radar_event_post_ids(&event.id).unwrap().iter().any(|id|id==KNOWN_MISSED_POST) {
                println!("target event={} phase={} expected={:?} closed={:?}",event.id,event.phase,event.expected_at,event.closed_at);
            }
        }
        assert!(repair_unapplied_signals(&db,now).unwrap().recovered.is_empty());
    }

    /// 2026-09-08 08:40 Beijing — the fixed screenshot moment.
    const TEST_NOW: i64 = 1_788_828_000_000;
    /// Lands around 6pm PST today → 2026-09-08 10:00 Beijing.
    const POSTED_AT: i64 = 1_788_809_097_000;
    const EXPECTED_AT: i64 = 1_788_832_800_000;

    fn fixture() -> Database {
        let path = std::env::temp_dir().join(format!(
            "aqm-repair-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        Database::initialize_at(path).unwrap()
    }

    #[test]
    fn legacy_row_without_prepared_input_recovers_and_invalid_hash_retries() {
        for valid in [true,false] {
            let db=fixture();
            db.connect().unwrap().execute("INSERT INTO tibo_posts(id,url,text,posted_at,kind,explicit_reset,is_reply,replies,reposts,likes,extra_json,synced_at,lifecycle_consumed_at) VALUES('2097043464538264003','https://x.com/thstottiaux/status/2097043464538264003','We will do a global reset. Lands around 6pm PST today.',?1,'unknown',0,0,0,0,0,'{}',?1,?1)",[POSTED_AT]).unwrap();
            let post=to_view(db.tibo_posts_by_ids(&[KNOWN_MISSED_POST.into()]).unwrap().remove(0));
            let inputs=repair_inputs(&db,&post).unwrap();
            let record=RadarAnalysisRecord{id:"legacy".into(),created_at:POSTED_AT,prompt_version:PROMPT_VERSION.into(),input_hash:if valid {format!("{:x}",simple_hash(&delta_post_block(&post,&inputs.time_claims,"N1")))} else {"wrong".into()},signal_type:"quota_reset".into(),signal_level:Some("strong".into()),event_relation:Some("same_event".into()),event_phase:Some("upcoming".into()),conclusion:Some("额度重置公告".into()),citations_json:format!("[\"{KNOWN_MISSED_POST}\"]"),new_post_ids_json:format!("[\"{KNOWN_MISSED_POST}\"]"),support_json:"[]".into(),against_json:"[]".into(),uncertainty_json:"[]".into(),event_context_post_ids_json:"[]".into(),historical_post_ids_json:"[]".into(),..RadarAnalysisRecord::default()};
            db.insert_radar_analysis(&record).unwrap();
            let result=repair_unapplied_signals(&db,TEST_NOW).unwrap();
            assert_eq!(result.recovered.len(),usize::from(valid));
            assert_eq!(result.retried.len(),usize::from(!valid));
            if valid {assert_eq!(db.active_radar_event().unwrap().unwrap().expected_at,Some(EXPECTED_AT));}
        }
    }

    #[test]
    fn stale_passed_forecast_is_replaced_by_latest_evidence_tonight() {
        let db = fixture();
        let posted = chrono::DateTime::parse_from_rfc3339("2026-09-12T11:20:00+08:00")
            .unwrap()
            .timestamp_millis();
        let now = posted + 60_000;
        let old_at = chrono::DateTime::parse_from_rfc3339("2026-09-11T15:00:00+08:00")
            .unwrap()
            .timestamp_millis();
        db.connect().unwrap().execute(
            "INSERT INTO tibo_posts(id,url,text,posted_at,kind,explicit_reset,is_reply,replies,reposts,likes,extra_json,synced_at,lifecycle_consumed_at)
             VALUES('tonight-post','https://x.com/tibo/status/1','Of course, there will also be a reset tonight.',?1,'direct',1,0,0,0,0,'{}',?1,?1)",
            [posted],
        ).unwrap();
        let mut event = RadarEventRecord {
            id: "event-stale".into(),
            phase: "upcoming".into(),
            title: "预计即将重置".into(),
            summary: None,
            first_signal_at: old_at - 3_600_000,
            latest_evidence_at: posted,
            claimed_landed_at: None,
            observed_reset_at: None,
            closed_at: None,
            close_reason: None,
            expected_at: Some(old_at),
            expires_at: Some(old_at + 48 * 3_600_000),
            state_revision: 1,
            user_confirmed_reset_at: None,
            event_type: "quota_reset".into(),
        };
        event.expires_at = event_expiry(&event);
        db.insert_radar_event(&event).unwrap();
        db.add_radar_event_evidence("event-stale", "tonight-post", "delta", "analysis-1").unwrap();
        repair_stale_announced_time(&db, now).unwrap();
        let updated = db.radar_event("event-stale").unwrap().unwrap();
        let expected = chrono::DateTime::parse_from_rfc3339("2026-09-12T15:00:00+08:00")
            .unwrap()
            .timestamp_millis();
        assert_eq!(updated.expected_at, Some(expected));
    }

    #[test]
    fn recovers_known_announcement_without_calling_model_and_stays_idempotent() {
        let db = fixture();
        db.connect().unwrap().execute(
            "INSERT INTO tibo_posts(id,url,text,posted_at,kind,explicit_reset,is_reply,replies,reposts,likes,extra_json,synced_at,lifecycle_consumed_at)
             VALUES('2097043464538264003','https://x.com/thstottiaux/status/2097043464538264003',
             'We will do a global reset of the usage for all paid subscriptions. Lands around 6pm PST today.',
             ?1,'unknown',0,0,0,0,0,'{}',?1,?1)",
            [POSTED_AT],
        ).unwrap();
        refresh_time_claims(&db).unwrap();
        let post = to_view(db.tibo_posts_by_ids(&["2097043464538264003".into()]).unwrap().remove(0));
        let mut parsed = ModelJson {
            conclusion: Some("已发现额度重置公告".into()),
            analysis_basis: Some("原文明确预告付费订阅全局额度重置".into()),
            post_outcomes: vec![application::PostOutcome { post_id: "N1".into(), outcome: "signal".into() }],
            event_updates: vec![application::EventUpdate {
                event_type: "quota_reset".into(),
                event_id: None,
                operation: "create".into(),
                phase: "upcoming".into(),
                signal_level: "strong".into(),
                citations: vec!["N1".into()],
                time_post_id: Some("N1".into()),
                time_raw: Some("6pm PST".into()),
                clear_time: false,
                conclusion: "已发现额度重置公告".into(),
            }],
            ..ModelJson::default()
        };
        let inputs = repair_inputs(&db, &post).unwrap();
        application::validate(&mut parsed, &inputs).unwrap();
        let mut record = RadarAnalysisRecord {
            id: "analysis-missed".into(),
            created_at: POSTED_AT + 60_000,
            range_key: MONITOR_RANGE_KEY.into(),
            prompt_version: PROMPT_VERSION.into(),
            citations_json: "[]".into(),
            support_json: "[]".into(),
            against_json: "[]".into(),
            uncertainty_json: "[]".into(),
            new_post_ids_json: "[\"2097043464538264003\"]".into(),
            event_context_post_ids_json: "[]".into(),
            historical_post_ids_json: "[]".into(),
            conclusion: Some("已发现额度重置公告".into()),
            analysis_basis: Some("原文明确预告付费订阅全局额度重置".into()),
            signal_type: "quota_reset".into(),
            signal_level: Some("strong".into()),
            event_relation: Some("new_event".into()),
            event_phase: Some("upcoming".into()),
            ..RadarAnalysisRecord::default()
        };
        record.new_post_ids_json=serde_json::to_string(&vec![post.id.clone()]).unwrap();
        record.citations_json=record.new_post_ids_json.clone();
        record.input_hash=format!("{:x}",simple_hash(&delta_post_block(&post,&inputs.time_claims,"N1")));
        record.conclusion=parsed.conclusion.clone();
        record.signal_type="quota_reset".into(); record.signal_level=Some("strong".into());
        record.event_relation=Some("new_event".into()); record.event_phase=Some("upcoming".into());
        application::save_prepared(&db, "missed-key", &record, &parsed, "actual input").unwrap();

        let first = repair_unapplied_signals(&db, TEST_NOW).unwrap();
        assert_eq!(first.recovered, vec!["2097043464538264003"]);
        let event = db.active_radar_event_of_type("quota_reset").unwrap().expect("event");
        assert_eq!(event.expected_at, Some(EXPECTED_AT));
        assert!(event.closed_at.is_none());
        assert_eq!(event.phase, "upcoming");

        let second = repair_unapplied_signals(&db, TEST_NOW).unwrap();
        assert!(second.recovered.is_empty());
        assert_eq!(db.active_radar_events().unwrap().len(), 1);

        // A later unrelated analysis must not clear the recovered event.
        db.connect().unwrap().execute(
            "INSERT INTO tibo_posts(id,url,text,posted_at,kind,explicit_reset,is_reply,replies,reposts,likes,extra_json,synced_at)
             VALUES('later-chat','https://x.com/tibo/status/1','just chatting',?1,'none',0,0,0,0,0,'{}',?1)",
            [TEST_NOW],
        ).unwrap();
        reconcile_event_state(&db, TEST_NOW).unwrap();
        assert!(db.active_radar_event_of_type("quota_reset").unwrap().is_some());
    }

    #[test]
    fn later_than_deadline_archives_instead_of_pretending_new() {
        let db = fixture();
        db.connect().unwrap().execute(
            "INSERT INTO tibo_posts(id,url,text,posted_at,kind,explicit_reset,is_reply,replies,reposts,likes,extra_json,synced_at,lifecycle_consumed_at)
             VALUES('2097043464538264003','https://x.com/x/status/2097043464538264003',
             'We will do a global reset. Lands around 6pm PST today.',?1,'unknown',0,0,0,0,0,'{}',?1,?1)",
            [POSTED_AT],
        ).unwrap();
        refresh_time_claims(&db).unwrap();
        let post = to_view(db.tibo_posts_by_ids(&["2097043464538264003".into()]).unwrap().remove(0));
        let mut parsed = ModelJson {
            conclusion: Some("已发现额度重置公告".into()),
            post_outcomes: vec![application::PostOutcome { post_id: "N1".into(), outcome: "signal".into() }],
            event_updates: vec![application::EventUpdate {
                event_type: "quota_reset".into(), event_id: None, operation: "create".into(),
                phase: "upcoming".into(), signal_level: "strong".into(), citations: vec!["N1".into()],
                time_post_id: Some("N1".into()), time_raw: Some("6pm PST".into()), clear_time: false,
                conclusion: "已发现额度重置公告".into(),
            }],
            ..ModelJson::default()
        };
        let inputs = repair_inputs(&db, &post).unwrap();
        application::validate(&mut parsed, &inputs).unwrap();
        let mut record = RadarAnalysisRecord {
            id: "analysis-late".into(), created_at: POSTED_AT, range_key: MONITOR_RANGE_KEY.into(),
            prompt_version: PROMPT_VERSION.into(), citations_json: "[]".into(), support_json: "[]".into(),
            against_json: "[]".into(), uncertainty_json: "[]".into(), new_post_ids_json: "[]".into(),
            event_context_post_ids_json: "[]".into(), historical_post_ids_json: "[]".into(),
            ..RadarAnalysisRecord::default()
        };
        record.new_post_ids_json=serde_json::to_string(&vec![post.id.clone()]).unwrap();
        record.citations_json=record.new_post_ids_json.clone();
        record.input_hash=format!("{:x}",simple_hash(&delta_post_block(&post,&inputs.time_claims,"N1")));
        record.conclusion=parsed.conclusion.clone();
        record.signal_type="quota_reset".into(); record.signal_level=Some("strong".into());
        record.event_relation=Some("new_event".into()); record.event_phase=Some("upcoming".into());
        application::save_prepared(&db, "late-key", &record, &parsed, "input").unwrap();
        let far_future = EXPECTED_AT + 48 * 3_600_000;
        repair_unapplied_signals(&db, far_future).unwrap();
        let event = db.radar_events_recent(4).unwrap().into_iter().next().expect("event");
        assert!(event.closed_at.is_some());
        assert_eq!(event.close_reason.as_deref(), Some("timeout_unverified"));
        assert_ne!(event.phase, "watching");
    }

    #[test]
    fn conclusion_repair_preserves_future_announcement() {
        let db = fixture();
        let record = RadarAnalysisRecord {
            id:"future-announcement".into(), created_at:POSTED_AT,
            conclusion:Some("本次新增帖子中，官方宣布明天重置，尚未发生。".into()),
            signal_type:"quota_reset".into(),signal_level:Some("strong".into()),
            event_phase:Some("upcoming".into()),..RadarAnalysisRecord::default()
        };
        db.insert_radar_analysis(&record).unwrap();
        repair_damaged_conclusions(&db,TEST_NOW).unwrap();
        assert_eq!(db.latest_radar_analysis().unwrap().unwrap().conclusion,record.conclusion);
    }

    #[test]
    fn conclusion_repair_restores_original_and_is_idempotent() {
        let db=fixture();
        let record=RadarAnalysisRecord {id:"damaged".into(),created_at:POSTED_AT,
            conclusion:Some(UNSUPPORTED_CONCLUSION.into()),analysis_basis:Some(UNSUPPORTED_BASIS.into()),
            event_phase:Some("upcoming".into()),..RadarAnalysisRecord::default()};
        db.insert_radar_analysis(&record).unwrap();
        save_repair(&db,"repair:damaged_conclusions:v2",TEST_NOW,"done").unwrap();
        db.connect().unwrap().execute("INSERT INTO radar_analysis_inputs VALUES('damaged','{}',?1)",
            [json!({"conclusion":"官方预告明天重置，尚未发生。","analysis_basis":"只有预告，尚无落地证据。"}).to_string()]).unwrap();
        repair_damaged_conclusions(&db,TEST_NOW+1).unwrap();
        repair_damaged_conclusions(&db,TEST_NOW+2).unwrap();
        let row=db.latest_radar_analysis().unwrap().unwrap();
        assert_eq!(row.conclusion.as_deref(),Some("官方预告明天重置，尚未发生。"));
        assert_eq!(row.analysis_basis.as_deref(),Some("只有预告，尚无落地证据。"));
        assert_eq!(row.event_phase,record.event_phase);
        assert!(repair_already_done(&db,&format!("{CONCLUSION_REPAIR_KEY}:damaged")).unwrap());
    }

    #[test]
    fn conclusion_repair_recovers_event_update_conclusion_when_top_level_truncated() {
        let db = fixture();
        let record = RadarAnalysisRecord {
            id: "analysis-truncated".into(),
            created_at: POSTED_AT,
            conclusion: Some("本次新增帖子中，9月8日 12:05 的最新帖子、9月8日 12:07 的最新帖".into()),
            analysis_basis: Some("原文确认额度已刷新".into()),
            event_phase: Some("upcoming".into()),
            ..RadarAnalysisRecord::default()
        };
        db.insert_radar_analysis(&record).unwrap();
        let input_json = json!({
            "id": "analysis-truncated",
            "conclusion": "本次新增帖子中，9月8日 12:05 的最新帖子、9月8日 12:07 的最新帖",
            "analysis_basis": "原文确认额度已刷新",
            "event_updates": [{
                "conclusion": "对所有付费用户执行全球额度重置"
            }]
        });
        db.connect().unwrap().execute(
            "INSERT INTO radar_analysis_inputs VALUES('analysis-truncated','{}',?1)",
            [input_json.to_string()],
        ).unwrap();

        repair_damaged_conclusions(&db, TEST_NOW + 1).unwrap();
        let row = db.latest_radar_analysis().unwrap().unwrap();
        assert_eq!(row.conclusion.as_deref(), Some("对所有付费用户执行全球额度重置"));
        assert!(repair_already_done(&db, &format!("{CONCLUSION_REPAIR_KEY}:analysis-truncated")).unwrap());
    }

    #[test]
    fn conclusion_repair_missing_original_does_not_invent_facts() {
        let db=fixture();
        db.insert_radar_analysis(&RadarAnalysisRecord {id:"missing-original".into(),created_at:POSTED_AT,
            conclusion:Some(UNSUPPORTED_CONCLUSION.into()),..RadarAnalysisRecord::default()}).unwrap();
        save_repair(&db,"repair:damaged_conclusions:v2",TEST_NOW,"done").unwrap();
        repair_damaged_conclusions(&db,TEST_NOW+1).unwrap();
        assert_eq!(db.latest_radar_analysis().unwrap().unwrap().conclusion.as_deref(),Some(UNKNOWN_LEGACY_TEXT));
    }
}
