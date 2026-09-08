//! 阶段二业务数据仓库。所有写入均按 Source generation 防止旧请求覆盖新结果。

use crate::domain::refresh::{SourceRefreshOutput, StoredTrendPoint};
use crate::storage::database::Database;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

static RUN_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct UserPlatformRecord {
    pub platform_id: String,
    pub display_name: String,
    pub notes: String,
    pub api_base_url: Option<String>,
    #[allow(dead_code)]
    pub sort_index: i64,
}

#[derive(Debug, Clone)]
pub struct SourceRecord {
    pub id: String,
    pub account_id: String,
    pub account_name: String,
    pub account_kind: String,
    pub platform_id: String,
    pub adapter_id: String,
    pub source_type: String,
    pub display_name: String,
    pub secret_ref: Option<String>,
    pub state: String,
    pub last_validated_at: Option<i64>,
    pub last_success_at: Option<i64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RadarCustomModelRecord {
    pub source_id: String,
    pub model: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct RadarChatEndpointRecord {
    pub id: String,
    pub display_name: String,
    pub api_base_url: String,
    pub secret_ref: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct RadarChatEndpointModelRecord {
    pub endpoint_id: String,
    pub model: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct AccountRecord {
    pub id: String,
    pub platform_id: String,
    pub display_name: String,
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct SnapshotRecord {
    pub capability_id: String,
    pub display_name: String,
    pub value_kind: String,
    pub primary_value: Option<String>,
    pub secondary_value: Option<String>,
    pub progress: Option<f64>,
    pub trend: Vec<StoredTrendPoint>,
    pub captured_at: i64,
    /// 结构化窗口时长（秒）；仅额度窗口快照有值。
    pub window_seconds: Option<i64>,
    /// 结构化窗口重置时间（epoch 毫秒）。
    pub reset_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct TiboPostRecord {
    pub id: String,
    pub url: String,
    pub text: String,
    pub posted_at: i64,
    pub kind: String,
    pub tibo_lane: Option<String>,
    pub explicit_reset: bool,
    pub verification_status: Option<String>,
    pub is_reply: bool,
    pub replies: i64,
    pub reposts: i64,
    pub likes: i64,
    pub extra_json: String,
    pub synced_at: i64,
    pub translated_text: Option<String>,
    pub translated_at: Option<i64>,
    pub translation_source: Option<String>,
    /// NULL：从未成功参与生命周期实时分析；有值后只能作为上下文或历史。
    pub lifecycle_consumed_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct RadarCheckRecord {
    pub id: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub status: String,
    pub sync_status: Option<String>,
    pub parse_status: Option<String>,
    pub analyze_status: Option<String>,
    pub error_message: Option<String>,
    pub post_count: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RadarAnalysisRecord {
    pub id: String,
    pub created_at: i64,
    pub range_key: String,
    pub cut_post_id: Option<String>,
    pub from_posted_at: Option<i64>,
    pub to_posted_at: Option<i64>,
    pub source_id: Option<String>,
    pub model: Option<String>,
    pub prompt_version: String,
    pub input_hash: String,
    pub conclusion: Option<String>,
    pub analysis_basis: Option<String>,
    pub confidence: Option<String>,
    pub citations_json: String,
    pub support_json: String,
    pub against_json: String,
    pub uncertainty_json: String,
    /// 本次输入 NEW POSTS 的真实 post_id（JSON 数组）；旧记录为 "[]"。
    pub new_post_ids_json: String,
    /// 本次输入 EVENT CONTEXT POSTS 的真实 post_id（JSON 数组）。
    pub event_context_post_ids_json: String,
    /// 本次输入 HISTORICAL CONTEXT POSTS 的真实 post_id（JSON 数组）。
    pub historical_post_ids_json: String,
    pub error_message: Option<String>,
    /// 关联的重置事件；历史分析与无关动态分析为 None。
    pub event_id: Option<String>,
    /// delta | rebuild
    pub analysis_mode: Option<String>,
    pub context_hash: String,
    pub prompt_hash: String,
    /// new_event | same_event | none
    pub event_relation: Option<String>,
    /// watching | upcoming | landed_claimed | landed_observed | closed
    pub event_phase: Option<String>,
    /// reinforce | no_change | weaken | advance_phase | cancel | new_event
    pub delta_effect: Option<String>,
    /// none | weak | strong
    pub signal_level: Option<String>,
    /// complete | context_missing | conflicting
    pub context_status: Option<String>,
    pub temporal_phase: Option<String>,
    pub valid_until: Option<i64>,
    pub state_revision: i64,
    pub timezone_policy_version: String,
    /// banked_reset | quota_reset | none | unknown；旧记录为 unknown，不得伪造成新分析。
    pub signal_type: String,
}

#[derive(Debug, Clone)]
pub struct RadarEventRecord {
    pub id: String,
    /// watching | upcoming | landed_claimed | landed_observed | closed
    pub phase: String,
    pub title: String,
    pub summary: Option<String>,
    pub first_signal_at: i64,
    pub latest_evidence_at: i64,
    pub claimed_landed_at: Option<i64>,
    pub observed_reset_at: Option<i64>,
    pub closed_at: Option<i64>,
    pub close_reason: Option<String>,
    pub expected_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub state_revision: i64,
    /// 用户确认额度已重置的时间；与 observation.user_confirmed_at（重置卡归因）不是同一字段。
    pub user_confirmed_reset_at: Option<i64>,
    /// banked_reset | quota_reset；旧事件迁移为 quota_reset。
    pub event_type: String,
}

#[derive(Debug, Clone)]
pub struct RadarTimeClaimRecord {
    pub post_id: String,
    pub raw_text: String,
    pub clock_hour: Option<i64>,
    pub clock_minute: Option<i64>,
    pub date_relation: Option<String>,
    pub timezone_kind: Option<String>,
    pub timezone_assumed: bool,
    pub parse_status: String,
    pub resolved_at: Option<i64>,
    pub precision: String,
    pub parser_version: String,
    /// grant | deadline | historical | unknown：发放/预告时间、截止时间、历史时间。
    /// 预计重置时间只允许 grant/unknown 参与，截止与历史时间不得冒充预告。
    pub claim_kind: String,
}

/// 雷达来源（codexradar / willcodex）各自的同步状态；一个来源失败不影响另一来源。
#[derive(Debug, Clone)]
pub struct RadarSourceStatusRecord {
    pub source_id: String,
    pub last_check_at: Option<i64>,
    pub last_success_at: Option<i64>,
    pub last_error: Option<String>,
    pub last_post_count: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct WindowSampleRecord {
    pub id: i64,
    pub capability_id: String,
    pub display_name: String,
    pub progress: Option<f64>,
    pub captured_at: i64,
    pub window_seconds: Option<i64>,
    pub reset_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct QuotaResetObservationRecord {
    pub id: i64,
    pub account_id: String,
    pub source_id: String,
    pub capability_id: String,
    pub previous_snapshot_id: i64,
    pub current_snapshot_id: i64,
    pub classification: String,
    pub observed_at: i64,
    pub event_id: Option<String>,
    pub temporal_correlation: String,
    pub user_confirmed_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct CapabilityValueSample {
    pub id: i64,
    pub captured_at: i64,
    pub primary_value: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BankedResetObservationRecord {
    #[allow(dead_code)]
    pub id: i64,
    pub account_id: String,
    pub source_id: String,
    pub previous_snapshot_id: i64,
    pub current_snapshot_id: i64,
    pub previous_count: i64,
    pub current_count: i64,
    pub observed_at: i64,
    pub event_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RefreshHistoryRecord {
    pub id: String,
    pub source_id: String,
    pub source_name: String,
    pub account_id: String,
    pub account_name: String,
    pub status: String,
    pub finished_at: Option<i64>,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredTrendPointDto {
    label: String,
    value: String,
}

impl Database {
    pub fn list_user_platforms(&self) -> Result<Vec<UserPlatformRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT platform_id, display_name, notes, api_base_url, sort_index
                 FROM user_platforms ORDER BY sort_index, created_at, platform_id",
            )
            .map_err(|err| format!("准备已添加平台查询失败: {err}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok(UserPlatformRecord {
                    platform_id: row.get(0)?,
                    display_name: row.get(1)?,
                    notes: row.get(2)?,
                    api_base_url: row.get(3)?,
                    sort_index: row.get(4)?,
                })
            })
            .map_err(|err| format!("查询已添加平台失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取已添加平台失败: {err}"))
    }

    pub fn user_platform(&self, platform_id: &str) -> Result<Option<UserPlatformRecord>, String> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT platform_id, display_name, notes, api_base_url, sort_index FROM user_platforms WHERE platform_id = ?1",
                params![platform_id],
                |row| {
                    Ok(UserPlatformRecord {
                        platform_id: row.get(0)?,
                        display_name: row.get(1)?,
                        notes: row.get(2)?,
                        api_base_url: row.get(3)?,
                        sort_index: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(|err| format!("读取已添加平台失败: {err}"))
    }

    pub fn ensure_account_source(
        &self,
        account_id: &str,
        platform_id: &str,
        source_id: &str,
        source_type: &str,
        source_name: &str,
        account_name: &str,
    ) -> Result<(), String> {
        self.ensure_account_source_with_adapter(
            account_id,
            platform_id,
            "default",
            source_id,
            source_id,
            source_type,
            source_name,
            account_name,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn ensure_account_source_with_adapter(
        &self,
        account_id: &str,
        platform_id: &str,
        account_kind: &str,
        source_id: &str,
        adapter_id: &str,
        source_type: &str,
        source_name: &str,
        account_name: &str,
    ) -> Result<(), String> {
        let connection = self.connect()?;
        let now = epoch_ms();
        connection
            .execute(
                "INSERT OR IGNORE INTO accounts(id, platform_id, display_name, kind, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![account_id, platform_id, account_name, account_kind, now],
            )
            .map_err(|err| format!("初始化账户失败: {err}"))?;
        connection
            .execute(
                "INSERT OR IGNORE INTO sources(id, account_id, adapter_id, source_type, display_name, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                params![source_id, account_id, adapter_id, source_type, source_name, now],
            )
            .map_err(|err| format!("初始化来源失败: {err}"))?;
        Ok(())
    }

    pub fn account(&self, account_id: &str) -> Result<AccountRecord, String> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT id, platform_id, display_name, kind FROM accounts WHERE id = ?1",
                params![account_id],
                |row| {
                    Ok(AccountRecord {
                        id: row.get(0)?,
                        platform_id: row.get(1)?,
                        display_name: row.get(2)?,
                        kind: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(|err| format!("读取账户失败: {err}"))?
            .ok_or_else(|| "未找到账户".to_string())
    }

    pub fn rename_account(&self, account_id: &str, display_name: &str) -> Result<(), String> {
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "UPDATE accounts SET display_name = ?2, updated_at = ?3 WHERE id = ?1",
                params![account_id, display_name, epoch_ms()],
            )
            .map_err(|err| format!("重命名账户失败: {err}"))?;
        if changed == 0 {
            Err("未找到该账户".into())
        } else {
            Ok(())
        }
    }

    pub fn rename_source(&self, source_id: &str, display_name: &str) -> Result<(), String> {
        let source = self.source(source_id)?;
        let connection = self.connect()?;
        let now = epoch_ms();
        let renamed = connection
            .execute(
                "UPDATE sources SET display_name = ?2, updated_at = ?3 WHERE id = ?1",
                params![source_id, display_name, now],
            )
            .map_err(|err| format!("重命名来源失败: {err}"))?;
        if renamed == 0 {
            return Err("未找到该来源".into());
        }
        connection
            .execute(
                "UPDATE accounts SET display_name = ?2, updated_at = ?3 WHERE id = ?1",
                params![source.account_id, display_name, now],
            )
            .map_err(|err| format!("重命名账户失败: {err}"))?;
        Ok(())
    }

    pub fn delete_account(&self, account_id: &str) -> Result<(), String> {
        if self.account(account_id)?.kind != "additional" {
            return Err("不能删除本机或默认账户".into());
        }
        let connection = self.connect()?;
        let changed = connection
            .execute("DELETE FROM accounts WHERE id = ?1", params![account_id])
            .map_err(|err| format!("删除账户失败: {err}"))?;
        if changed == 0 {
            Err("未找到该账户".into())
        } else {
            Ok(())
        }
    }

    pub fn remove_user_platform(&self, platform_id: &str) -> Result<Vec<SourceRecord>, String> {
        if self.user_platform(platform_id)?.is_none() {
            return Err("未添加该平台".into());
        }
        let sources = self.list_sources(platform_id)?;
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction()
            .map_err(|err| format!("开始移除平台失败: {err}"))?;
        let now = epoch_ms();
        transaction
            .execute(
                "DELETE FROM refresh_runs WHERE platform_id = ?1",
                params![platform_id],
            )
            .map_err(|err| format!("清除刷新历史失败: {err}"))?;
        for source in &sources {
            transaction
                .execute(
                    "DELETE FROM capability_snapshots WHERE source_id = ?1",
                    params![source.id],
                )
                .map_err(|err| format!("清除平台快照失败: {err}"))?;
            if matches!(
                source.account_id.as_str(),
                "openai-codex-local" | "deepseek-default"
            ) {
                transaction
                    .execute(
                        "UPDATE sources SET secret_ref = NULL, state = 'auth_required', generation = generation + 1,
                         last_validated_at = NULL, last_success_at = NULL, error_code = NULL, error_message = NULL, updated_at = ?2
                         WHERE id = ?1",
                        params![source.id, now],
                    )
                    .map_err(|err| format!("重置默认来源失败: {err}"))?;
            }
        }
        transaction
            .execute(
                "DELETE FROM accounts WHERE platform_id = ?1 AND id NOT IN ('openai-codex-local', 'deepseek-default')",
                params![platform_id],
            )
            .map_err(|err| format!("删除平台账户失败: {err}"))?;
        let changed = transaction
            .execute(
                "DELETE FROM user_platforms WHERE platform_id = ?1",
                params![platform_id],
            )
            .map_err(|err| format!("移除平台失败: {err}"))?;
        if changed == 0 {
            return Err("未添加该平台".into());
        }
        transaction
            .commit()
            .map_err(|err| format!("提交移除平台失败: {err}"))?;
        Ok(sources)
    }

    pub fn add_user_platform(
        &self,
        platform_id: &str,
        display_name: &str,
        api_base_url: Option<&str>,
    ) -> Result<(), String> {
        let connection = self.connect()?;
        let now = epoch_ms();
        let next_index: i64 = connection
            .query_row(
                "SELECT COALESCE(MAX(sort_index), -1) + 1 FROM user_platforms",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        connection
            .execute(
                "INSERT INTO user_platforms(platform_id, display_name, notes, api_base_url, sort_index, created_at, updated_at)
                 VALUES (?1, ?2, '', ?3, ?4, ?5, ?5)
                 ON CONFLICT(platform_id) DO NOTHING",
                params![platform_id, display_name, api_base_url, next_index, now],
            )
            .map(|_| ())
            .map_err(|err| format!("添加平台失败: {err}"))
    }

    pub fn save_user_platform_setup(
        &self,
        platform_id: &str,
        display_name: &str,
        notes: &str,
        api_base_url: Option<&str>,
    ) -> Result<(), String> {
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "UPDATE user_platforms
                 SET display_name = ?2, notes = ?3, api_base_url = ?4, updated_at = ?5
                 WHERE platform_id = ?1",
                params![platform_id, display_name, notes, api_base_url, epoch_ms()],
            )
            .map_err(|err| format!("保存平台接入信息失败: {err}"))?;
        if changed == 0 {
            Err("未添加该平台".into())
        } else {
            Ok(())
        }
    }

    pub fn save_user_platform_api_base(
        &self,
        platform_id: &str,
        api_base_url: Option<&str>,
    ) -> Result<(), String> {
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "UPDATE user_platforms SET api_base_url = ?2, updated_at = ?3 WHERE platform_id = ?1",
                params![platform_id, api_base_url, epoch_ms()],
            )
            .map_err(|err| format!("保存 API 请求地址失败: {err}"))?;
        if changed == 0 {
            Err("未添加该平台".into())
        } else {
            Ok(())
        }
    }

    pub fn setting_bool(&self, key: &str) -> Result<bool, String> {
        let connection = self.connect()?;
        let value = connection
            .query_row(
                "SELECT value_json FROM settings WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|err| format!("读取设置失败: {err}"))?;
        Ok(value.as_deref() == Some("true"))
    }

    pub fn set_setting_bool(&self, key: &str, value: bool) -> Result<(), String> {
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO settings(key, value_json, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
                params![key, if value { "true" } else { "false" }, epoch_ms()],
            )
            .map(|_| ())
            .map_err(|err| format!("保存设置失败: {err}"))
    }

    pub fn list_sources(&self, platform_id: &str) -> Result<Vec<SourceRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT s.id, s.account_id, a.display_name, a.kind, a.platform_id, COALESCE(s.adapter_id, s.id), s.source_type, s.display_name, s.secret_ref, s.state, s.last_validated_at, s.last_success_at, s.error_code, s.error_message
                 FROM sources s JOIN accounts a ON a.id = s.account_id
                 WHERE a.platform_id = ?1
                 ORDER BY CASE a.kind WHEN 'local' THEN 0 WHEN 'default' THEN 1 ELSE 2 END, a.created_at, s.created_at, s.id",
            )
            .map_err(|err| format!("准备 Source 查询失败: {err}"))?;
        let rows = statement
            .query_map(params![platform_id], map_source)
            .map_err(|err| format!("查询 Source 失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取 Source 失败: {err}"))
    }

    pub fn source(&self, source_id: &str) -> Result<SourceRecord, String> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT s.id, s.account_id, a.display_name, a.kind, a.platform_id, COALESCE(s.adapter_id, s.id), s.source_type, s.display_name, s.secret_ref, s.state, s.last_validated_at, s.last_success_at, s.error_code, s.error_message
                 FROM sources s JOIN accounts a ON a.id = s.account_id WHERE s.id = ?1",
                params![source_id],
                map_source,
            )
            .optional()
            .map_err(|err| format!("读取 Source 失败: {err}"))?
            .ok_or_else(|| "未找到数据来源".to_string())
    }

    pub fn begin_refresh_run(
        &self,
        platform_id: &str,
        source_ids: &[String],
    ) -> Result<String, String> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction()
            .map_err(|err| format!("开始刷新流水失败: {err}"))?;
        let now = epoch_ms();
        let run_id = format!(
            "{platform_id}-{now}-{}",
            RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        transaction
            .execute(
                "INSERT INTO refresh_runs(id, platform_id, started_at, status) VALUES (?1, ?2, ?3, 'running')",
                params![run_id, platform_id, now],
            )
            .map_err(|err| format!("写入刷新流水失败: {err}"))?;
        for source_id in source_ids {
            transaction
                .execute(
                    "INSERT INTO refresh_results(run_id, source_id, started_at, status) VALUES (?1, ?2, ?3, 'running')",
                    params![run_id, source_id, now],
                )
                .map_err(|err| format!("写入 Source 刷新流水失败: {err}"))?;
        }
        transaction
            .commit()
            .map_err(|err| format!("提交刷新流水失败: {err}"))?;
        Ok(run_id)
    }

    pub fn begin_source_refresh(&self, source_id: &str) -> Result<i64, String> {
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "UPDATE sources SET generation = generation + 1, state = 'refreshing', updated_at = ?2 WHERE id = ?1",
                params![source_id, epoch_ms()],
            )
            .map_err(|err| format!("标记 Source 刷新失败: {err}"))?;
        if changed == 0 {
            return Err("未找到数据来源".into());
        }
        connection
            .query_row(
                "SELECT generation FROM sources WHERE id = ?1",
                params![source_id],
                |row| row.get(0),
            )
            .map_err(|err| format!("读取 Source generation 失败: {err}"))
    }

    pub fn complete_source_refresh(
        &self,
        run_id: &str,
        source: &SourceRecord,
        generation: i64,
        output: &SourceRefreshOutput,
    ) -> Result<bool, String> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction()
            .map_err(|err| format!("开始保存刷新结果失败: {err}"))?;
        let now = epoch_ms();
        let (state, error_code, error_message, result_status) = match &output.error {
            None => ("ready", None, None, "success"),
            Some(error) => (
                if error.auth_required {
                    "auth_required"
                } else {
                    "error"
                },
                Some(error.code.as_str()),
                Some(error.message.as_str()),
                if output.capabilities.is_empty() {
                    "failed"
                } else {
                    "partial"
                },
            ),
        };
        let changed = transaction
            .execute(
                "UPDATE sources
                 SET state = ?3, last_validated_at = ?4,
                     last_success_at = CASE WHEN ?5 = 1 THEN ?4 ELSE last_success_at END,
                     error_code = ?6, error_message = ?7, updated_at = ?4
                 WHERE id = ?1 AND generation = ?2",
                params![
                    source.id,
                    generation,
                    state,
                    now,
                    i64::from(!output.capabilities.is_empty()),
                    error_code,
                    error_message,
                ],
            )
            .map_err(|err| format!("更新 Source 状态失败: {err}"))?;
        if changed == 0 {
            transaction
                .rollback()
                .map_err(|err| format!("回滚过期刷新结果失败: {err}"))?;
            return Ok(false);
        }

        for capability in &output.capabilities {
            let trend_json = serde_json::to_string(
                &capability
                    .trend
                    .iter()
                    .map(|point| StoredTrendPointDto {
                        label: point.label.clone(),
                        value: point.value.clone(),
                    })
                    .collect::<Vec<_>>(),
            )
            .map_err(|err| format!("序列化趋势数据失败: {err}"))?;
            transaction
                .execute(
                    "INSERT INTO capability_snapshots(account_id, source_id, capability_id, display_name, value_kind, primary_value, secondary_value, progress, trend_json, captured_at, generation, window_seconds, reset_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        source.account_id,
                        source.id,
                        capability.capability_id,
                        capability.display_name,
                        capability.value_kind,
                        capability.primary_value,
                        capability.secondary_value,
                        capability.progress,
                        trend_json,
                        now,
                        generation,
                        capability.window_seconds,
                        capability.reset_at,
                    ],
                )
                .map_err(|err| format!("保存能力快照失败: {err}"))?;
        }
        transaction
            .execute(
                "UPDATE refresh_results SET finished_at = ?3, status = ?4, error_code = ?5, error_message = ?6
                 WHERE run_id = ?1 AND source_id = ?2",
                params![run_id, source.id, now, result_status, error_code, error_message],
            )
            .map_err(|err| format!("更新 Source 刷新流水失败: {err}"))?;
        transaction
            .commit()
            .map_err(|err| format!("提交 Source 刷新结果失败: {err}"))?;
        Ok(true)
    }

    pub fn finish_refresh_run(&self, run_id: &str) -> Result<(), String> {
        let connection = self.connect()?;
        let (failed, partial): (i64, i64) = connection
            .query_row(
                "SELECT SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), SUM(CASE WHEN status = 'partial' THEN 1 ELSE 0 END)
                 FROM refresh_results WHERE run_id = ?1",
                params![run_id],
                |row| Ok((row.get::<_, Option<i64>>(0)?.unwrap_or(0), row.get::<_, Option<i64>>(1)?.unwrap_or(0))),
            )
            .map_err(|err| format!("汇总刷新流水失败: {err}"))?;
        let status = if failed > 0 || partial > 0 {
            "partial"
        } else {
            "success"
        };
        connection
            .execute(
                "UPDATE refresh_runs SET finished_at = ?2, status = ?3 WHERE id = ?1",
                params![run_id, epoch_ms(), status],
            )
            .map_err(|err| format!("结束刷新流水失败: {err}"))?;
        Ok(())
    }

    pub fn save_secret_ref(&self, source_id: &str, secret_ref: &str) -> Result<(), String> {
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "UPDATE sources SET secret_ref = ?2, updated_at = ?3 WHERE id = ?1",
                params![source_id, secret_ref, epoch_ms()],
            )
            .map_err(|err| format!("保存凭据引用失败: {err}"))?;
        if changed == 0 {
            Err("未找到数据来源".into())
        } else {
            Ok(())
        }
    }

    pub fn clear_secret_ref(&self, source_id: &str) -> Result<(), String> {
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "UPDATE sources SET secret_ref = NULL, state = 'auth_required', generation = generation + 1,
                 last_validated_at = ?2, error_code = NULL, error_message = NULL, updated_at = ?2 WHERE id = ?1",
                params![source_id, epoch_ms()],
            )
            .map_err(|err| format!("清除凭据引用失败: {err}"))?;
        if changed == 0 {
            Err("未找到数据来源".into())
        } else {
            Ok(())
        }
    }

    pub fn latest_snapshot(
        &self,
        source_id: &str,
        capability_id: &str,
    ) -> Result<Option<SnapshotRecord>, String> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT capability_id, display_name, value_kind, primary_value, secondary_value, progress, trend_json, captured_at, window_seconds, reset_at
                 FROM capability_snapshots WHERE source_id = ?1 AND capability_id = ?2
                 ORDER BY captured_at DESC, id DESC LIMIT 1",
                params![source_id, capability_id],
                map_snapshot_record,
            )
            .optional()
            .map_err(|err| format!("读取能力快照失败: {err}"))
    }

    pub fn recent_capability_primary_values(
        &self,
        source_id: &str,
        capability_id: &str,
        limit: i64,
    ) -> Result<Vec<(i64, Option<String>)>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT captured_at, primary_value FROM capability_snapshots
                 WHERE source_id = ?1 AND capability_id = ?2
                 ORDER BY captured_at DESC, id DESC LIMIT ?3",
            )
            .map_err(|err| format!("准备能力历史查询失败: {err}"))?;
        let rows = statement
            .query_map(params![source_id, capability_id, limit], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|err| format!("读取能力历史失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取能力历史失败: {err}"))
    }

    pub fn latest_window_snapshots(&self, source_id: &str) -> Result<Vec<SnapshotRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT c.capability_id, c.display_name, c.value_kind, c.primary_value, c.secondary_value,
                        c.progress, c.trend_json, c.captured_at, c.window_seconds, c.reset_at
                 FROM capability_snapshots c
                 INNER JOIN (
                     SELECT capability_id, MAX(id) AS max_id
                     FROM capability_snapshots
                     WHERE source_id = ?1 AND capability_id LIKE 'quota_window_%'
                     GROUP BY capability_id
                 ) latest ON c.id = latest.max_id",
            )
            .map_err(|err| format!("准备窗口快照查询失败: {err}"))?;
        let rows = statement
            .query_map(params![source_id], map_snapshot_record)
            .map_err(|err| format!("读取窗口快照失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取窗口快照失败: {err}"))
    }

