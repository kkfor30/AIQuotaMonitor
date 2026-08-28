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
    pub platform_id: String,
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
pub struct RefreshHistoryRecord {
    pub id: String,
    pub source_id: String,
    pub source_name: String,
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
        let connection = self.connect()?;
        let now = epoch_ms();
        connection
            .execute(
                "INSERT OR IGNORE INTO accounts(id, platform_id, display_name, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                params![account_id, platform_id, account_name, now],
            )
            .map_err(|err| format!("初始化账户失败: {err}"))?;
        connection
            .execute(
                "INSERT OR IGNORE INTO sources(id, account_id, source_type, display_name, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![source_id, account_id, source_type, source_name, now],
            )
            .map_err(|err| format!("初始化来源失败: {err}"))?;
        Ok(())
    }

    pub fn delete_account(&self, account_id: &str) -> Result<(), String> {
        if matches!(account_id, "openai-codex-local" | "deepseek-default") {
            return Err("不能删除默认账户".into());
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
                "SELECT s.id, s.account_id, a.platform_id, s.source_type, s.display_name, s.secret_ref, s.state, s.last_validated_at, s.last_success_at, s.error_code, s.error_message
                 FROM sources s JOIN accounts a ON a.id = s.account_id
                 WHERE a.platform_id = ?1 ORDER BY s.created_at, s.id",
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
                "SELECT s.id, s.account_id, a.platform_id, s.source_type, s.display_name, s.secret_ref, s.state, s.last_validated_at, s.last_success_at, s.error_code, s.error_message
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

    pub fn refresh_history(&self, platform_id: &str, limit: usize) -> Result<Vec<RefreshHistoryRecord>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT rr.run_id || ':' || rr.source_id, rr.source_id, s.display_name, rr.status, rr.finished_at, rr.error_message
                 FROM refresh_results rr JOIN refresh_runs r ON r.id = rr.run_id JOIN sources s ON s.id = rr.source_id
                 WHERE r.platform_id = ?1 ORDER BY rr.started_at DESC, rr.id DESC LIMIT ?2",
            )
            .map_err(|err| format!("准备刷新历史查询失败: {err}"))?;
        let rows = statement
            .query_map(params![platform_id, limit as i64], |row| {
                Ok(RefreshHistoryRecord {
                    id: row.get(0)?, source_id: row.get(1)?, source_name: row.get(2)?, status: row.get(3)?,
                    finished_at: row.get(4)?, error_message: row.get(5)?,
                })
            })
            .map_err(|err| format!("查询刷新历史失败: {err}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("读取刷新历史失败: {err}"))
    }
}

fn map_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<SourceRecord> {
    Ok(SourceRecord {
        id: row.get(0)?, account_id: row.get(1)?, platform_id: row.get(2)?, source_type: row.get(3)?,
        display_name: row.get(4)?, secret_ref: row.get(5)?, state: row.get(6)?,
        last_validated_at: row.get(7)?, last_success_at: row.get(8)?, error_code: row.get(9)?, error_message: row.get(10)?,
    })
}

fn epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
