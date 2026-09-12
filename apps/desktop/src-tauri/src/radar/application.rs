//! Validated material outcomes and atomic event application (no network access).
use super::*;
use rusqlite::{params, OptionalExtension};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PostOutcome {
    pub post_id: String,
    pub outcome: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EventUpdate {
    pub event_type: String,
    #[serde(default)] pub event_id: Option<String>,
    pub operation: String,
    pub phase: String,
    pub signal_level: String,
    pub citations: Vec<String>,
    #[serde(default)] pub time_post_id: Option<String>,
    #[serde(default)] pub time_raw: Option<String>,
    #[serde(default)] pub clear_time: bool,
    pub conclusion: String,
}

fn resolve(inputs: &DeltaInputs, reference: &str) -> Result<String, String> {
    inputs.aliases.iter().find(|(id, alias)| id.as_str() == reference || alias.as_str() == reference)
        .map(|(id, _)| id.clone()).ok_or_else(|| format!("分析引用不存在：{reference}"))
}

pub(super) fn validate(parsed: &mut ModelJson, inputs: &DeltaInputs) -> Result<(), String> {
    if parsed.post_outcomes.is_empty() { return Err("分析缺少逐帖处理结果，材料保留待处理".into()); }
    let new_ids: HashSet<_> = inputs.delta.iter().map(|p| p.id.as_str()).collect();
    let mut outcomes = HashMap::new();
    for outcome in &mut parsed.post_outcomes {
        outcome.post_id = resolve(inputs, &outcome.post_id)?;
        if !new_ids.contains(outcome.post_id.as_str()) || outcomes.insert(outcome.post_id.clone(), outcome.outcome.clone()).is_some()
            || !matches!(outcome.outcome.as_str(), "signal" | "no_signal" | "retry") {
            return Err("逐帖处理结果包含重复、非新增引用或非法状态".into());
        }
    }
    let mut referenced = HashSet::new();
    let mut types = HashSet::new();
    for update in &mut parsed.event_updates {
        if !matches!(update.event_type.as_str(), "quota_reset" | "banked_reset")
            || !matches!(update.operation.as_str(), "create" | "reinforce" | "weaken" | "advance_phase" | "cancel" | "update_time")
            || !matches!(update.phase.as_str(), "watching" | "upcoming" | "landed_claimed")
            || !matches!(update.signal_level.as_str(), "none" | "weak" | "strong")
            || !types.insert(update.event_type.clone()) {
            return Err("事件更新包含非法枚举或同类型冲突更新".into());
        }
        update.citations = update.citations.iter().map(|id| resolve(inputs, id)).collect::<Result<_,_>>()?;
        let cited_new: Vec<_> = update.citations.iter().filter(|id| new_ids.contains(id.as_str())).collect();
        if cited_new.is_empty() { return Err("事件更新必须引用真实新增材料".into()); }
        let negative = matches!(update.operation.as_str(), "cancel" | "weaken");
        for id in cited_new {
            let outcome = outcomes.get(id).map(String::as_str);
            if negative {
                if !matches!(outcome, Some("signal" | "no_signal")) {
                    return Err("撤回或减弱引用了不可用的材料".into());
                }
            } else if outcome != Some("signal") {
                return Err("事件引用与逐帖结果冲突".into());
            }
            if outcome == Some("signal") {
                referenced.insert(id.clone());
            }
        }
        if update.conclusion.trim().is_empty() { return Err("事件更新缺少结论".into()); }
        if update.clear_time && (update.time_post_id.is_some() || update.time_raw.is_some()) { return Err("撤销时间与设置时间不能同时出现".into()); }
        if update.time_raw.is_some() != update.time_post_id.is_some() { return Err("时间依据必须包含原帖与原声明".into()); }
        if let Some(id) = &update.time_post_id {
            let id = resolve(inputs, id)?;
            if !update.citations.contains(&id) { return Err("时间依据必须属于事件引用".into()); }
            if !inputs.time_claims.iter().any(|c| c.post_id == id && Some(&c.raw_text) == update.time_raw.as_ref() && c.claim_kind == "grant" && c.resolved_at.is_some()) {
                return Err("时间声明无效、语义不明或并非预告时间".into());
            }
            update.time_post_id = Some(id);
        }
    }
    if outcomes.iter().any(|(id, outcome)| outcome == "signal" && !referenced.contains(id)) {
        return Err("有效信号缺少事件更新，保留待处理".into());
    }
    Ok(())
}

pub(super) fn commit(database: &Database, inputs: &DeltaInputs, parsed: &ModelJson,
    record: &RadarAnalysisRecord, input_json: &str) -> Result<(), String> {
    database.atomic(|db| {
        let already: bool = db.connect()?.query_row("SELECT EXISTS(SELECT 1 FROM radar_analysis_inputs WHERE analysis_id=?1)", [&record.id], |r|r.get(0)).map_err(|e|e.to_string())?;
        if already {
            for outcome in &parsed.post_outcomes {
                if outcome.outcome != "retry" {
                    db.mark_tibo_posts_consumed(&[outcome.post_id.clone()], epoch_ms())?;
                }
            }
            return Ok(());
        }
        let current = db.active_radar_events()?;
        let expected: Vec<(String,i64)> = inputs.event_status.as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .and_then(|v| v.get("events").and_then(Value::as_array).cloned()).unwrap_or_default()
            .iter().filter_map(|v| Some((v.get("event_id")?.as_str()?.into(),v.get("state_revision")?.as_i64()?))).collect();
        if current.len() != expected.len() || current.iter().any(|e| !expected.contains(&(e.id.clone(),e.state_revision))) {
            return Err("事件已变化，请重新分析当前材料".into());
        }
        for post in &inputs.delta {
            let stored = db.tibo_posts_by_ids(&[post.id.clone()])?.into_iter().next().ok_or("分析材料已不存在")?;
            let stored = to_view(stored);
            if stored.text != post.text || stored.context != post.context || stored.posted_at != post.posted_at {
                return Err("分析期间原帖发生修订，请重试".into());
            }
        }
        let mut applied = HashMap::<String,(Option<String>,String)>::new();
        let mut event_ids = Vec::new();
        for update in &parsed.event_updates {
            if let Some(id) = &update.event_id {
                if !current.iter().any(|e| &e.id == id && e.event_type == update.event_type) {
                    return Err("事件更新目标已失效".into());
                }
            }
            let latest_cited = inputs.delta.iter().filter(|p|update.citations.contains(&p.id)).map(|p|p.posted_at).max().unwrap_or(0);
            if current.iter().any(|event|event.event_type==update.event_type && latest_cited < event.latest_evidence_at) {
                for id in &update.citations { applied.insert(id.clone(),(None,"superseded_evidence".into())); }
                continue;
            }
            let selected = inputs.time_claims.iter().find(|c| Some(&c.post_id)==update.time_post_id.as_ref() && Some(&c.raw_text)==update.time_raw.as_ref());
            let mut selected_inputs = inputs.clone();
            selected_inputs.time_claims = selected.cloned().into_iter().collect();
            let item = ModelJson {
                conclusion: Some(update.conclusion.clone()), analysis_basis: parsed.analysis_basis.clone(),
                signal_type: Some(update.event_type.clone()), signal_level: Some(update.signal_level.clone()),
                event_relation: Some(if update.operation == "create" { "new_event" } else { "same_event" }.into()),
                event_phase: Some(update.phase.clone()), delta_effect: Some(update.operation.clone()),
                citations: update.citations.clone(), expected_time_post: update.time_post_id.clone(),
                ..ModelJson::default()
            };
            let (event_id, historical) = apply_analysis_to_event(db, &selected_inputs, &item, &record.id)?;
            if event_id.is_none() && !historical {
                if matches!(update.operation.as_str(), "cancel" | "weaken") {
                    for id in &update.citations {
                        applied.insert(id.clone(), (None, "no_active_event".into()));
                    }
                    continue;
                }
                return Err("有效信号未能应用到事件，材料保留待处理".into());
            }
            if let Some(id) = &event_id {
                if let Some(mut event) = db.radar_event(id)? {
                    let stale_time = selected.is_some_and(|claim| announcement_post_is_stale(db, &event, &claim.post_id));
                    if event.closed_at.is_none() && event.observed_reset_at.is_none() && (update.clear_time || selected.is_some()) && !stale_time && matches!(update.operation.as_str(), "create" | "reinforce" | "update_time" | "advance_phase" | "weaken") {
                        event.expected_at = if update.clear_time { None } else { selected.and_then(|c|c.resolved_at) };
                        event.expires_at = event_expiry(&event);
                        event.state_revision += 1;
                        db.update_radar_event(&event)?;
                        db.connect()?.execute("INSERT INTO radar_event_time_basis VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(event_id) DO UPDATE SET post_id=excluded.post_id,raw_text=excluded.raw_text,precision=excluded.precision,timezone_kind=excluded.timezone_kind,timezone_assumed=excluded.timezone_assumed",
                            params![id,selected.map(|c|&c.post_id),selected.map(|c|&c.raw_text),selected.map_or("unknown",|c|c.precision.as_str()),selected.and_then(|c|c.timezone_kind.as_deref()),selected.is_some_and(|c|c.timezone_assumed)]).map_err(|e|e.to_string())?;
                    }
                }
                event_ids.push(id.clone());
                db.connect()?.execute("INSERT OR IGNORE INTO radar_analysis_events VALUES(?1,?2)", params![record.id,id]).map_err(|e|e.to_string())?;
            }
            for id in &update.citations {
                applied.insert(id.clone(), (event_id.clone(), if historical { "historical_only" } else { "applied" }.into()));
            }
        }
        let mut stored_record = record.clone();
        stored_record.event_id = event_ids.first().cloned();
        db.insert_radar_analysis(&stored_record)?;
        db.connect()?.execute("INSERT INTO radar_analysis_inputs VALUES(?1,?2,?3)",
            params![record.id,input_json,serde_json::to_string(parsed).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        for outcome in &parsed.post_outcomes {
            let (event_id,status) = applied.get(&outcome.post_id).cloned().unwrap_or((None,
                if outcome.outcome == "retry" { "retryable_failure" } else { "no_signal" }.into()));
            db.connect()?.execute("INSERT INTO radar_applications(analysis_id,post_id,material_version,event_id,outcome,reason,created_at)
                SELECT ?1,id,material_version,?3,?4,?5,?6 FROM tibo_posts WHERE id=?2",
                params![record.id,outcome.post_id,event_id,status,if status=="retryable_failure" {"model_requested_retry"} else {"validated"},epoch_ms()]).map_err(|e|e.to_string())?;
            if outcome.outcome != "retry" { db.mark_tibo_posts_consumed(&[outcome.post_id.clone()],epoch_ms())?; }
        }
        Ok(())
    })
}


pub(super) fn cache_key(input: &str, context: &str, prompt: &str, target: &ChatTarget) -> String {
    format!("{:x}",simple_hash(&format!("{input}|{context}|{prompt}|{}|{}|{PROMPT_VERSION}",target.source_id,target.model)))
}
pub(super) fn save_prepared(db: &Database, key: &str, record: &RadarAnalysisRecord, parsed: &ModelJson, input: &str) -> Result<(), String> {
    db.connect()?.execute("INSERT OR REPLACE INTO radar_prepared_analyses VALUES(?1,?2,?3,?4)",params![key,serde_json::to_string(record).map_err(|e|e.to_string())?,input,serde_json::to_string(parsed).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
    Ok(())
}
pub(super) fn replay_prepared(db: &Database, inputs: &DeltaInputs, key: &str) -> Result<bool, String> {
    let row: Option<(String,String,String)> = db.connect()?.query_row("SELECT record_json,input_json,result_json FROM radar_prepared_analyses WHERE cache_key=?1",[key],|r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|e.to_string())?;
    let Some((record,input,result)) = row else { return Ok(false); };
    let record: RadarAnalysisRecord = serde_json::from_str(&record).map_err(|e|e.to_string())?;
    let mut parsed: ModelJson = serde_json::from_str(&result).map_err(|e|e.to_string())?;
    validate(&mut parsed,inputs)?;
    // Partial or explicitly retriable outputs must get a new analysis opportunity.
    if parsed.post_outcomes.len()!=inputs.delta.len() || parsed.post_outcomes.iter().any(|p|p.outcome=="retry") { return Ok(false); }
    commit(db,inputs,&parsed,&record,&input)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Database, DeltaInputs, ModelJson, RadarAnalysisRecord) {
        let path = std::env::temp_dir().join(format!("aqm-application-{}-{}.db",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let db = Database::initialize_at(path).unwrap();
        let now = epoch_ms();
        db.connect().unwrap().execute("INSERT INTO tibo_posts(id,url,text,posted_at,kind,explicit_reset,is_reply,replies,reposts,likes,extra_json,synced_at) VALUES('2097043464538264003','https://x.com/thstottiaux/status/2097043464538264003','We will do a global reset of the usage for all paid subscriptions.',?1,'unknown',0,0,0,0,0,'{}',?1)",[now]).unwrap();
        let inputs = collect_delta_inputs(&db).unwrap();
        let mut parsed = ModelJson {conclusion:Some("已发现额度重置公告".into()), post_outcomes:vec![PostOutcome{post_id:"N1".into(),outcome:"signal".into()}],event_updates:vec![EventUpdate{event_type:"quota_reset".into(),event_id:None,operation:"create".into(),phase:"upcoming".into(),signal_level:"strong".into(),citations:vec!["N1".into()],time_post_id:None,time_raw:None,clear_time:false,conclusion:"已发现额度重置公告".into()}],..ModelJson::default()};
        validate(&mut parsed,&inputs).unwrap();
        let record = RadarAnalysisRecord{id:"analysis-test".into(),created_at:now,range_key:"3d".into(),prompt_version:PROMPT_VERSION.into(),citations_json:"[]".into(),support_json:"[]".into(),against_json:"[]".into(),uncertainty_json:"[]".into(),new_post_ids_json:"[]".into(),event_context_post_ids_json:"[]".into(),historical_post_ids_json:"[]".into(),..RadarAnalysisRecord::default()};
        (db,inputs,parsed,record)
    }
    #[test]
    fn analysis_cursor_reads_all_records_with_equal_timestamps() {
        let (db,inputs,parsed,record)=fixture();
        commit(&db,&inputs,&parsed,&record,"input").unwrap();
        let event=db.active_radar_event().unwrap().unwrap();
        for index in 0..25 {
            let next=RadarAnalysisRecord{id:format!("page-{index:03}"),event_id:Some(event.id.clone()),..record.clone()};
            db.insert_radar_analysis(&next).unwrap();
        }
        let first=list_event_analyses(&db,&event.id,None,20).unwrap();
        assert_eq!(first.items.len(),20);
        let second=list_event_analyses(&db,&event.id,first.next_cursor.as_deref(),20).unwrap();
        assert_eq!(second.items.len(),6);assert!(second.next_cursor.is_none());
        let ids:std::collections::HashSet<_>=first.items.iter().chain(&second.items).map(|a|&a.id).collect();assert_eq!(ids.len(),26);
    }

    #[test]
    fn late_material_cannot_clear_newer_time() {
        let (db,inputs,parsed,record)=fixture();
        commit(&db,&inputs,&parsed,&record,"input").unwrap();
        let mut event=db.active_radar_event().unwrap().unwrap();
        event.expected_at=Some(epoch_ms()+3600000);event.state_revision+=1;
        db.update_radar_event(&event).unwrap();
        db.connect().unwrap().execute("UPDATE tibo_posts SET posted_at=posted_at-60000",[]).unwrap();
        let inputs=collect_delta_inputs(&db).unwrap();
        let mut parsed=parsed;parsed.event_updates[0].operation="update_time".into();parsed.event_updates[0].clear_time=true;
        let record=RadarAnalysisRecord{id:"late-time".into(),..record};
        commit(&db,&inputs,&parsed,&record,"late").unwrap();
        assert_eq!(db.radar_event(&event.id).unwrap().unwrap().expected_at,event.expected_at);
    }

    #[test]
    fn older_pst_grant_cannot_overwrite_midnight_basis() {
        let (db, _, parsed, record) = fixture();
        commit(&db, &collect_delta_inputs(&db).unwrap(), &parsed, &record, "input").unwrap();
        let midnight_posted = chrono::DateTime::parse_from_rfc3339("2026-09-12T11:20:00+08:00")
            .unwrap()
            .timestamp_millis();
        let midnight_at = chrono::DateTime::parse_from_rfc3339("2026-09-12T15:00:00+08:00")
            .unwrap()
            .timestamp_millis();
        let old_at = chrono::DateTime::parse_from_rfc3339("2026-09-08T10:00:00+08:00")
            .unwrap()
            .timestamp_millis();
        db.connect().unwrap().execute(
            "INSERT INTO tibo_posts(id,url,text,posted_at,kind,explicit_reset,is_reply,replies,reposts,likes,extra_json,synced_at)
             VALUES('midnight-today','https://x.com/tibo/status/new','a reset is also landing by midnight today.',?1,'direct',1,0,0,0,0,'{}',?1)",
            [midnight_posted],
        ).unwrap();
        db.connect().unwrap().execute(
            "UPDATE tibo_posts SET posted_at=?1, text='Lands around 6pm PST today.' WHERE id='2097043464538264003'",
            [old_at - 3_600_000],
        ).unwrap();
        let mut event = db.active_radar_event().unwrap().unwrap();
        event.expected_at = Some(midnight_at);
        event.latest_evidence_at = midnight_posted;
        event.state_revision += 1;
        db.update_radar_event(&event).unwrap();
        db.connect().unwrap().execute(
            "INSERT INTO radar_event_time_basis VALUES(?1,'midnight-today','midnight','assumed','PT',1)
             ON CONFLICT(event_id) DO UPDATE SET post_id=excluded.post_id,raw_text=excluded.raw_text,precision=excluded.precision,timezone_kind=excluded.timezone_kind,timezone_assumed=excluded.timezone_assumed",
            [&event.id],
        ).unwrap();
        refresh_time_claims(&db).unwrap();
        let inputs = collect_delta_inputs(&db).unwrap();
        let claim = inputs
            .time_claims
            .iter()
            .find(|item| item.post_id == "2097043464538264003")
            .expect("old pst claim");
        let mut parsed = parsed;
        parsed.event_updates[0].operation = "reinforce".into();
        parsed.event_updates[0].time_post_id = Some(claim.post_id.clone());
        parsed.event_updates[0].time_raw = Some(claim.raw_text.clone());
        parsed.event_updates[0].citations = vec!["N1".into()];
        let record = RadarAnalysisRecord { id: "old-grant".into(), ..record };
        commit(&db, &inputs, &parsed, &record, "old").unwrap();
        let updated = db.radar_event(&event.id).unwrap().unwrap();
        assert_eq!(updated.expected_at, Some(midnight_at));
        assert_ne!(updated.expected_at, claim.resolved_at);
    }

    #[test]
    fn unclassified_announcement_atomic_and_idempotent() {
        let (db,inputs,parsed,record)=fixture();
        commit(&db,&inputs,&parsed,&record,"actual input").unwrap();
        commit(&db,&inputs,&parsed,&record,"actual input").unwrap();
        assert_eq!(db.active_radar_events().unwrap().len(),1);
        assert!(db.unconsumed_tibo_posts_since(0,100).unwrap().is_empty());
        assert!(db.latest_event_radar_analysis(&db.active_radar_event().unwrap().unwrap().id).unwrap().is_some());
    }
    #[test]
    fn failure_after_event_creation_rolls_everything_back_and_replays() {
        let (db,inputs,parsed,record)=fixture();
        save_prepared(&db,"key",&record,&parsed,"actual input").unwrap();
        db.connect().unwrap().execute_batch("CREATE TRIGGER fail_application BEFORE INSERT ON radar_applications BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        assert!(commit(&db,&inputs,&parsed,&record,"actual input").is_err());
        assert!(db.active_radar_events().unwrap().is_empty());
        assert!(db.latest_radar_analysis().unwrap().is_none());
        assert_eq!(db.unconsumed_tibo_posts_since(0,100).unwrap().len(),1);
        db.connect().unwrap().execute_batch("DROP TRIGGER fail_application").unwrap();
        assert!(replay_prepared(&db,&inputs,"key").unwrap());
        assert_eq!(db.active_radar_events().unwrap().len(),1);
    }
    #[test]
    fn rejects_fabricated_and_unapplied_signal_and_keeps_retry() {
        let (db,inputs,mut parsed,record)=fixture();
        parsed.event_updates[0].citations=vec!["invented".into()];
        assert!(validate(&mut parsed,&inputs).is_err());
        parsed.event_updates.clear();
        assert!(validate(&mut parsed,&inputs).is_err());
        parsed.post_outcomes[0].outcome="retry".into();
        validate(&mut parsed,&inputs).unwrap();
        commit(&db,&inputs,&parsed,&record,"input").unwrap();
        assert_eq!(db.unconsumed_tibo_posts_since(0,100).unwrap().len(),1);
    }
    #[test]
    fn material_revision_requeues_but_engagement_does_not() {
        let (db,inputs,parsed,record)=fixture();
        commit(&db,&inputs,&parsed,&record,"input").unwrap();
        db.connect().unwrap().execute("UPDATE tibo_posts SET likes=42",[]).unwrap();
        assert!(db.unconsumed_tibo_posts_since(0,100).unwrap().is_empty());
        db.connect().unwrap().execute("UPDATE tibo_posts SET text='Corrected announcement'",[]).unwrap();
        assert_eq!(db.unconsumed_tibo_posts_since(0,100).unwrap().len(),1);
        let version:i64=db.connect().unwrap().query_row("SELECT material_version FROM tibo_posts",[],|r|r.get(0)).unwrap();
        assert_eq!(version,2);
    }
    #[test]
    fn weaken_and_cancel_accept_no_signal_posts() {
        let (db, inputs, mut parsed, _record) = fixture();
        parsed.post_outcomes[0].outcome = "no_signal".into();
        parsed.event_updates[0].operation = "weaken".into();
        parsed.event_updates[0].signal_level = "none".into();
        assert!(validate(&mut parsed, &inputs).is_ok());
        parsed.event_updates[0].operation = "cancel".into();
        assert!(validate(&mut parsed, &inputs).is_ok());
        let _ = db;
    }

    #[test]
    fn event_revision_conflict_keeps_material_pending() {
        let (db,inputs,parsed,record)=fixture();
        commit(&db,&inputs,&parsed,&record,"input").unwrap();
        db.connect().unwrap().execute("UPDATE tibo_posts SET text='Updated reset announcement'",[]).unwrap();
        let fresh=collect_delta_inputs(&db).unwrap();
        db.connect().unwrap().execute("UPDATE radar_events SET state_revision=state_revision+1",[]).unwrap();
        let record=RadarAnalysisRecord{id:"analysis-stale".into(),..record};
        assert!(commit(&db,&fresh,&parsed,&record,"input").is_err());
        assert_eq!(db.unconsumed_tibo_posts_since(0,100).unwrap().len(),1);
    }
}