    /// 最近 N 个本地自然日内同一 Source + 窗口能力的历史：每天取最后一条真实快照，
    /// 缺失日不返回（不插值、不补零）。返回按日期升序的 (MM-DD, 剩余百分比)。
    pub fn window_daily_trend(
        &self,
        source_id: &str,
        capability_id: &str,
        days: i64,
    ) -> Result<Vec<(String, f64)>, String> {
        self.snapshot_daily_trend(source_id, capability_id, days, parse_percent_value)
    }

    /// 最近 N 个本地自然日内同一 Source + 金额能力的历史（每日最后一条快照的原值）。
    pub fn money_daily_trend(
        &self,
        source_id: &str,
        capability_id: &str,
        days: i64,
    ) -> Result<Vec<(String, f64)>, String> {
        self.snapshot_daily_trend(source_id, capability_id, days, parse_money_value)
    }

    /// 每日趋势通用聚合：取窗口内每天最大 id（最后一次写入）的真实快照，按本地日分桶。
    fn snapshot_daily_trend(
        &self,
        source_id: &str,
        capability_id: &str,
        days: i64,
        parse: impl Fn(&str) -> Option<f64>,
    ) -> Result<Vec<(String, f64)>, String> {
        let connection = self.connect()?;
        let since = epoch_ms().saturating_sub(days * 86_400_000);
        let mut statement = connection
            .prepare(
                "SELECT id, captured_at, primary_value FROM capability_snapshots
                 WHERE source_id = ?1 AND capability_id = ?2 AND captured_at >= ?3 AND primary_value IS NOT NULL
                 ORDER BY id ASC",
            )
            .map_err(|err| format!("准备每日趋势查询失败: {err}"))?;
        let rows = statement
            .query_map(params![source_id, capability_id, since], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|err| format!("读取每日趋势失败: {err}"))?;
        // 同一本地自然日取最大 id（最后一次写入），缺失日自然缺席
        let mut by_day: std::collections::BTreeMap<chrono::NaiveDate, (i64, f64)> =
            std::collections::BTreeMap::new();
        for row in rows {
            let (id, captured_at, primary) =
                row.map_err(|err| format!("读取每日趋势失败: {err}"))?;
            let Some(value) = parse(&primary) else {
                continue;
            };
            let day = chrono::DateTime::from_timestamp_millis(captured_at)
                .map(|time| time.with_timezone(&chrono::Local).date_naive());
            let Some(day) = day else { continue };
            match by_day.get(&day) {
                Some(&(existing_id, _)) if existing_id > id => {}
                _ => {
                    by_day.insert(day, (id, value));
                }
            }
        }
        Ok(by_day
            .into_iter()
            .map(|(day, (_, value))| (day.format("%m-%d").to_string(), value))
            .collect())
    }

