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

#[derive(Debug, Clone)]
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
    pub error_message: Option<String>,
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
        if changed == 0 { Err("未找到该账户".into()) } else { Ok(()) }
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
            if matches!(source.account_id.as_str(), "openai-codex-local" | "deepseek-default") {
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

    pub fn add_user_platform(&self, platform_id: &str, display_name: &str, api_base_url: Option<&str>) -> Result<(), String> {
        let connection = self.connect()?;
        let now = epoch_ms();
        let next_index: i64 = connection
            .query_row("SELECT COALESCE(MAX(sort_index), -1) + 1 FROM user_platforms", [], |row| row.get(0))
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

    pub fn save_user_platform_api_base(&self, platform_id: &str, api_base_url: Option<&str>) -> Result<(), String> {
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

    pub fn begin_refresh_run(&self, platform_id: &str, source_ids: &[String]) -> Result<String, String> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction()
            .map_err(|err| format!("开始刷新流水失败: {err}"))?;
        let now = epoch_ms();
        let run_id = format!("{platform_id}-{now}-{}", RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed));
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
                if error.auth_required { "auth_required" } else { "error" },
                Some(error.code.as_str()),
                Some(error.message.as_str()),
                if output.capabilities.is_empty() { "failed" } else { "partial" },
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
                    "INSERT INTO capability_snapshots(account_id, source_id, capability_id, display_name, value_kind, primary_value, secondary_value, progress, trend_json, captured_at, generation)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
        let status = if failed > 0 || partial > 0 { "partial" } else { "success" };
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
        if changed == 0 { Err("未找到数据来源".into()) } else { Ok(()) }
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
        if changed == 0 { Err("未找到数据来源".into()) } else { Ok(()) }
    }

    pub fn latest_snapshot(&self, source_id: &str, capability_id: &str) -> Result<Option<SnapshotRecord>, String> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT capability_id, display_name, value_kind, primary_value, secondary_value, progress, trend_json, captured_at
                 FROM capability_snapshots WHERE source_id = ?1 AND capability_id = ?2
                 ORDER BY captured_at DESC, id DESC LIMIT 1",
                params![source_id, capability_id],
                |row| {
                    let trend_json: String = row.get(6)?;
                    let trend = serde_json::from_str::<Vec<StoredTrendPointDto>>(&trend_json)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|point| StoredTrendPoint { label: point.label, value: point.value })
                        .collect();
                    Ok(SnapshotRecord {
                        capability_id: row.get(0)?, display_name: row.get(1)?, value_kind: row.get(2)?,
                        primary_value: row.get(3)?, secondary_value: row.get(4)?, progress: row.get(5)?,
                        trend, captured_at: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(|err| format!("读取能力快照失败: {err}"))
    }

    pub fn latest_window_snapshots(&self, source_id: &str) -> Result<Vec<SnapshotRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT c.capability_id, c.display_name, c.value_kind, c.primary_value, c.secondary_value,
                        c.progress, c.trend_json, c.captured_at
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
            .query_map(params![source_id], |row| {
                let trend_json: String = row.get(6)?;
                let trend = serde_json::from_str::<Vec<StoredTrendPointDto>>(&trend_json)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|point| StoredTrendPoint { label: point.label, value: point.value })
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
                })
            })
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
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?))
            })
            .map_err(|err| format!("读取每日趋势失败: {err}"))?;
        // 同一本地自然日取最大 id（最后一次写入），缺失日自然缺席
        let mut by_day: std::collections::BTreeMap<chrono::NaiveDate, (i64, f64)> =
            std::collections::BTreeMap::new();
        for row in rows {
            let (id, captured_at, primary) = row.map_err(|err| format!("读取每日趋势失败: {err}"))?;
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

    pub fn refresh_history(&self, platform_id: &str, limit: usize) -> Result<Vec<RefreshHistoryRecord>, String> {
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
                    id: row.get(0)?, source_id: row.get(1)?, source_name: row.get(2)?,
                    account_id: row.get(3)?, account_name: row.get(4)?, status: row.get(5)?,
                    finished_at: row.get(6)?, error_message: row.get(7)?,
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

    pub fn replace_tibo_posts(&self, posts: &[TiboPostRecord], synced_at: i64) -> Result<(), String> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction()
            .map_err(|err| format!("开始保存雷达动态失败: {err}"))?;
        for post in posts {
            transaction
                .execute(
                    "INSERT INTO tibo_posts(id, url, text, posted_at, kind, tibo_lane, explicit_reset, verification_status, is_reply, replies, reposts, likes, extra_json, synced_at, translated_text, translated_at, translation_source)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
                     ON CONFLICT(id) DO UPDATE SET
                        url = excluded.url, text = excluded.text, posted_at = excluded.posted_at, kind = excluded.kind,
                        tibo_lane = excluded.tibo_lane, explicit_reset = excluded.explicit_reset,
                        verification_status = excluded.verification_status, is_reply = excluded.is_reply,
                        replies = excluded.replies, reposts = excluded.reposts, likes = excluded.likes,
                        extra_json = excluded.extra_json, synced_at = excluded.synced_at,
                        translated_text = COALESCE(excluded.translated_text, tibo_posts.translated_text),
                        translated_at = COALESCE(excluded.translated_at, tibo_posts.translated_at),
                        translation_source = COALESCE(excluded.translation_source, tibo_posts.translation_source)",
                    params![
                        post.id, post.url, post.text, post.posted_at, post.kind, post.tibo_lane,
                        i64::from(post.explicit_reset), post.verification_status, i64::from(post.is_reply),
                        post.replies, post.reposts, post.likes, post.extra_json, synced_at,
                        post.translated_text, post.translated_at, post.translation_source
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
                "SELECT id, url, text, posted_at, kind, tibo_lane, explicit_reset, verification_status, is_reply, replies, reposts, likes, extra_json, synced_at, translated_text, translated_at, translation_source
                 FROM tibo_posts ORDER BY posted_at DESC LIMIT ?1",
            )
            .map_err(|err| format!("准备雷达动态查询失败: {err}"))?;
        let rows = statement
            .query_map(params![limit as i64], map_tibo_post)
            .map_err(|err| format!("查询雷达动态失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取雷达动态失败: {err}"))
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

    pub fn latest_radar_analysis(&self) -> Result<Option<RadarAnalysisRecord>, String> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT id, created_at, range_key, cut_post_id, from_posted_at, to_posted_at, source_id, model, prompt_version, input_hash, conclusion, analysis_basis, confidence, citations_json, support_json, against_json, uncertainty_json, error_message
                 FROM radar_analyses WHERE error_message IS NULL ORDER BY created_at DESC LIMIT 1",
                [],
                map_radar_analysis,
            )
            .optional()
            .map_err(|err| format!("读取雷达分析失败: {err}"))
    }

    pub fn insert_radar_analysis(&self, analysis: &RadarAnalysisRecord) -> Result<(), String> {
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO radar_analyses(id, created_at, range_key, cut_post_id, from_posted_at, to_posted_at, source_id, model, prompt_version, input_hash, conclusion, analysis_basis, confidence, citations_json, support_json, against_json, uncertainty_json, error_message)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
                params![
                    analysis.id, analysis.created_at, analysis.range_key, analysis.cut_post_id,
                    analysis.from_posted_at, analysis.to_posted_at, analysis.source_id, analysis.model,
                    analysis.prompt_version, analysis.input_hash, analysis.conclusion, analysis.analysis_basis, analysis.confidence,
                    analysis.citations_json, analysis.support_json, analysis.against_json,
                    analysis.uncertainty_json, analysis.error_message
                ],
            )
            .map(|_| ())
            .map_err(|err| format!("保存雷达分析失败: {err}"))
    }
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
        error_message: row.get(17)?,
    })
}

fn map_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<SourceRecord> {
    Ok(SourceRecord {
        id: row.get(0)?, account_id: row.get(1)?, account_name: row.get(2)?, account_kind: row.get(3)?,
        platform_id: row.get(4)?, adapter_id: row.get(5)?, source_type: row.get(6)?,
        display_name: row.get(7)?, secret_ref: row.get(8)?, state: row.get(9)?,
        last_validated_at: row.get(10)?, last_success_at: row.get(11)?, error_code: row.get(12)?, error_message: row.get(13)?,
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
    text.parse::<f64>().ok().filter(|value| value.is_finite() && *value >= 0.0)
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
            .ensure_account_source("glm-default", "glm", "glm-coding-plan", "api_key", "Coding Plan", "默认账户")
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