    pub fn refresh_history(
        &self,
        platform_id: &str,
        limit: usize,
    ) -> Result<Vec<RefreshHistoryRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT rr.run_id || ':' || rr.source_id, rr.source_id, s.display_name, a.id, a.display_name, rr.status, rr.finished_at, rr.error_message
                 FROM refresh_results rr JOIN refresh_runs r ON r.id = rr.run_id JOIN sources s ON s.id = rr.source_id JOIN accounts a ON a.id = s.account_id
                 WHERE r.platform_id = ?1 ORDER BY rr.started_at DESC, rr.id DESC LIMIT ?2",
            )
            .map_err(|err| format!("准备刷新历史查询失败: {err}"))?;
        let rows = statement
            .query_map(params![platform_id, limit as i64], |row| {
                Ok(RefreshHistoryRecord {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    source_name: row.get(2)?,
                    account_id: row.get(3)?,
                    account_name: row.get(4)?,
                    status: row.get(5)?,
                    finished_at: row.get(6)?,
                    error_message: row.get(7)?,
                })
            })
            .map_err(|err| format!("查询刷新历史失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取刷新历史失败: {err}"))
    }

    pub fn setting_string(&self, key: &str) -> Result<Option<String>, String> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT value_json FROM settings WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|err| format!("读取设置失败: {err}"))
    }

    pub fn set_setting_string(&self, key: &str, value: &str) -> Result<(), String> {
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO settings(key, value_json, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
                params![key, value, epoch_ms()],
            )
            .map(|_| ())
            .map_err(|err| format!("保存设置失败: {err}"))
    }

    pub fn reorder_user_platforms(&self, platform_ids: &[String]) -> Result<(), String> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction()
            .map_err(|err| format!("开始保存平台顺序失败: {err}"))?;
        let now = epoch_ms();
        for (index, platform_id) in platform_ids.iter().enumerate() {
            transaction
                .execute(
                    "UPDATE user_platforms SET sort_index = ?2, updated_at = ?3 WHERE platform_id = ?1",
                    params![platform_id, index as i64, now],
                )
                .map_err(|err| format!("保存平台顺序失败: {err}"))?;
        }
        transaction
            .commit()
            .map_err(|err| format!("提交平台顺序失败: {err}"))
    }

    pub fn clear_cached_snapshots(&self) -> Result<(), String> {
        let connection = self.connect()?;
        connection
            .execute_batch(
                "DELETE FROM capability_snapshots;
                 DELETE FROM refresh_results;
                 DELETE FROM refresh_runs;",
            )
            .map_err(|err| format!("清除本地缓存失败: {err}"))
    }

    pub fn replace_tibo_posts(
        &self,
        posts: &[TiboPostRecord],
        synced_at: i64,
    ) -> Result<(), String> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction()
            .map_err(|err| format!("开始保存雷达动态失败: {err}"))?;
        for post in posts {
            transaction
                .execute(
                    "INSERT INTO tibo_posts(id, url, text, posted_at, kind, tibo_lane, explicit_reset, verification_status, is_reply, replies, reposts, likes, extra_json, synced_at, translated_text, translated_at, translation_source, lifecycle_consumed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
                     ON CONFLICT(id) DO UPDATE SET
                        url = CASE WHEN excluded.url<>'' THEN excluded.url ELSE tibo_posts.url END,
                        text = CASE WHEN excluded.text<>'' THEN excluded.text ELSE tibo_posts.text END,
                        posted_at = CASE WHEN excluded.posted_at>0 THEN excluded.posted_at ELSE tibo_posts.posted_at END,
                        kind = CASE WHEN excluded.kind='unknown' THEN tibo_posts.kind ELSE excluded.kind END,
                        tibo_lane = COALESCE(excluded.tibo_lane,tibo_posts.tibo_lane), explicit_reset = MAX(excluded.explicit_reset,tibo_posts.explicit_reset),
                        verification_status = excluded.verification_status, is_reply = excluded.is_reply,
                        replies = excluded.replies, reposts = excluded.reposts, likes = excluded.likes,
                        extra_json = json_patch(tibo_posts.extra_json, excluded.extra_json), synced_at = excluded.synced_at,
                        translated_text = COALESCE(excluded.translated_text, tibo_posts.translated_text),
                        translated_at = COALESCE(excluded.translated_at, tibo_posts.translated_at),
                        translation_source = COALESCE(excluded.translation_source, tibo_posts.translation_source)",
                    params![
                        post.id, post.url, post.text, post.posted_at, post.kind, post.tibo_lane,
                        i64::from(post.explicit_reset), post.verification_status, i64::from(post.is_reply),
                        post.replies, post.reposts, post.likes, post.extra_json, synced_at,
                        post.translated_text, post.translated_at, post.translation_source,
                        post.lifecycle_consumed_at
                    ],
                )
                .map_err(|err| format!("写入雷达动态失败: {err}"))?;
        }
        transaction
            .commit()
            .map_err(|err| format!("提交雷达动态失败: {err}"))
    }

    pub fn list_tibo_posts(&self, limit: usize) -> Result<Vec<TiboPostRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, url, text, posted_at, kind, tibo_lane, explicit_reset, verification_status, is_reply, replies, reposts, likes, extra_json, synced_at, translated_text, translated_at, translation_source, lifecycle_consumed_at
                 FROM tibo_posts ORDER BY posted_at DESC LIMIT ?1",
            )
            .map_err(|err| format!("准备雷达动态查询失败: {err}"))?;
        let rows = statement
            .query_map(params![limit as i64], map_tibo_post)
            .map_err(|err| format!("查询雷达动态失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取雷达动态失败: {err}"))
    }

    /// 监控窗口内尚未消费的帖子（分析 NEW 候选）。与浏览分页（list_tibo_posts）完全独立，
    /// 不受 UI 列表条数限制；浏览范围筛选不改变本查询。
    pub fn unconsumed_tibo_posts_since(
        &self,
        since_ms: i64,
        limit: usize,
    ) -> Result<Vec<TiboPostRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, url, text, posted_at, kind, tibo_lane, explicit_reset, verification_status, is_reply, replies, reposts, likes, extra_json, synced_at, translated_text, translated_at, translation_source, lifecycle_consumed_at
                 FROM tibo_posts
                 WHERE lifecycle_consumed_at IS NULL AND posted_at >= ?1 AND posted_at > 0
                 AND NOT EXISTS(SELECT 1 FROM radar_event_evidence ee JOIN radar_events ev ON ev.id=ee.event_id WHERE ee.post_id=tibo_posts.id AND ev.closed_at IS NOT NULL)
                 ORDER BY posted_at ASC,id ASC LIMIT ?2",
            )
            .map_err(|err| format!("准备待分析动态查询失败: {err}"))?;
        let rows = statement
            .query_map(params![since_ms, limit as i64], map_tibo_post)
            .map_err(|err| format!("查询待分析动态失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取待分析动态失败: {err}"))
    }

    /// Posts in the monitor window that were consumed or flagged but never attached to an event.
    pub fn unapplied_tibo_post_ids_since(
        &self,
        since_ms: i64,
        known_missed_id: &str,
    ) -> Result<Vec<String>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id FROM tibo_posts
                 WHERE posted_at >= ?1 AND posted_at > 0
                   AND NOT EXISTS (SELECT 1 FROM radar_event_evidence e WHERE e.post_id = tibo_posts.id)
                   AND (lifecycle_consumed_at IS NOT NULL OR id = ?2)
                 ORDER BY posted_at ASC, id ASC",
            )
            .map_err(|err| format!("准备未应用动态查询失败: {err}"))?;
        let rows = statement
            .query_map(params![since_ms, known_missed_id], |row| row.get(0))
            .map_err(|err| format!("查询未应用动态失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取未应用动态失败: {err}"))
    }

    pub fn list_tibo_posts_page(
        &self,
        from_ms: Option<i64>,
        to_ms: Option<i64>,
        after_posted_at: Option<i64>,
        after_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<TiboPostRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, url, text, posted_at, kind, tibo_lane, explicit_reset, verification_status, is_reply, replies, reposts, likes, extra_json, synced_at, translated_text, translated_at, translation_source, lifecycle_consumed_at
                 FROM tibo_posts
                 WHERE (?1 IS NULL OR posted_at >= ?1)
                   AND (?2 IS NULL OR posted_at <= ?2)
                   AND (?3 IS NULL OR posted_at < ?3 OR (posted_at = ?3 AND id < ?4))
                 ORDER BY posted_at DESC, id DESC LIMIT ?5",
            )
            .map_err(|err| format!("准备雷达动态分页失败: {err}"))?;
        let rows = statement
            .query_map(
                params![from_ms, to_ms, after_posted_at, after_id.unwrap_or(""), limit as i64],
                map_tibo_post,
            )
            .map_err(|err| format!("查询雷达动态分页失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取雷达动态分页失败: {err}"))
    }

    /// 按 ID 集合取帖（事件上下文 / 历史上下文），与浏览分页独立。
    pub fn tibo_posts_by_ids(&self, ids: &[String]) -> Result<Vec<TiboPostRecord>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, url, text, posted_at, kind, tibo_lane, explicit_reset, verification_status, is_reply, replies, reposts, likes, extra_json, synced_at, translated_text, translated_at, translation_source, lifecycle_consumed_at
                 FROM tibo_posts WHERE id = ?1",
            )
            .map_err(|err| format!("准备动态批量查询失败: {err}"))?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let rows = statement
                .query_map(params![id], map_tibo_post)
                .map_err(|err| format!("查询动态失败: {err}"))?;
            out.extend(
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|err| format!("读取动态失败: {err}"))?,
            );
        }
        out.sort_by(|a, b| b.posted_at.cmp(&a.posted_at));
        Ok(out)
    }

    /// 保存一条动态的中文翻译（由 translate_radar_post 命令调用）。
    pub fn update_tibo_translation(
        &self,
        post_id: &str,
        translated_text: &str,
        translated_at: i64,
        translation_source: &str,
    ) -> Result<(), String> {
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "UPDATE tibo_posts SET translated_text = ?2, translated_at = ?3, translation_source = ?4 WHERE id = ?1",
                params![post_id, translated_text, translated_at, translation_source],
            )
            .map_err(|err| format!("保存雷达翻译失败: {err}"))?;
        if changed == 0 {
            return Err(format!("雷达动态 {post_id} 不存在"));
        }
        Ok(())
    }

    /// 更新动态的中文翻译及 AI 归类。
    pub fn update_tibo_enrichment(
        &self,
        post_id: &str,
        translated_text: Option<&str>,
        translated_at: i64,
        translation_source: &str,
        kind: Option<&str>,
        explicit_reset: Option<bool>,
        tibo_lane: Option<&str>,
    ) -> Result<(), String> {
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "UPDATE tibo_posts
                 SET translated_text = COALESCE(?2, translated_text),
                     translated_at = CASE WHEN ?2 IS NOT NULL THEN ?3 ELSE translated_at END,
                     translation_source = CASE WHEN ?2 IS NOT NULL THEN ?4 ELSE translation_source END,
                     kind = COALESCE(?5, kind),
                     explicit_reset = COALESCE(?6, explicit_reset),
                     tibo_lane = COALESCE(?7, tibo_lane)
                 WHERE id = ?1",
                params![
                    post_id,
                    translated_text,
                    translated_at,
                    translation_source,
                    kind,
                    explicit_reset.map(|b| if b { 1i64 } else { 0i64 }),
                    tibo_lane,
                ],
            )
            .map_err(|err| format!("更新动态富化失败: {err}"))?;
        if changed == 0 {
            return Err(format!("雷达动态 {post_id} 不存在"));
        }
        Ok(())
    }

    /// 查询指定时间后未翻译的动态（用于后台 AI 自动丰富翻译）。
    pub fn untranslated_tibo_posts_since(
        &self,
        since_ms: i64,
        limit: usize,
    ) -> Result<Vec<TiboPostRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, url, text, posted_at, kind, tibo_lane, explicit_reset, verification_status, is_reply, replies, reposts, likes, extra_json, synced_at, translated_text, translated_at, translation_source, lifecycle_consumed_at
                 FROM tibo_posts
                 WHERE posted_at >= ?1 AND (translated_text IS NULL OR translated_text = '')
                 ORDER BY posted_at DESC
                 LIMIT ?2",
            )
            .map_err(|err| format!("准备未翻译动态查询失败: {err}"))?;
        let rows = statement
            .query_map(params![since_ms, limit as i64], map_tibo_post)
            .map_err(|err| format!("读取未翻译动态失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("收集未翻译动态失败: {err}"))
    }

    /// 查询指定时间后待丰富（未翻译或分类为 unknown / 未分类）的动态。
    pub fn unenriched_tibo_posts_since(
        &self,
        since_ms: i64,
        limit: usize,
    ) -> Result<Vec<TiboPostRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, url, text, posted_at, kind, tibo_lane, explicit_reset, verification_status, is_reply, replies, reposts, likes, extra_json, synced_at, translated_text, translated_at, translation_source, lifecycle_consumed_at
                 FROM tibo_posts
                 WHERE posted_at >= ?1 AND (
                     translated_text IS NULL OR translated_text = ''
                     OR kind = 'unknown'
                     OR tibo_lane IS NULL OR tibo_lane = '未分类'
                 )
                 ORDER BY posted_at DESC
                 LIMIT ?2",
            )
            .map_err(|err| format!("准备待丰富动态查询失败: {err}"))?;
        let rows = statement
            .query_map(params![since_ms, limit as i64], map_tibo_post)
            .map_err(|err| format!("读取待丰富动态失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("收集待丰富动态失败: {err}"))
    }

    /// 成功完成实时分析后标记本批新增帖子；已消费的帖子保持原时间。
    /// 生产路径由 insert_radar_analysis_and_consume 事务替代；保留给测试直接构造消费状态。
    #[allow(dead_code)]
    pub fn mark_tibo_posts_consumed(
        &self,
        post_ids: &[String],
        consumed_at: i64,
    ) -> Result<(), String> {
        if post_ids.is_empty() {
            return Ok(());
        }
        let connection = self.connect()?;
        for post_id in post_ids {
            connection
                .execute(
                    "UPDATE tibo_posts SET lifecycle_consumed_at = ?2
                     WHERE id = ?1 AND lifecycle_consumed_at IS NULL",
                    params![post_id, consumed_at],
                )
                .map_err(|err| format!("标记雷达帖子已消费失败: {err}"))?;
        }
        Ok(())
    }

    /// 雷达自定义分析模型：按来源保存用户验证过可用的模型名。
    pub fn list_radar_custom_models(&self) -> Result<Vec<RadarCustomModelRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare("SELECT source_id, model, created_at FROM radar_custom_models ORDER BY created_at, id")
            .map_err(|err| format!("准备自定义模型查询失败: {err}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok(RadarCustomModelRecord {
                    source_id: row.get(0)?,
                    model: row.get(1)?,
                    created_at: row.get(2)?,
                })
            })
            .map_err(|err| format!("查询自定义模型失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取自定义模型失败: {err}"))
    }

    pub fn add_radar_custom_model(&self, source_id: &str, model: &str) -> Result<(), String> {
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT OR IGNORE INTO radar_custom_models(source_id, model, created_at) VALUES (?1, ?2, ?3)",
                params![source_id, model, epoch_ms()],
            )
            .map_err(|err| format!("保存自定义模型失败: {err}"))?;
        Ok(())
    }

    pub fn delete_radar_custom_model(&self, source_id: &str, model: &str) -> Result<(), String> {
        let connection = self.connect()?;
        connection
            .execute(
                "DELETE FROM radar_custom_models WHERE source_id = ?1 AND model = ?2",
                params![source_id, model],
            )
            .map_err(|err| format!("删除自定义模型失败: {err}"))?;
        Ok(())
    }

    pub fn list_radar_chat_endpoints(&self) -> Result<Vec<RadarChatEndpointRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, display_name, api_base_url, secret_ref, created_at
                 FROM radar_chat_endpoints ORDER BY created_at, id",
            )
            .map_err(|err| format!("准备对话接入查询失败: {err}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok(RadarChatEndpointRecord {
                    id: row.get(0)?,
                    display_name: row.get(1)?,
                    api_base_url: row.get(2)?,
                    secret_ref: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|err| format!("查询对话接入失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取对话接入失败: {err}"))
    }

    pub fn radar_chat_endpoint(
        &self,
        endpoint_id: &str,
    ) -> Result<Option<RadarChatEndpointRecord>, String> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT id, display_name, api_base_url, secret_ref, created_at
                 FROM radar_chat_endpoints WHERE id = ?1",
                params![endpoint_id],
                |row| {
                    Ok(RadarChatEndpointRecord {
                        id: row.get(0)?,
                        display_name: row.get(1)?,
                        api_base_url: row.get(2)?,
                        secret_ref: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(|err| format!("读取对话接入失败: {err}"))
    }

    pub fn insert_radar_chat_endpoint(
        &self,
        record: &RadarChatEndpointRecord,
    ) -> Result<(), String> {
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO radar_chat_endpoints(id, display_name, api_base_url, secret_ref, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    record.id,
                    record.display_name,
                    record.api_base_url,
                    record.secret_ref,
                    record.created_at
                ],
            )
            .map_err(|err| format!("保存对话接入失败: {err}"))?;
        Ok(())
    }

    pub fn delete_radar_chat_endpoint(&self, endpoint_id: &str) -> Result<(), String> {
        let connection = self.connect()?;
        connection
            .execute(
                "DELETE FROM radar_chat_endpoints WHERE id = ?1",
                params![endpoint_id],
            )
            .map_err(|err| format!("删除对话接入失败: {err}"))?;
        Ok(())
    }

    pub fn list_radar_chat_endpoint_models(
        &self,
        endpoint_id: &str,
    ) -> Result<Vec<RadarChatEndpointModelRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT endpoint_id, model, created_at FROM radar_chat_endpoint_models
                 WHERE endpoint_id = ?1 ORDER BY created_at, model",
            )
            .map_err(|err| format!("准备接入模型查询失败: {err}"))?;
        let rows = statement
            .query_map(params![endpoint_id], |row| {
                Ok(RadarChatEndpointModelRecord {
                    endpoint_id: row.get(0)?,
                    model: row.get(1)?,
                    created_at: row.get(2)?,
                })
            })
            .map_err(|err| format!("查询接入模型失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取接入模型失败: {err}"))
    }

    pub fn add_radar_chat_endpoint_model(
        &self,
        endpoint_id: &str,
        model: &str,
    ) -> Result<(), String> {
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT OR IGNORE INTO radar_chat_endpoint_models(endpoint_id, model, created_at)
                 VALUES (?1, ?2, ?3)",
                params![endpoint_id, model, epoch_ms()],
            )
            .map_err(|err| format!("保存接入模型失败: {err}"))?;
        Ok(())
    }

    pub fn delete_radar_chat_endpoint_model(
        &self,
        endpoint_id: &str,
        model: &str,
    ) -> Result<(), String> {
        let connection = self.connect()?;
        connection
            .execute(
                "DELETE FROM radar_chat_endpoint_models WHERE endpoint_id = ?1 AND model = ?2",
                params![endpoint_id, model],
            )
            .map_err(|err| format!("删除接入模型失败: {err}"))?;
        Ok(())
    }

    pub fn insert_radar_check(&self, check: &RadarCheckRecord) -> Result<(), String> {
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO radar_checks(id, started_at, finished_at, status, sync_status, parse_status, analyze_status, error_message, post_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    check.id, check.started_at, check.finished_at, check.status, check.sync_status,
                    check.parse_status, check.analyze_status, check.error_message, check.post_count
                ],
            )
            .map(|_| ())
            .map_err(|err| format!("写入雷达检查失败: {err}"))
    }

    pub fn list_radar_checks(&self, limit: usize) -> Result<Vec<RadarCheckRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, started_at, finished_at, status, sync_status, parse_status, analyze_status, error_message, post_count
                 FROM radar_checks ORDER BY started_at DESC LIMIT ?1",
            )
            .map_err(|err| format!("准备雷达检查查询失败: {err}"))?;
        let rows = statement
            .query_map(params![limit as i64], |row| {
                Ok(RadarCheckRecord {
                    id: row.get(0)?,
                    started_at: row.get(1)?,
                    finished_at: row.get(2)?,
                    status: row.get(3)?,
                    sync_status: row.get(4)?,
                    parse_status: row.get(5)?,
                    analyze_status: row.get(6)?,
                    error_message: row.get(7)?,
                    post_count: row.get(8)?,
                })
            })
            .map_err(|err| format!("查询雷达检查失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取雷达检查失败: {err}"))
    }

    pub fn radar_analysis_by_id(&self, id: &str) -> Result<Option<RadarAnalysisRecord>, String> {
        let connection = self.connect()?;
        connection
            .query_row(
                &radar_analysis_select("WHERE id = ?1"),
                [id],
                map_radar_analysis,
            )
            .optional()
            .map_err(|err| format!("读取雷达分析失败: {err}"))
    }

    pub fn latest_radar_analysis(&self) -> Result<Option<RadarAnalysisRecord>, String> {
        let connection = self.connect()?;
        connection
            .query_row(
                &radar_analysis_select(
                    "WHERE error_message IS NULL ORDER BY created_at DESC LIMIT 1",
                ),
                [],
                map_radar_analysis,
            )
            .optional()
            .map_err(|err| format!("读取雷达分析失败: {err}"))
    }

    /// 指定事件的最新成功分析（eventAnalysis：本轮事件为什么成立）。
    pub fn latest_event_radar_analysis(
        &self,
        event_id: &str,
    ) -> Result<Option<RadarAnalysisRecord>, String> {
        let connection = self.connect()?;
        connection
            .query_row(
                &radar_analysis_select(
                    "WHERE error_message IS NULL AND (event_id = ?1 OR id IN (SELECT analysis_id FROM radar_analysis_events WHERE event_id=?1)) ORDER BY created_at DESC LIMIT 1",
                ),
                params![event_id],
                map_radar_analysis,
            )
            .optional()
            .map_err(|err| format!("读取事件雷达分析失败: {err}"))
    }

    /// 最近一次已关闭事件（「最近一次事件」折叠区来源；不冒充当前信号）。
    pub fn latest_closed_radar_event(&self) -> Result<Option<RadarEventRecord>, String> {
        let connection = self.connect()?;
        connection
            .query_row(
                &radar_event_select(
                    "WHERE closed_at IS NOT NULL ORDER BY closed_at DESC LIMIT 1",
                ),
                [],
                map_radar_event,
            )
            .optional()
            .map_err(|err| format!("读取最近关闭的重置事件失败: {err}"))
    }

    /// 最近一次被本机观察或用户确认的重置事件；“最近一次重置”唯一来源。
    /// 不要求事件已关闭：仍处于观察期（landed_observed/user_confirmed 未到期关闭）的事件
    /// 同样是有效事实，仅查已关闭事件会漏掉最新观察。
    /// invalid_historical_replay、timeout、claimed_unverified 等普通关闭事件不参与，
    /// 时间只取 observed_reset_at / user_confirmed_reset_at，禁止回退 claimed_landed_at 或 closed_at。
    pub fn latest_confirmed_reset_event(&self) -> Result<Option<RadarEventRecord>, String> {
        let connection = self.connect()?;
        connection
            .query_row(
                &radar_event_select(
                    "WHERE event_type = 'quota_reset'
                       AND (observed_reset_at IS NOT NULL OR user_confirmed_reset_at IS NOT NULL)
                     ORDER BY COALESCE(observed_reset_at, user_confirmed_reset_at) DESC
                     LIMIT 1",
                ),
                [],
                map_radar_event,
            )
            .optional()
            .map_err(|err| format!("读取最近确认的重置事件失败: {err}"))
    }

    /// 分析复用查找：新增输入、事件上下文、提示词哈希、模型与 prompt 版本完全一致的成功分析。
    pub fn find_reusable_radar_analysis(
        &self,
        input_hash: &str,
        context_hash: &str,
        prompt_hash: &str,
        source_id: Option<&str>,
        model: Option<&str>,
        prompt_version: &str,
        temporal_phase: &str,
        state_revision: i64,
        timezone_policy_version: &str,
        now: i64,
    ) -> Result<Option<RadarAnalysisRecord>, String> {
        let connection = self.connect()?;
        connection
            .query_row(
                &radar_analysis_select(
                    "WHERE error_message IS NULL AND input_hash = ?1 AND context_hash = ?2 AND prompt_hash = ?3
                     AND source_id IS ?4 AND model IS ?5 AND prompt_version = ?6 AND temporal_phase = ?7
                     AND state_revision = ?8 AND timezone_policy_version = ?9
                     AND (valid_until IS NULL OR valid_until > ?10)
                     ORDER BY created_at DESC LIMIT 1",
                ),
                params![
                    input_hash, context_hash, prompt_hash, source_id, model, prompt_version,
                    temporal_phase, state_revision, timezone_policy_version, now,
                ],
                map_radar_analysis,
            )
            .optional()
            .map_err(|err| format!("查找可复用雷达分析失败: {err}"))
    }

    pub fn insert_radar_analysis(&self, analysis: &RadarAnalysisRecord) -> Result<(), String> {
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO radar_analyses(id, created_at, range_key, cut_post_id, from_posted_at, to_posted_at, source_id, model, prompt_version, input_hash, conclusion, analysis_basis, confidence, citations_json, support_json, against_json, uncertainty_json, new_post_ids_json, event_context_post_ids_json, historical_post_ids_json, error_message, event_id, analysis_mode, context_hash, prompt_hash, event_relation, event_phase, delta_effect, signal_level, context_status, temporal_phase, valid_until, state_revision, timezone_policy_version, signal_type)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35)",
                params![
                    analysis.id, analysis.created_at, analysis.range_key, analysis.cut_post_id,
                    analysis.from_posted_at, analysis.to_posted_at, analysis.source_id, analysis.model,
                    analysis.prompt_version, analysis.input_hash, analysis.conclusion, analysis.analysis_basis, analysis.confidence,
                    analysis.citations_json, analysis.support_json, analysis.against_json,
                    analysis.uncertainty_json, analysis.new_post_ids_json,
                    analysis.event_context_post_ids_json, analysis.historical_post_ids_json,
                    analysis.error_message,
                    analysis.event_id, analysis.analysis_mode, analysis.context_hash, analysis.prompt_hash,
                    analysis.event_relation, analysis.event_phase, analysis.delta_effect,
                    analysis.signal_level, analysis.context_status,
                    analysis.temporal_phase, analysis.valid_until, analysis.state_revision,
                    analysis.timezone_policy_version, analysis.signal_type,
                ],
            )
            .map(|_| ())
            .map_err(|err| format!("保存雷达分析失败: {err}"))
    }

    /// 分析落库与材料消费在同一事务：分析记录、事件推进结果与 lifecycle_consumed_at
    /// 保持一致，失败时一起回滚，不出现「材料被消耗但没有分析记录」的中间态。
    pub fn insert_radar_analysis_and_consume(
        &self,
        analysis: &RadarAnalysisRecord,
        consumed_post_ids: &[String],
        consumed_at: i64,
    ) -> Result<(), String> {
        if self.in_transaction() {
            self.insert_radar_analysis(analysis)?;
            return self.mark_tibo_posts_consumed(consumed_post_ids, consumed_at);
        }
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction()
            .map_err(|err| format!("开始分析落库事务失败: {err}"))?;
        transaction
            .execute(
                "INSERT INTO radar_analyses(id, created_at, range_key, cut_post_id, from_posted_at, to_posted_at, source_id, model, prompt_version, input_hash, conclusion, analysis_basis, confidence, citations_json, support_json, against_json, uncertainty_json, new_post_ids_json, event_context_post_ids_json, historical_post_ids_json, error_message, event_id, analysis_mode, context_hash, prompt_hash, event_relation, event_phase, delta_effect, signal_level, context_status, temporal_phase, valid_until, state_revision, timezone_policy_version, signal_type)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35)",
                params![
                    analysis.id, analysis.created_at, analysis.range_key, analysis.cut_post_id,
                    analysis.from_posted_at, analysis.to_posted_at, analysis.source_id, analysis.model,
                    analysis.prompt_version, analysis.input_hash, analysis.conclusion, analysis.analysis_basis, analysis.confidence,
                    analysis.citations_json, analysis.support_json, analysis.against_json,
                    analysis.uncertainty_json, analysis.new_post_ids_json,
                    analysis.event_context_post_ids_json, analysis.historical_post_ids_json,
                    analysis.error_message,
                    analysis.event_id, analysis.analysis_mode, analysis.context_hash, analysis.prompt_hash,
                    analysis.event_relation, analysis.event_phase, analysis.delta_effect,
                    analysis.signal_level, analysis.context_status,
                    analysis.temporal_phase, analysis.valid_until, analysis.state_revision,
                    analysis.timezone_policy_version, analysis.signal_type,
                ],
            )
            .map_err(|err| format!("保存雷达分析失败: {err}"))?;
        for post_id in consumed_post_ids {
            transaction
                .execute(
                    "UPDATE tibo_posts SET lifecycle_consumed_at = ?2
                     WHERE id = ?1 AND lifecycle_consumed_at IS NULL",
                    params![post_id, consumed_at],
                )
                .map_err(|err| format!("标记雷达帖子已消费失败: {err}"))?;
        }
        transaction
            .commit()
            .map_err(|err| format!("提交分析落库事务失败: {err}"))
    }

    pub fn active_radar_event(&self) -> Result<Option<RadarEventRecord>, String> {
        Ok(self.active_radar_events()?.into_iter().next())
    }

    pub fn active_radar_events(&self) -> Result<Vec<RadarEventRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(&radar_event_select(
                "WHERE closed_at IS NULL ORDER BY latest_evidence_at DESC, updated_at DESC",
            ))
            .map_err(|err| format!("准备活动重置事件查询失败: {err}"))?;
        let rows = statement
            .query_map([], map_radar_event)
            .map_err(|err| format!("读取活动重置事件失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取活动重置事件失败: {err}"))
    }

    pub fn active_radar_event_of_type(
        &self,
        event_type: &str,
    ) -> Result<Option<RadarEventRecord>, String> {
        let connection = self.connect()?;
        connection
            .query_row(
                &radar_event_select(
                    "WHERE closed_at IS NULL AND event_type = ?1 ORDER BY latest_evidence_at DESC, updated_at DESC LIMIT 1",
                ),
                params![event_type],
                map_radar_event,
            )
            .optional()
            .map_err(|err| format!("读取指定类型活动重置事件失败: {err}"))
    }

    pub fn radar_event(&self, event_id: &str) -> Result<Option<RadarEventRecord>, String> {
        let connection = self.connect()?;
        connection
            .query_row(
                &radar_event_select("WHERE id = ?1"),
                params![event_id],
                map_radar_event,
            )
            .optional()
            .map_err(|err| format!("读取重置事件失败: {err}"))
    }

    pub fn radar_events_page(
        &self,
        after_sort_at: Option<i64>,
        after_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RadarEventRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(&radar_event_select(
                "WHERE (?1 IS NULL OR COALESCE(closed_at, latest_evidence_at) < ?1
                    OR (COALESCE(closed_at, latest_evidence_at) = ?1 AND id < ?2))
                 ORDER BY (closed_at IS NULL) DESC, COALESCE(closed_at, latest_evidence_at) DESC, id DESC
                 LIMIT ?3",
            ))
            .map_err(|err| format!("准备重置事件分页失败: {err}"))?;
        let rows = statement
            .query_map(
                params![after_sort_at, after_id.unwrap_or(""), limit as i64],
                map_radar_event,
            )
            .map_err(|err| format!("读取重置事件分页失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取重置事件分页失败: {err}"))
    }

    pub fn recently_closed_radar_events(&self, since_ms: i64) -> Result<Vec<RadarEventRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(&radar_event_select(
                "WHERE closed_at IS NOT NULL AND closed_at >= ?1 ORDER BY closed_at DESC",
            ))
            .map_err(|err| format!("准备近期关闭事件查询失败: {err}"))?;
        let rows = statement
            .query_map(params![since_ms], map_radar_event)
            .map_err(|err| format!("读取近期关闭事件失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取近期关闭事件失败: {err}"))
    }

    pub fn radar_event_transitions(
        &self,
        event_id: &str,
    ) -> Result<Vec<(i64, String, String, String)>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT at, before_json, after_json, reason FROM radar_transitions
                 WHERE event_id = ?1 ORDER BY at ASC, id ASC",
            )
            .map_err(|err| format!("准备事件状态转换查询失败: {err}"))?;
        let rows = statement
            .query_map(params![event_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .map_err(|err| format!("读取事件状态转换失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取事件状态转换失败: {err}"))
    }

    pub fn radar_event_time_basis(
        &self,
        event_id: &str,
    ) -> Result<Option<(Option<String>, Option<String>, String, Option<String>, bool)>, String> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT post_id, raw_text, precision, timezone_kind, timezone_assumed
                 FROM radar_event_time_basis WHERE event_id = ?1",
                [event_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get::<_, i64>(4)? != 0,
                    ))
                },
            )
            .optional()
            .map_err(|err| format!("读取事件时间依据失败: {err}"))
    }

    /// 历史记录页事件列表：活动事件在前，其余按关闭/最新证据时间倒序。
    pub fn radar_events_recent(&self, limit: usize) -> Result<Vec<RadarEventRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(&radar_event_select(
                "ORDER BY (closed_at IS NULL) DESC, COALESCE(closed_at, latest_evidence_at) DESC LIMIT ?1",
            ))
            .map_err(|err| format!("准备重置事件历史查询失败: {err}"))?;
        let rows = statement
            .query_map(params![limit as i64], map_radar_event)
            .map_err(|err| format!("读取重置事件历史失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取重置事件历史失败: {err}"))
    }

    /// 事件关联的全部成功分析（历史页「当时的 AI 解释」按时间正序展示）。
    pub fn event_radar_analyses_page(&self,event_id:&str,at:Option<i64>,id:Option<&str>,limit:usize) -> Result<Vec<RadarAnalysisRecord>,String> {
        let connection=self.connect()?;
        let mut stmt=connection.prepare(&radar_analysis_select("WHERE error_message IS NULL AND (event_id=?1 OR id IN(SELECT analysis_id FROM radar_analysis_events WHERE event_id=?1)) AND (?2 IS NULL OR created_at<?2 OR (created_at=?2 AND id<?3)) ORDER BY created_at DESC,id DESC LIMIT ?4")).map_err(|e|e.to_string())?;
        let rows=stmt.query_map(params![event_id,at,id,limit as i64],map_radar_analysis).map_err(|e|e.to_string())?;
        rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
    }

    pub fn event_radar_analyses(
        &self,
        event_id: &str,
        limit: usize,
    ) -> Result<Vec<RadarAnalysisRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(&radar_analysis_select(
                "WHERE error_message IS NULL AND (event_id = ?1 OR id IN (SELECT analysis_id FROM radar_analysis_events WHERE event_id=?1)) ORDER BY created_at ASC LIMIT ?2",
            ))
            .map_err(|err| format!("准备事件分析历史查询失败: {err}"))?;
        let rows = statement
            .query_map(params![event_id, limit as i64], map_radar_analysis)
            .map_err(|err| format!("读取事件分析历史失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取事件分析历史失败: {err}"))
    }

    pub fn insert_radar_event(&self, event: &RadarEventRecord) -> Result<(), String> {
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO radar_events(id, phase, title, summary, first_signal_at, latest_evidence_at, claimed_landed_at, observed_reset_at, closed_at, close_reason, expected_at, expires_at, state_revision, user_confirmed_reset_at, event_type, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                params![
                    event.id, event.phase, event.title, event.summary, event.first_signal_at,
                    event.latest_evidence_at, event.claimed_landed_at, event.observed_reset_at,
                    event.closed_at, event.close_reason, event.expected_at, event.expires_at,
                    event.state_revision, event.user_confirmed_reset_at, event.event_type,
                    epoch_ms(), epoch_ms(),
                ],
            )
            .map(|_| ())
            .map_err(|err| format!("写入重置事件失败: {err}"))
    }

    pub fn update_radar_event(&self, event: &RadarEventRecord) -> Result<(), String> {
        let connection = self.connect()?;
        connection
            .execute(
                "UPDATE radar_events SET phase = ?2, title = ?3, summary = ?4, latest_evidence_at = ?5,
                 claimed_landed_at = ?6, observed_reset_at = ?7, closed_at = ?8, close_reason = ?9,
                 expected_at = ?10, expires_at = ?11, state_revision = ?12, user_confirmed_reset_at = ?13,
                 updated_at = ?14
                 WHERE id = ?1",
                params![
                    event.id, event.phase, event.title, event.summary, event.latest_evidence_at,
                    event.claimed_landed_at, event.observed_reset_at, event.closed_at, event.close_reason,
                    event.expected_at, event.expires_at, event.state_revision, event.user_confirmed_reset_at,
                    epoch_ms(),
                ],
            )
            .map(|_| ())
            .map_err(|err| format!("更新重置事件失败: {err}"))
    }

    /// 事件关联原帖：同事件同帖去重；relation 为 context | delta。
    pub fn add_radar_event_evidence(
        &self,
        event_id: &str,
        post_id: &str,
        relation: &str,
        analysis_id: &str,
    ) -> Result<(), String> {
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT OR IGNORE INTO radar_event_evidence(event_id, post_id, relation, analysis_id, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![event_id, post_id, relation, analysis_id, epoch_ms()],
            )
            .map(|_| ())
            .map_err(|err| format!("写入事件证据失败: {err}"))
    }

    pub fn radar_event_post_ids(&self, event_id: &str) -> Result<Vec<String>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT p.id FROM radar_event_evidence e JOIN tibo_posts p ON p.id = e.post_id
                 WHERE e.event_id = ?1 ORDER BY p.posted_at DESC",
            )
            .map_err(|err| format!("准备事件证据查询失败: {err}"))?;
        let rows = statement
            .query_map(params![event_id], |row| row.get::<_, String>(0))
            .map_err(|err| format!("读取事件证据失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取事件证据失败: {err}"))
    }

    pub fn closed_radar_event_post_ids(&self) -> Result<Vec<String>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT DISTINCT e.post_id FROM radar_event_evidence e
                 JOIN radar_events r ON r.id = e.event_id WHERE r.closed_at IS NOT NULL",
            )
            .map_err(|err| format!("准备历史事件证据查询失败: {err}"))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|err| format!("读取历史事件证据失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取历史事件证据失败: {err}"))
    }

    pub fn replace_radar_time_claims(
        &self,
        post_id: &str,
        claims: &[RadarTimeClaimRecord],
    ) -> Result<(), String> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction()
            .map_err(|err| format!("开始时间声明事务失败: {err}"))?;
        transaction
            .execute(
                "DELETE FROM radar_time_claims WHERE post_id = ?1",
                params![post_id],
            )
            .map_err(|err| format!("清理时间声明失败: {err}"))?;
        let now = epoch_ms();
        for claim in claims {
        transaction
            .execute(
                "INSERT INTO radar_time_claims(post_id, raw_text, clock_hour, clock_minute, date_relation,
                 timezone_kind, timezone_assumed, parse_status, resolved_at, precision, parser_version,
                 claim_kind, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)",
                params![post_id, claim.raw_text, claim.clock_hour, claim.clock_minute,
                    claim.date_relation, claim.timezone_kind, i64::from(claim.timezone_assumed),
                    claim.parse_status, claim.resolved_at, claim.precision, claim.parser_version,
                    claim.claim_kind, now,
                ],
            )
                .map_err(|err| format!("写入时间声明失败: {err}"))?;
        }
        transaction
            .commit()
            .map_err(|err| format!("提交时间声明失败: {err}"))
    }

    pub fn radar_time_claims_for_posts(
        &self,
        post_ids: &[String],
    ) -> Result<Vec<RadarTimeClaimRecord>, String> {
        if post_ids.is_empty() {
            return Ok(Vec::new());
        }
        let connection = self.connect()?;
        let mut out = Vec::new();
        let mut statement = connection
            .prepare(
                "SELECT post_id, raw_text, clock_hour, clock_minute, date_relation, timezone_kind,
                        timezone_assumed, parse_status, resolved_at, precision, parser_version, claim_kind
                 FROM radar_time_claims WHERE post_id = ?1 ORDER BY id",
            )
            .map_err(|err| format!("准备时间声明查询失败: {err}"))?;
        for post_id in post_ids {
            let rows = statement
                .query_map(params![post_id], |row| {
                    Ok(RadarTimeClaimRecord {
                        post_id: row.get(0)?,
                        raw_text: row.get(1)?,
                        clock_hour: row.get(2)?,
                        clock_minute: row.get(3)?,
                        date_relation: row.get(4)?,
                        timezone_kind: row.get(5)?,
                        timezone_assumed: row.get::<_, i64>(6)? != 0,
                        parse_status: row.get(7)?,
                        resolved_at: row.get(8)?,
                        precision: row.get(9)?,
                        parser_version: row.get(10)?,
                        claim_kind: row
                            .get::<_, Option<String>>(11)?
                            .filter(|value| !value.is_empty())
                            .unwrap_or_else(|| "unknown".into()),
                    })
                })
                .map_err(|err| format!("读取时间声明失败: {err}"))?;
            out.extend(
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|err| format!("读取时间声明失败: {err}"))?,
            );
        }
        Ok(out)
    }

    /// GPT 平台的额度来源（含账号信息），按账号创建顺序返回。
    pub fn openai_quota_sources(&self) -> Result<Vec<SourceRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT s.id, s.account_id, a.display_name, a.kind, a.platform_id, s.adapter_id, s.source_type,
                        s.display_name, s.secret_ref, s.state, s.last_validated_at, s.last_success_at, s.error_code, s.error_message
                 FROM sources s JOIN accounts a ON a.id = s.account_id
                 WHERE a.platform_id = 'openai'
                 ORDER BY a.created_at, s.id",
            )
            .map_err(|err| format!("准备额度来源查询失败: {err}"))?;
        let rows = statement
            .query_map([], map_source)
            .map_err(|err| format!("读取额度来源失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取额度来源失败: {err}"))
    }

    /// 结构化额度窗口快照（window_seconds 非空），同一能力内按捕获时间倒序；
    /// 观察器在代码里取相邻成功快照对，不在 SQL 里做窗口函数。
    pub fn recent_window_samples(
        &self,
        source_id: &str,
    ) -> Result<Vec<WindowSampleRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, capability_id, display_name, progress, captured_at, window_seconds, reset_at
                 FROM capability_snapshots
                 WHERE source_id = ?1 AND capability_id LIKE 'quota_window_%' AND window_seconds IS NOT NULL
                   AND captured_at >= ?2
                 ORDER BY capability_id, captured_at ASC, id ASC",
            )
            .map_err(|err| format!("准备额度窗口样本查询失败: {err}"))?;
        let since = epoch_ms().saturating_sub(30 * 86_400_000);
        let rows = statement
            .query_map(params![source_id, since], |row| {
                Ok(WindowSampleRecord {
                    id: row.get(0)?,
                    capability_id: row.get(1)?,
                    display_name: row.get(2)?,
                    progress: row.get(3)?,
                    captured_at: row.get(4)?,
                    window_seconds: row.get(5)?,
                    reset_at: row.get(6)?,
                })
            })
            .map_err(|err| format!("读取额度窗口样本失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取额度窗口样本失败: {err}"))
    }

    pub fn insert_quota_reset_observation(
        &self,
        observation: &QuotaResetObservationRecord,
    ) -> Result<i64, String> {
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT OR IGNORE INTO quota_reset_observations(account_id, source_id, capability_id,
                 previous_snapshot_id, current_snapshot_id, classification, observed_at, event_id,
                 temporal_correlation, user_confirmed_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    observation.account_id, observation.source_id, observation.capability_id,
                    observation.previous_snapshot_id, observation.current_snapshot_id,
                    observation.classification, observation.observed_at, observation.event_id,
                    observation.temporal_correlation, observation.user_confirmed_at, epoch_ms(),
                ],
            )
            .map_err(|err| format!("保存额度重置观察失败: {err}"))?;
        connection
            .query_row(
                "SELECT id FROM quota_reset_observations WHERE source_id = ?1 AND capability_id = ?2
                 AND previous_snapshot_id = ?3 AND current_snapshot_id = ?4",
                params![
                    observation.source_id, observation.capability_id,
                    observation.previous_snapshot_id, observation.current_snapshot_id,
                ],
                |row| row.get(0),
            )
            .map_err(|err| format!("读取额度重置观察失败: {err}"))
    }

    pub fn quota_reset_observations(
        &self,
        source_id: Option<&str>,
        event_id: Option<&str>,
    ) -> Result<Vec<QuotaResetObservationRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, account_id, source_id, capability_id, previous_snapshot_id, current_snapshot_id,
                        classification, observed_at, event_id, temporal_correlation, user_confirmed_at
                 FROM quota_reset_observations
                 WHERE (?1 IS NULL OR source_id = ?1) AND (?2 IS NULL OR event_id = ?2)
                 ORDER BY observed_at DESC, id DESC",
            )
            .map_err(|err| format!("准备额度观察查询失败: {err}"))?;
        let rows = statement
            .query_map(params![source_id, event_id], map_quota_reset_observation)
            .map_err(|err| format!("读取额度观察失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取额度观察失败: {err}"))
    }

    pub fn confirm_quota_reset_observation(
        &self,
        observation_id: i64,
        confirmed_at: i64,
    ) -> Result<(), String> {
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "UPDATE quota_reset_observations SET user_confirmed_at = ?2 WHERE id = ?1",
                params![observation_id, confirmed_at],
            )
            .map_err(|err| format!("确认额度重置观察失败: {err}"))?;
        if changed == 0 {
            Err("未找到额度重置观察".into())
        } else {
            Ok(())
        }
    }

    /// 幂等关联：把「事件发生前已独立保存、尚未关联」的额度观察补挂到事件上。
    /// 只更新 event_id IS NULL 的行，重复执行不产生重复记录；相关等级由调用方逐条重算。
    pub fn quota_reset_observations_pending_link(
        &self,
        since_ms: i64,
    ) -> Result<Vec<QuotaResetObservationRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, account_id, source_id, capability_id, previous_snapshot_id, current_snapshot_id,
                        classification, observed_at, event_id, temporal_correlation, user_confirmed_at
                 FROM quota_reset_observations
                 WHERE event_id IS NULL AND observed_at >= ?1
                 ORDER BY observed_at ASC",
            )
            .map_err(|err| format!("准备待关联观察查询失败: {err}"))?;
        let rows = statement
            .query_map(params![since_ms], map_quota_reset_observation)
            .map_err(|err| format!("读取待关联观察失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取待关联观察失败: {err}"))
    }

    pub fn set_quota_reset_observation_event(
        &self,
        observation_id: i64,
        event_id: &str,
        correlation: &str,
    ) -> Result<(), String> {
        let connection = self.connect()?;
        connection
            .execute(
                "UPDATE quota_reset_observations SET event_id = ?2, temporal_correlation = ?3
                 WHERE id = ?1 AND event_id IS NULL",
                params![observation_id, event_id, correlation],
            )
            .map(|_| ())
            .map_err(|err| format!("关联额度观察失败: {err}"))
    }

    /// 幂等关联：把事件窗口内尚未关联的重置卡到账观察（数量增加）补挂到事件。
    pub fn observation_pair_is_contiguous(&self, previous: i64, current: i64) -> Result<bool,String> {
        self.connect()?.query_row("SELECT EXISTS(SELECT 1 FROM capability_snapshots p JOIN capability_snapshots c ON c.id=?2
            WHERE p.id=?1 AND p.source_id=c.source_id AND p.account_id=c.account_id AND p.capability_id=c.capability_id
            AND p.window_seconds IS c.window_seconds AND c.captured_at>p.captured_at AND c.captured_at-p.captured_at<=7200000
            AND NOT EXISTS(SELECT 1 FROM capability_snapshots gap WHERE gap.source_id=p.source_id AND gap.capability_id=p.capability_id AND gap.captured_at>p.captured_at AND gap.captured_at<c.captured_at))",params![previous,current],|r|r.get(0)).map_err(|e|e.to_string())
    }

    pub fn link_banked_grant_observations(
        &self,
        event_id: &str,
        since_ms: i64,
    ) -> Result<usize, String> {
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "UPDATE banked_reset_observations SET event_id = ?1
                 WHERE event_id IS NULL AND current_count > previous_count AND observed_at >= ?2",
                params![event_id, since_ms],
            )
            .map_err(|err| format!("关联重置卡观察失败: {err}"))?;
        Ok(changed)
    }

    /// 雷达来源（codexradar / willcodex）各自的同步结果；失败与成功分别记录。
    pub fn upsert_radar_source_status(
        &self,
        source_id: &str,
        success: bool,
        error: Option<&str>,
        post_count: Option<i64>,
    ) -> Result<(), String> {
        let connection = self.connect()?;
        let now = epoch_ms();
        connection
            .execute(
                "INSERT INTO radar_source_status(source_id, last_check_at, last_success_at, last_error, last_post_count, updated_at)
                 VALUES (?1, ?2, CASE WHEN ?3 THEN ?2 ELSE NULL END, ?4, ?5, ?2)
                 ON CONFLICT(source_id) DO UPDATE SET
                    last_check_at = excluded.last_check_at,
                    last_success_at = CASE WHEN ?3 THEN excluded.last_check_at ELSE radar_source_status.last_success_at END,
                    last_error = excluded.last_error,
                    last_post_count = CASE WHEN ?3 THEN excluded.last_post_count ELSE radar_source_status.last_post_count END,
                    updated_at = excluded.updated_at",
                params![source_id, now, i64::from(success), error, post_count],
            )
            .map(|_| ())
            .map_err(|err| format!("保存雷达来源状态失败: {err}"))
    }

    pub fn radar_source_statuses(&self) -> Result<Vec<RadarSourceStatusRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT source_id, last_check_at, last_success_at, last_error, last_post_count
                 FROM radar_source_status ORDER BY source_id",
            )
            .map_err(|err| format!("准备雷达来源状态查询失败: {err}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok(RadarSourceStatusRecord {
                    source_id: row.get(0)?,
                    last_check_at: row.get(1)?,
                    last_success_at: row.get(2)?,
                    last_error: row.get(3)?,
                    last_post_count: row.get(4)?,
                })
            })
            .map_err(|err| format!("查询雷达来源状态失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取雷达来源状态失败: {err}"))
    }

    /// 指定能力的历史快照（含 id），按捕获时间升序；观察器在代码里取相邻可解析对。
    pub fn capability_value_samples(
        &self,
        source_id: &str,
        capability_id: &str,
        since: i64,
    ) -> Result<Vec<CapabilityValueSample>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, captured_at, primary_value FROM capability_snapshots
                 WHERE source_id = ?1 AND capability_id = ?2 AND captured_at >= ?3
                 ORDER BY captured_at ASC, id ASC",
            )
            .map_err(|err| format!("准备能力快照样本查询失败: {err}"))?;
        let rows = statement
            .query_map(params![source_id, capability_id, since], |row| {
                Ok(CapabilityValueSample {
                    id: row.get(0)?,
                    captured_at: row.get(1)?,
                    primary_value: row.get(2)?,
                })
            })
            .map_err(|err| format!("读取能力快照样本失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取能力快照样本失败: {err}"))
    }

    pub fn insert_banked_reset_observation(
        &self,
        observation: &BankedResetObservationRecord,
    ) -> Result<i64, String> {
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT OR IGNORE INTO banked_reset_observations(
                    account_id, source_id, previous_snapshot_id, current_snapshot_id,
                    previous_count, current_count, observed_at, event_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    observation.account_id,
                    observation.source_id,
                    observation.previous_snapshot_id,
                    observation.current_snapshot_id,
                    observation.previous_count,
                    observation.current_count,
                    observation.observed_at,
                    observation.event_id,
                    epoch_ms(),
                ],
            )
            .map_err(|err| format!("保存重置卡发放观察失败: {err}"))?;
        connection
            .query_row(
                "SELECT id FROM banked_reset_observations
                 WHERE source_id = ?1 AND previous_snapshot_id = ?2 AND current_snapshot_id = ?3",
                params![
                    observation.source_id,
                    observation.previous_snapshot_id,
                    observation.current_snapshot_id,
                ],
                |row| row.get(0),
            )
            .map_err(|err| format!("读取重置卡发放观察失败: {err}"))
    }

    pub fn banked_reset_observations(
        &self,
        source_id: Option<&str>,
    ) -> Result<Vec<BankedResetObservationRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, account_id, source_id, previous_snapshot_id, current_snapshot_id,
                        previous_count, current_count, observed_at, event_id
                 FROM banked_reset_observations
                 WHERE (?1 IS NULL OR source_id = ?1)
                 ORDER BY observed_at DESC, id DESC",
            )
            .map_err(|err| format!("准备重置卡发放观察查询失败: {err}"))?;
        let rows = statement
            .query_map(params![source_id], map_banked_reset_observation)
            .map_err(|err| format!("读取重置卡发放观察失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取重置卡发放观察失败: {err}"))
    }

    /// 事件关联的重置卡数量观察（历史记录页）。
    pub fn banked_reset_observations_for_event(
        &self,
        event_id: &str,
    ) -> Result<Vec<BankedResetObservationRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, account_id, source_id, previous_snapshot_id, current_snapshot_id,
                        previous_count, current_count, observed_at, event_id
                 FROM banked_reset_observations
                 WHERE event_id = ?1
                 ORDER BY observed_at ASC, id ASC",
            )
            .map_err(|err| format!("准备事件重置卡观察查询失败: {err}"))?;
        let rows = statement
            .query_map(params![event_id], map_banked_reset_observation)
            .map_err(|err| format!("读取事件重置卡观察失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取事件重置卡观察失败: {err}"))
    }
}

/// radar_analyses 全列 SELECT；各查询只差异 WHERE/ORDER 子句。
fn radar_analysis_select(suffix: &str) -> String {
    format!(
        "SELECT id, created_at, range_key, cut_post_id, from_posted_at, to_posted_at, source_id, model, prompt_version, input_hash, conclusion, analysis_basis, confidence, citations_json, support_json, against_json, uncertainty_json, new_post_ids_json, event_context_post_ids_json, historical_post_ids_json, error_message, event_id, analysis_mode, context_hash, prompt_hash, event_relation, event_phase, delta_effect, signal_level, context_status, temporal_phase, valid_until, state_revision, timezone_policy_version, signal_type
         FROM radar_analyses {suffix}"
    )
}

fn radar_event_select(suffix: &str) -> String {
    format!(
        "SELECT id, phase, title, summary, first_signal_at, latest_evidence_at, claimed_landed_at, observed_reset_at, closed_at, close_reason, expected_at, expires_at, state_revision, user_confirmed_reset_at, event_type
         FROM radar_events {suffix}"
    )
}

fn map_snapshot_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<SnapshotRecord> {
    let trend_json: String = row.get(6)?;
    let trend = serde_json::from_str::<Vec<StoredTrendPointDto>>(&trend_json)
        .unwrap_or_default()
        .into_iter()
        .map(|point| StoredTrendPoint {
            label: point.label,
            value: point.value,
        })
        .collect();
    Ok(SnapshotRecord {
        capability_id: row.get(0)?,
        display_name: row.get(1)?,
        value_kind: row.get(2)?,
        primary_value: row.get(3)?,
        secondary_value: row.get(4)?,
        progress: row.get(5)?,
        trend,
        captured_at: row.get(7)?,
        window_seconds: row.get(8)?,
        reset_at: row.get(9)?,
    })
}

fn map_tibo_post(row: &rusqlite::Row<'_>) -> rusqlite::Result<TiboPostRecord> {
    Ok(TiboPostRecord {
        id: row.get(0)?,
        url: row.get(1)?,
        text: row.get(2)?,
        posted_at: row.get(3)?,
        kind: row.get(4)?,
        tibo_lane: row.get(5)?,
        explicit_reset: row.get::<_, i64>(6)? != 0,
        verification_status: row.get(7)?,
        is_reply: row.get::<_, i64>(8)? != 0,
        replies: row.get(9)?,
        reposts: row.get(10)?,
        likes: row.get(11)?,
        extra_json: row.get(12)?,
        synced_at: row.get(13)?,
        translated_text: row.get(14)?,
        translated_at: row.get(15)?,
        translation_source: row.get(16)?,
        lifecycle_consumed_at: row.get(17)?,
    })
}

fn map_radar_analysis(row: &rusqlite::Row<'_>) -> rusqlite::Result<RadarAnalysisRecord> {
    Ok(RadarAnalysisRecord {
        id: row.get(0)?,
        created_at: row.get(1)?,
        range_key: row.get(2)?,
        cut_post_id: row.get(3)?,
        from_posted_at: row.get(4)?,
        to_posted_at: row.get(5)?,
        source_id: row.get(6)?,
        model: row.get(7)?,
        prompt_version: row.get(8)?,
        input_hash: row.get(9)?,
        conclusion: row.get(10)?,
        analysis_basis: row.get(11)?,
        confidence: row.get(12)?,
        citations_json: row.get(13)?,
        support_json: row.get(14)?,
        against_json: row.get(15)?,
        uncertainty_json: row.get(16)?,
        new_post_ids_json: row.get(17)?,
        event_context_post_ids_json: row.get(18)?,
        historical_post_ids_json: row.get(19)?,
        error_message: row.get(20)?,
        event_id: row.get(21)?,
        analysis_mode: row.get(22)?,
        context_hash: row.get(23)?,
        prompt_hash: row.get(24)?,
        event_relation: row.get(25)?,
        event_phase: row.get(26)?,
        delta_effect: row.get(27)?,
        signal_level: row.get(28)?,
        context_status: row.get(29)?,
        temporal_phase: row.get(30)?,
        valid_until: row.get(31)?,
        state_revision: row.get(32)?,
        timezone_policy_version: row.get(33)?,
        signal_type: row.get(34)?,
    })
}

fn map_radar_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<RadarEventRecord> {
    Ok(RadarEventRecord {
        id: row.get(0)?,
        phase: row.get(1)?,
        title: row.get(2)?,
        summary: row.get(3)?,
        first_signal_at: row.get(4)?,
        latest_evidence_at: row.get(5)?,
        claimed_landed_at: row.get(6)?,
        observed_reset_at: row.get(7)?,
        closed_at: row.get(8)?,
        close_reason: row.get(9)?,
        expected_at: row.get(10)?,
        expires_at: row.get(11)?,
        state_revision: row.get(12)?,
        user_confirmed_reset_at: row.get(13)?,
        event_type: row.get(14)?,
    })
}

fn map_quota_reset_observation(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<QuotaResetObservationRecord> {
    Ok(QuotaResetObservationRecord {
        id: row.get(0)?,
        account_id: row.get(1)?,
        source_id: row.get(2)?,
        capability_id: row.get(3)?,
        previous_snapshot_id: row.get(4)?,
        current_snapshot_id: row.get(5)?,
        classification: row.get(6)?,
        observed_at: row.get(7)?,
        event_id: row.get(8)?,
        temporal_correlation: row.get(9)?,
        user_confirmed_at: row.get(10)?,
    })
}

fn map_banked_reset_observation(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<BankedResetObservationRecord> {
    Ok(BankedResetObservationRecord {
        id: row.get(0)?,
        account_id: row.get(1)?,
        source_id: row.get(2)?,
        previous_snapshot_id: row.get(3)?,
        current_snapshot_id: row.get(4)?,
        previous_count: row.get(5)?,
        current_count: row.get(6)?,
        observed_at: row.get(7)?,
        event_id: row.get(8)?,
    })
}

fn map_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<SourceRecord> {
    Ok(SourceRecord {
        id: row.get(0)?,
        account_id: row.get(1)?,
        account_name: row.get(2)?,
        account_kind: row.get(3)?,
        platform_id: row.get(4)?,
        adapter_id: row.get(5)?,
        source_type: row.get(6)?,
        display_name: row.get(7)?,
        secret_ref: row.get(8)?,
        state: row.get(9)?,
        last_validated_at: row.get(10)?,
        last_success_at: row.get(11)?,
        error_code: row.get(12)?,
        error_message: row.get(13)?,
    })
}

fn epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// 解析窗口快照主值中的剩余百分比（如 "62.5%" → 62.5）；非百分比文本返回 None。
fn parse_percent_value(primary: &str) -> Option<f64> {
    let text = primary.trim().trim_end_matches('%').trim();
    text.parse::<f64>().ok().filter(|value| value.is_finite())
}

/// 解析金额快照主值（如 "¥3.50"、"$1,299.00" → 数值）；仅用于图表展示，不做汇总。
fn parse_money_value(primary: &str) -> Option<f64> {
    let text: String = primary
        .trim()
        .trim_start_matches(['¥', '$'])
        .chars()
        .filter(|ch| ch.is_ascii_digit() || *ch == '.')
        .collect();
    text.parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::Database;

    #[test]
    fn remove_user_platform_deletes_catalog_row() {
        let path = std::env::temp_dir().join(format!(
            "ai-quota-monitor-remove-{}-{}.db",
            std::process::id(),
            epoch_ms()
        ));
        let database = Database::initialize_at(path.clone()).expect("db");
        database
            .ensure_account_source(
                "glm-default",
                "glm",
                "glm-coding-plan",
                "api_key",
                "Coding Plan",
                "默认账户",
            )
            .expect("source");
        database
            .add_user_platform("glm", "GLM 国内", Some("https://open.bigmodel.cn"))
            .expect("add");
        assert!(database.user_platform("glm").expect("read").is_some());
        database.remove_user_platform("glm").expect("remove");
        assert!(database.user_platform("glm").expect("read").is_none());
        let _ = std::fs::remove_file(path);
    }
}
