//! SQLite v1 schema 与迁移入口。
//!
//! 迁移机制参考 cc-switch（审计基线 6243e20a）的版本表与升级前备份思路，
//! 业务表按 AIQuotaMonitor 的 Account → Source → Capability → Snapshot 重新设计。

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::ops::{Deref, DerefMut};
use tauri::{AppHandle, Manager};

const CURRENT_SCHEMA_VERSION: i64 = 18;

#[derive(Debug, Clone)]
pub struct Database {
    path: PathBuf,
    session: Option<Arc<Mutex<Connection>>>,
}

pub enum DatabaseConnection<'a> {
    Owned(Connection),
    Session(MutexGuard<'a, Connection>),
}
impl Deref for DatabaseConnection<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection { match self { Self::Owned(c) => c, Self::Session(c) => c } }
}
impl DerefMut for DatabaseConnection<'_> {
    fn deref_mut(&mut self) -> &mut Connection { match self { Self::Owned(c) => c, Self::Session(c) => c } }
}

impl Database {
    pub fn initialize(app: &AppHandle) -> Result<Self, String> {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|err| format!("无法定位应用数据目录: {err}"))?;
        fs::create_dir_all(&data_dir).map_err(|err| format!("无法创建应用数据目录: {err}"))?;
        let path = data_dir.join("ai-quota-monitor.db");
        Self::initialize_at(path)
    }

    pub fn initialize_at(path: PathBuf) -> Result<Self, String> {
        let previous_version = read_schema_version(&path)?;
        if previous_version > 0 && previous_version < CURRENT_SCHEMA_VERSION {
            backup_before_upgrade(&path, previous_version)?;
        }

        let mut connection = open_connection(&path)?;
        migrate(&mut connection, previous_version)?;
        seed_platform_sources(&mut connection)?;
        Ok(Self { path, session: None })
    }

    pub fn connect(&self) -> Result<DatabaseConnection<'_>, String> {
        match &self.session {
            Some(session) => session.lock().map(DatabaseConnection::Session).map_err(|_| "数据库事务已中断".into()),
            None => open_connection(&self.path).map(DatabaseConnection::Owned),
        }
    }

    pub(crate) fn in_transaction(&self) -> bool { self.session.is_some() }

    /// 一次 ViewModel 读取复用一个短生命周期连接；WAL 读快照不占用写事务锁。
    /// query_only 防止聚合读取意外写库，结束后释放连接和页缓存。
    pub(crate) fn read_snapshot<T>(&self, read: impl FnOnce(&Database) -> Result<T, String>) -> Result<T, String> {
        if self.in_transaction() { return read(self); }
        let connection = open_connection(&self.path)?;
        connection.execute_batch("PRAGMA query_only = ON; BEGIN DEFERRED")
            .map_err(|error| format!("开始数据库快照读取失败: {error}"))?;
        let scoped = Self { path: self.path.clone(), session: Some(Arc::new(Mutex::new(connection))) };
        let value = read(&scoped)?;
        scoped.connect()?.execute_batch("COMMIT")
            .map_err(|error| format!("结束数据库快照读取失败: {error}"))?;
        Ok(value)
    }

    /// A synchronous unit of work: every repository call uses the same connection.
    /// Dropping the scoped connection on an error/panic rolls back SQLite's transaction.
    pub(crate) fn atomic<T>(&self, work: impl FnOnce(&Database) -> Result<T, String>) -> Result<T, String> {
        if self.in_transaction() { return work(self); }
        let connection = open_connection(&self.path)?;
        connection.execute_batch("BEGIN IMMEDIATE").map_err(|e| e.to_string())?;
        let scoped = Self { path: self.path.clone(), session: Some(Arc::new(Mutex::new(connection))) };
        match work(&scoped) {
            Ok(value) => {
                scoped.connect()?.execute_batch("COMMIT").map_err(|e| e.to_string())?;
                Ok(value)
            }
            Err(error) => {
                let _ = scoped.connect()?.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn open_connection(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path).map_err(|err| format!("打开 SQLite 失败: {err}"))?;
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;\nPRAGMA journal_mode = WAL;\nPRAGMA synchronous = NORMAL;\nPRAGMA busy_timeout = 5000;",
        )
        .map_err(|err| format!("初始化 SQLite 会话失败: {err}"))?;
    Ok(connection)
}

fn read_schema_version(path: &Path) -> Result<i64, String> {
    if !path.exists() {
        return Ok(0);
    }
    let connection =
        Connection::open(path).map_err(|err| format!("读取 SQLite 版本失败: {err}"))?;
    let table_exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations')",
            [],
            |row| row.get(0),
        )
        .map_err(|err| format!("检查 SQLite 版本表失败: {err}"))?;
    if !table_exists {
        return Ok(0);
    }
    connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get::<_, Option<i64>>(0)
        })
        .optional()
        .map_err(|err| format!("读取 SQLite schema 版本失败: {err}"))
        .map(|value| value.flatten().unwrap_or(0))
}

/// SQLite online backup captures committed WAL data consistently; verify before migrating.
fn backup_before_upgrade(path: &Path, version: i64) -> Result<(), String> {
    let backup = path.with_extension(format!("v{version}.bak"));
    let source = Connection::open(path).map_err(|e| e.to_string())?;
    let mut target = Connection::open(&backup).map_err(|e| e.to_string())?;
    rusqlite::backup::Backup::new(&source, &mut target)
        .and_then(|backup| backup.run_to_completion(128, std::time::Duration::from_millis(10), None))
        .map_err(|err| format!("升级前备份 SQLite 失败: {err}"))?;
    let integrity: String = target.query_row("PRAGMA integrity_check", [], |r|r.get(0)).map_err(|e|e.to_string())?;
    if integrity != "ok" { return Err(format!("数据库备份校验失败：{integrity}")); }
    Ok(())
}

fn migrate(connection: &mut Connection, previous_version: i64) -> Result<(), String> {
    if previous_version > CURRENT_SCHEMA_VERSION {
        return Err(format!(
            "数据库版本 {previous_version} 高于当前支持版本 {CURRENT_SCHEMA_VERSION}"
        ));
    }
    if previous_version == CURRENT_SCHEMA_VERSION {
        return Ok(());
    }

    let transaction = connection
        .transaction()
        .map_err(|err| format!("开始 SQLite 迁移失败: {err}"))?;
    if previous_version < 1 {
        migrate_v1(&transaction)?;
    }
    if previous_version < 2 {
        migrate_v2(&transaction)?;
    }
    if previous_version < 3 {
        migrate_v3(&transaction)?;
    }
    if previous_version < 4 {
        migrate_v4(&transaction)?;
    }
    if previous_version < 5 {
        migrate_v5(&transaction)?;
    }
    if previous_version < 6 {
        migrate_v6(&transaction)?;
    }
    if previous_version < 7 {
        migrate_v7(&transaction)?;
    }
    if previous_version < 8 {
        migrate_v8(&transaction)?;
    }
    if previous_version < 9 {
        migrate_v9(&transaction)?;
    }
    if previous_version < 10 {
        migrate_v10(&transaction)?;
    }
    if previous_version < 11 {
        migrate_v11(&transaction)?;
    }
    if previous_version < 12 {
        migrate_v12(&transaction)?;
    }
    if previous_version < 13 {
        migrate_v13(&transaction)?;
    }
    if previous_version < 14 {
        migrate_v14(&transaction)?;
    }
    if previous_version < 15 {
        migrate_v15(&transaction)?;
    }
    if previous_version < 16 {
        transaction.execute_batch(r#"
            ALTER TABLE tibo_posts ADD COLUMN material_version INTEGER NOT NULL DEFAULT 1;
            ALTER TABLE tibo_posts ADD COLUMN published_at INTEGER;
            UPDATE tibo_posts SET published_at = CASE WHEN posted_at > 0 THEN posted_at ELSE NULL END;
            CREATE TABLE radar_material_versions (
                post_id TEXT NOT NULL, version INTEGER NOT NULL, input_json TEXT NOT NULL,
                created_at INTEGER NOT NULL, PRIMARY KEY(post_id,version)
            );
            CREATE TABLE radar_applications (
                analysis_id TEXT NOT NULL, post_id TEXT NOT NULL, material_version INTEGER NOT NULL,
                event_id TEXT, outcome TEXT NOT NULL, reason TEXT NOT NULL,
                created_at INTEGER NOT NULL, PRIMARY KEY(analysis_id,post_id,material_version)
            );
            CREATE TABLE radar_analysis_events (
                analysis_id TEXT NOT NULL, event_id TEXT NOT NULL,
                PRIMARY KEY(analysis_id,event_id)
            );
            CREATE TABLE radar_transitions (
                id INTEGER PRIMARY KEY AUTOINCREMENT, event_id TEXT NOT NULL,
                at INTEGER NOT NULL, before_json TEXT NOT NULL, after_json TEXT NOT NULL,
                reason TEXT NOT NULL
            );
            CREATE TABLE radar_analysis_inputs (
                analysis_id TEXT PRIMARY KEY, input_json TEXT NOT NULL, result_json TEXT NOT NULL
            );
            CREATE TABLE radar_prepared_analyses (cache_key TEXT PRIMARY KEY, record_json TEXT NOT NULL, input_json TEXT NOT NULL, result_json TEXT NOT NULL);
            CREATE TABLE radar_repairs (repair_key TEXT PRIMARY KEY, at INTEGER NOT NULL, detail TEXT NOT NULL);
            CREATE TABLE radar_event_time_basis (
                event_id TEXT PRIMARY KEY, post_id TEXT, raw_text TEXT,
                precision TEXT NOT NULL, timezone_kind TEXT, timezone_assumed INTEGER NOT NULL DEFAULT 0
            );
            INSERT INTO radar_material_versions
                SELECT id,1,json_object('legacy',1,'text',text,'postedAt',published_at,'context',json_extract(extra_json,'$.context')),synced_at FROM tibo_posts;
            CREATE TRIGGER radar_material_insert AFTER INSERT ON tibo_posts BEGIN
                UPDATE tibo_posts SET published_at=CASE WHEN NEW.posted_at>0 THEN NEW.posted_at ELSE NULL END WHERE id=NEW.id;
                INSERT INTO radar_material_versions VALUES(NEW.id,NEW.material_version,json_object('text',NEW.text,'postedAt',CASE WHEN NEW.posted_at>0 THEN NEW.posted_at ELSE NULL END,'context',json_extract(NEW.extra_json,'$.context')),NEW.synced_at);
            END;
            CREATE TRIGGER radar_material_update AFTER UPDATE OF text,posted_at,extra_json ON tibo_posts
            WHEN NEW.text IS NOT OLD.text OR NEW.posted_at IS NOT OLD.posted_at OR json_extract(NEW.extra_json,'$.context') IS NOT json_extract(OLD.extra_json,'$.context') BEGIN
                UPDATE tibo_posts SET material_version=OLD.material_version+1,lifecycle_consumed_at=NULL,
                  published_at=CASE WHEN NEW.posted_at>0 THEN NEW.posted_at ELSE NULL END WHERE id=NEW.id;
                INSERT INTO radar_material_versions VALUES(NEW.id,OLD.material_version+1,json_object('text',NEW.text,'postedAt',CASE WHEN NEW.posted_at>0 THEN NEW.posted_at ELSE NULL END,'context',json_extract(NEW.extra_json,'$.context')),NEW.synced_at);
            END;
            INSERT INTO radar_transitions(event_id,at,before_json,after_json,reason)
                SELECT id,updated_at,json_object('phase',phase),json_object('phase','closed'),'legacy_closed_phase_repair'
                FROM radar_events WHERE closed_at IS NOT NULL AND phase!='closed';
            UPDATE radar_events SET phase='closed',state_revision=state_revision+1 WHERE closed_at IS NOT NULL AND phase!='closed';
            INSERT INTO radar_analysis_events SELECT id,event_id FROM radar_analyses WHERE event_id IS NOT NULL;
            CREATE TRIGGER radar_event_transition AFTER UPDATE ON radar_events
            WHEN OLD.state_revision != NEW.state_revision BEGIN
                INSERT INTO radar_transitions(event_id,at,before_json,after_json,reason) VALUES(NEW.id,NEW.updated_at,
                json_object('phase',OLD.phase,'expectedAt',OLD.expected_at,'revision',OLD.state_revision,'observedAt',OLD.observed_reset_at,'confirmedAt',OLD.user_confirmed_reset_at),
                json_object('phase',NEW.phase,'expectedAt',NEW.expected_at,'revision',NEW.state_revision,'observedAt',NEW.observed_reset_at,'confirmedAt',NEW.user_confirmed_reset_at),
                COALESCE(NEW.close_reason,'state_update'));
            END;
            INSERT INTO schema_migrations VALUES(16,CAST(strftime('%s','now') AS INTEGER)*1000);
        "#).map_err(|e| format!("迁移雷达 v16 失败: {e}"))?;
    }
    if previous_version < 17 {
        transaction.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS radar_prepared_analyses (
                cache_key TEXT PRIMARY KEY, record_json TEXT NOT NULL,
                input_json TEXT NOT NULL, result_json TEXT NOT NULL
            );
            INSERT INTO schema_migrations VALUES(17,CAST(strftime('%s','now') AS INTEGER)*1000);
        "#).map_err(|e|format!("迁移雷达 v17 失败: {e}"))?;
    }
    if previous_version < 18 {
        transaction.execute_batch(r#"
            CREATE TRIGGER IF NOT EXISTS radar_event_transition AFTER UPDATE ON radar_events
            WHEN OLD.state_revision != NEW.state_revision BEGIN
                INSERT INTO radar_transitions(event_id,at,before_json,after_json,reason) VALUES(NEW.id,NEW.updated_at,
                json_object('phase',OLD.phase,'expectedAt',OLD.expected_at,'revision',OLD.state_revision,'observedAt',OLD.observed_reset_at,'confirmedAt',OLD.user_confirmed_reset_at),
                json_object('phase',NEW.phase,'expectedAt',NEW.expected_at,'revision',NEW.state_revision,'observedAt',NEW.observed_reset_at,'confirmedAt',NEW.user_confirmed_reset_at),
                COALESCE(NEW.close_reason,'state_update'));
            END;
            INSERT INTO schema_migrations VALUES(18,CAST(strftime('%s','now') AS INTEGER)*1000);
        "#).map_err(|e|format!("迁移雷达 v18 失败: {e}"))?;
    }
    transaction
        .commit()
        .map_err(|err| format!("提交 SQLite 迁移失败: {err}"))
}

fn migrate_v1(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            r#"
            CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at INTEGER NOT NULL
            );

            CREATE TABLE accounts (
                id TEXT PRIMARY KEY,
                platform_id TEXT NOT NULL,
                display_name TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX idx_accounts_platform ON accounts(platform_id);

            CREATE TABLE sources (
                id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                source_type TEXT NOT NULL,
                display_name TEXT NOT NULL,
                secret_ref TEXT,
                state TEXT NOT NULL DEFAULT 'auth_required',
                generation INTEGER NOT NULL DEFAULT 0,
                last_validated_at INTEGER,
                last_success_at INTEGER,
                error_code TEXT,
                error_message TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX idx_sources_account ON sources(account_id);

            CREATE TABLE capability_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
                capability_id TEXT NOT NULL,
                display_name TEXT NOT NULL,
                value_kind TEXT NOT NULL,
                primary_value TEXT,
                secondary_value TEXT,
                progress REAL,
                trend_json TEXT NOT NULL DEFAULT '[]',
                captured_at INTEGER NOT NULL,
                generation INTEGER NOT NULL
            );
            CREATE INDEX idx_snapshots_latest
                ON capability_snapshots(source_id, capability_id, captured_at DESC);

            CREATE TABLE refresh_runs (
                id TEXT PRIMARY KEY,
                platform_id TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                finished_at INTEGER,
                status TEXT NOT NULL
            );
            CREATE INDEX idx_refresh_runs_platform
                ON refresh_runs(platform_id, started_at DESC);

            CREATE TABLE refresh_results (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id TEXT NOT NULL REFERENCES refresh_runs(id) ON DELETE CASCADE,
                source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
                started_at INTEGER NOT NULL,
                finished_at INTEGER,
                status TEXT NOT NULL,
                error_code TEXT,
                error_message TEXT
            );
            CREATE INDEX idx_refresh_results_run ON refresh_results(run_id);

            CREATE TABLE settings (
                key TEXT PRIMARY KEY,
                value_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE platform_order (
                platform_id TEXT PRIMARY KEY,
                sort_index INTEGER NOT NULL
            );

            INSERT INTO schema_migrations(version, applied_at)
            VALUES (1, CAST(strftime('%s', 'now') AS INTEGER) * 1000);
            "#,
        )
        .map_err(|err| format!("执行 SQLite v1 迁移失败: {err}"))
}

fn migrate_v2(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            r#"
            CREATE TABLE user_platforms (
                platform_id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                notes TEXT NOT NULL DEFAULT '',
                api_base_url TEXT,
                sort_index INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            INSERT INTO user_platforms(platform_id, display_name, notes, api_base_url, sort_index, created_at, updated_at)
            SELECT a.platform_id,
                   CASE a.platform_id
                     WHEN 'deepseek' THEN 'DeepSeek'
                     WHEN 'openai' THEN 'GPT / Codex'
                     ELSE a.display_name
                   END,
                   '',
                   CASE a.platform_id WHEN 'deepseek' THEN 'https://api.deepseek.com' ELSE NULL END,
                   COALESCE((SELECT sort_index FROM platform_order WHERE platform_id = a.platform_id), 0),
                   a.created_at,
                   a.updated_at
            FROM accounts a
            WHERE EXISTS (
                SELECT 1 FROM sources s
                WHERE s.account_id = a.id
                  AND (s.secret_ref IS NOT NULL OR s.last_success_at IS NOT NULL)
            );
            INSERT INTO schema_migrations(version, applied_at)
            VALUES (2, CAST(strftime('%s', 'now') AS INTEGER) * 1000);
            "#,
        )
        .map_err(|err| format!("执行 SQLite v2 迁移失败: {err}"))
}

fn migrate_v3(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            r#"
            CREATE TABLE tibo_posts (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                text TEXT NOT NULL,
                posted_at INTEGER NOT NULL,
                kind TEXT NOT NULL,
                tibo_lane TEXT,
                explicit_reset INTEGER NOT NULL DEFAULT 0,
                verification_status TEXT,
                is_reply INTEGER NOT NULL DEFAULT 0,
                replies INTEGER NOT NULL DEFAULT 0,
                reposts INTEGER NOT NULL DEFAULT 0,
                likes INTEGER NOT NULL DEFAULT 0,
                extra_json TEXT NOT NULL DEFAULT '{}',
                synced_at INTEGER NOT NULL
            );
            CREATE INDEX idx_tibo_posts_time ON tibo_posts(posted_at DESC);

            CREATE TABLE radar_checks (
                id TEXT PRIMARY KEY,
                started_at INTEGER NOT NULL,
                finished_at INTEGER,
                status TEXT NOT NULL,
                sync_status TEXT,
                parse_status TEXT,
                analyze_status TEXT,
                error_message TEXT,
                post_count INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE radar_analyses (
                id TEXT PRIMARY KEY,
                created_at INTEGER NOT NULL,
                range_key TEXT NOT NULL,
                cut_post_id TEXT,
                from_posted_at INTEGER,
                to_posted_at INTEGER,
                source_id TEXT,
                model TEXT,
                prompt_version TEXT NOT NULL,
                input_hash TEXT NOT NULL,
                conclusion TEXT,
                confidence TEXT,
                citations_json TEXT NOT NULL DEFAULT '[]',
                support_json TEXT NOT NULL DEFAULT '[]',
                against_json TEXT NOT NULL DEFAULT '[]',
                uncertainty_json TEXT NOT NULL DEFAULT '[]',
                error_message TEXT
            );
            CREATE INDEX idx_radar_analyses_time ON radar_analyses(created_at DESC);

            INSERT INTO schema_migrations(version, applied_at)
            VALUES (3, CAST(strftime('%s', 'now') AS INTEGER) * 1000);
            "#,
        )
        .map_err(|err| format!("执行 SQLite v3 迁移失败: {err}"))
}

fn migrate_v4(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            r#"
            ALTER TABLE tibo_posts ADD COLUMN translated_text TEXT;
            ALTER TABLE tibo_posts ADD COLUMN translated_at INTEGER;
            ALTER TABLE tibo_posts ADD COLUMN translation_source TEXT;

            INSERT INTO schema_migrations(version, applied_at)
            VALUES (4, CAST(strftime('%s', 'now') AS INTEGER) * 1000);
            "#,
        )
        .map_err(|err| format!("执行 SQLite v4 迁移失败: {err}"))
}

fn migrate_v5(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            r#"
            ALTER TABLE accounts ADD COLUMN kind TEXT NOT NULL DEFAULT 'default';
            ALTER TABLE sources ADD COLUMN adapter_id TEXT;
            ALTER TABLE radar_analyses ADD COLUMN analysis_basis TEXT;

            UPDATE sources SET adapter_id = id WHERE adapter_id IS NULL;
            UPDATE accounts SET kind = 'local'
            WHERE id IN ('openai-codex-local', 'claude-code-default');
            UPDATE accounts SET kind = 'additional'
            WHERE id LIKE 'openai-codex-extra-%';
            UPDATE sources SET adapter_id = 'openai-codex-local'
            WHERE id LIKE 'openai-codex-extra-%';

            INSERT INTO schema_migrations(version, applied_at)
            VALUES (5, CAST(strftime('%s', 'now') AS INTEGER) * 1000);
            "#,
        )
        .map_err(|err| format!("执行 SQLite v5 迁移失败: {err}"))
}

fn migrate_v6(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            r#"
            ALTER TABLE capability_snapshots ADD COLUMN window_seconds INTEGER;
            ALTER TABLE capability_snapshots ADD COLUMN reset_at INTEGER;

            ALTER TABLE radar_analyses ADD COLUMN event_id TEXT;
            ALTER TABLE radar_analyses ADD COLUMN analysis_mode TEXT;
            ALTER TABLE radar_analyses ADD COLUMN context_hash TEXT NOT NULL DEFAULT '';
            ALTER TABLE radar_analyses ADD COLUMN prompt_hash TEXT NOT NULL DEFAULT '';
            ALTER TABLE radar_analyses ADD COLUMN event_relation TEXT;
            ALTER TABLE radar_analyses ADD COLUMN event_phase TEXT;
            ALTER TABLE radar_analyses ADD COLUMN delta_effect TEXT;
            ALTER TABLE radar_analyses ADD COLUMN signal_level TEXT;
            ALTER TABLE radar_analyses ADD COLUMN context_status TEXT;

            CREATE TABLE radar_events (
                id TEXT PRIMARY KEY,
                phase TEXT NOT NULL,
                title TEXT NOT NULL,
                summary TEXT,
                first_signal_at INTEGER NOT NULL,
                latest_evidence_at INTEGER NOT NULL,
                claimed_landed_at INTEGER,
                observed_reset_at INTEGER,
                closed_at INTEGER,
                close_reason TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX idx_radar_events_updated ON radar_events(updated_at DESC);

            CREATE TABLE radar_event_evidence (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT NOT NULL REFERENCES radar_events(id) ON DELETE CASCADE,
                post_id TEXT NOT NULL,
                relation TEXT NOT NULL,
                analysis_id TEXT,
                added_at INTEGER NOT NULL,
                UNIQUE(event_id, post_id)
            );

            INSERT INTO schema_migrations(version, applied_at)
            VALUES (6, CAST(strftime('%s', 'now') AS INTEGER) * 1000);
            "#,
        )
        .map_err(|err| format!("执行 SQLite v6 迁移失败: {err}"))
}

fn migrate_v7(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            r#"
            CREATE TABLE radar_custom_models (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
                model TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                UNIQUE(source_id, model)
            );
            CREATE INDEX idx_radar_custom_models_source ON radar_custom_models(source_id, created_at);

            INSERT INTO schema_migrations(version, applied_at)
            VALUES (7, CAST(strftime('%s', 'now') AS INTEGER) * 1000);
            "#,
        )
        .map_err(|err| format!("执行 SQLite v7 迁移失败: {err}"))
}

fn migrate_v8(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            r#"
            ALTER TABLE radar_events ADD COLUMN expected_at INTEGER;
            ALTER TABLE radar_events ADD COLUMN expires_at INTEGER;
            ALTER TABLE radar_events ADD COLUMN state_revision INTEGER NOT NULL DEFAULT 0;

            ALTER TABLE radar_analyses ADD COLUMN temporal_phase TEXT;
            ALTER TABLE radar_analyses ADD COLUMN valid_until INTEGER;
            ALTER TABLE radar_analyses ADD COLUMN state_revision INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE radar_analyses ADD COLUMN timezone_policy_version TEXT NOT NULL DEFAULT '';

            CREATE TABLE radar_time_claims (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                post_id TEXT NOT NULL,
                raw_text TEXT NOT NULL,
                clock_hour INTEGER,
                clock_minute INTEGER,
                date_relation TEXT,
                timezone_kind TEXT,
                timezone_assumed INTEGER NOT NULL DEFAULT 0,
                parse_status TEXT NOT NULL,
                resolved_at INTEGER,
                precision TEXT NOT NULL,
                parser_version TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE(post_id, raw_text)
            );
            CREATE INDEX idx_radar_time_claims_post ON radar_time_claims(post_id, updated_at);

            CREATE TABLE quota_reset_observations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
                capability_id TEXT NOT NULL,
                previous_snapshot_id INTEGER NOT NULL REFERENCES capability_snapshots(id) ON DELETE CASCADE,
                current_snapshot_id INTEGER NOT NULL REFERENCES capability_snapshots(id) ON DELETE CASCADE,
                classification TEXT NOT NULL,
                observed_at INTEGER NOT NULL,
                event_id TEXT REFERENCES radar_events(id) ON DELETE SET NULL,
                temporal_correlation TEXT NOT NULL DEFAULT 'none',
                user_confirmed_at INTEGER,
                created_at INTEGER NOT NULL,
                UNIQUE(source_id, capability_id, previous_snapshot_id, current_snapshot_id)
            );
            CREATE INDEX idx_quota_reset_observations_event ON quota_reset_observations(event_id, observed_at DESC);
            CREATE INDEX idx_quota_reset_observations_source ON quota_reset_observations(source_id, observed_at DESC);

            UPDATE radar_events
            SET latest_evidence_at = COALESCE(
                (SELECT MAX(p.posted_at)
                 FROM radar_event_evidence e
                 JOIN tibo_posts p ON p.id = e.post_id
                 WHERE e.event_id = radar_events.id),
                latest_evidence_at
            );

            INSERT INTO schema_migrations(version, applied_at)
            VALUES (8, CAST(strftime('%s', 'now') AS INTEGER) * 1000);
            "#,
        )
        .map_err(|err| format!("执行 SQLite v8 迁移失败: {err}"))
}

fn migrate_v9(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            r#"
            -- 归一化窗口能力 id：旧版 GPT/Codex 适配器曾把标准时长写成 quota_window_{秒}s，
            -- 与规范 id（quota_window_5h/7d/30d）并存，导致同一窗口在界面重复展示（一个 fresh、
            -- 一个 missing）。此处把 5 小时/7 天/30 天三种标准时长重写为规范 id，其余时长保留。
            UPDATE capability_snapshots
            SET capability_id = CASE capability_id
                WHEN 'quota_window_18000s'  THEN 'quota_window_5h'
                WHEN 'quota_window_604800s' THEN 'quota_window_7d'
                WHEN 'quota_window_2592000s' THEN 'quota_window_30d'
                ELSE capability_id
            END
            WHERE capability_id IN ('quota_window_18000s','quota_window_604800s','quota_window_2592000s');

            INSERT INTO schema_migrations(version, applied_at)
            VALUES (9, CAST(strftime('%s', 'now') AS INTEGER) * 1000);
            "#,
        )
        .map_err(|err| format!("执行 SQLite v9 迁移失败: {err}"))
}

fn migrate_v10(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            r#"
            ALTER TABLE tibo_posts ADD COLUMN lifecycle_consumed_at INTEGER;
            ALTER TABLE radar_events ADD COLUMN user_confirmed_reset_at INTEGER;

            UPDATE tibo_posts
            SET lifecycle_consumed_at = (
                SELECT MIN(a.created_at)
                FROM radar_analyses a
                WHERE a.error_message IS NULL
                  AND a.from_posted_at IS NOT NULL
                  AND a.to_posted_at IS NOT NULL
                  AND tibo_posts.posted_at >= a.from_posted_at
                  AND tibo_posts.posted_at <= a.to_posted_at
            )
            WHERE lifecycle_consumed_at IS NULL
              AND EXISTS (
                SELECT 1 FROM radar_analyses a
                WHERE a.error_message IS NULL
                  AND a.from_posted_at IS NOT NULL
                  AND a.to_posted_at IS NOT NULL
                  AND tibo_posts.posted_at >= a.from_posted_at
                  AND tibo_posts.posted_at <= a.to_posted_at
              );

            UPDATE tibo_posts
            SET lifecycle_consumed_at = (
                SELECT MIN(e.added_at) FROM radar_event_evidence e WHERE e.post_id = tibo_posts.id
            )
            WHERE lifecycle_consumed_at IS NULL
              AND EXISTS (SELECT 1 FROM radar_event_evidence e WHERE e.post_id = tibo_posts.id);

            INSERT INTO schema_migrations(version, applied_at)
            VALUES (10, CAST(strftime('%s', 'now') AS INTEGER) * 1000);
            "#,
        )
        .map_err(|err| format!("执行 SQLite v10 迁移失败: {err}"))
}

/// v11：radar_analyses 增加输入分组持久化，用于隔离“当前判断依据”与“历史上下文引用”。
/// 旧记录三列均为空数组，不做猜测性回填。
fn migrate_v11(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            r#"
            ALTER TABLE radar_analyses ADD COLUMN new_post_ids_json TEXT NOT NULL DEFAULT '[]';
            ALTER TABLE radar_analyses ADD COLUMN event_context_post_ids_json TEXT NOT NULL DEFAULT '[]';
            ALTER TABLE radar_analyses ADD COLUMN historical_post_ids_json TEXT NOT NULL DEFAULT '[]';

            INSERT INTO schema_migrations(version, applied_at)
            VALUES (11, CAST(strftime('%s', 'now') AS INTEGER) * 1000);
            "#,
        )
        .map_err(|err| format!("执行 SQLite v11 迁移失败: {err}"))
}

/// v12：分析增加 signal_type，事件增加 event_type；窄范围回补 v16 把 explicit_reset
/// 帖子判成 none 的生命周期消费标记。旧分析标 unknown，旧事件视为额度重置。
fn migrate_v12(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            r#"
            ALTER TABLE radar_analyses ADD COLUMN signal_type TEXT NOT NULL DEFAULT 'unknown';
            ALTER TABLE radar_events ADD COLUMN event_type TEXT NOT NULL DEFAULT 'quota_reset';

            UPDATE tibo_posts
            SET lifecycle_consumed_at = NULL
            WHERE explicit_reset = 1
              AND lifecycle_consumed_at IS NOT NULL
              AND NOT EXISTS (
                  SELECT 1 FROM radar_event_evidence e WHERE e.post_id = tibo_posts.id
              )
              AND EXISTS (
                  SELECT 1 FROM radar_analyses a
                  WHERE a.error_message IS NULL
                    AND a.prompt_version = 'radar-v16'
                    AND IFNULL(a.event_relation, '') = 'none'
                    AND (
                        EXISTS (
                            SELECT 1 FROM json_each(a.new_post_ids_json) AS j
                            WHERE j.value = tibo_posts.id
                        )
                        OR EXISTS (
                            SELECT 1 FROM json_each(a.citations_json) AS j
                            WHERE j.value = tibo_posts.id
                        )
                    )
              );

            INSERT INTO schema_migrations(version, applied_at)
            VALUES (12, CAST(strftime('%s', 'now') AS INTEGER) * 1000);
            "#,
        )
        .map_err(|err| format!("执行 SQLite v12 迁移失败: {err}"))
}

/// v13：雷达独立 OpenAI 兼容对话接入，不混入平台额度 Source。
fn migrate_v13(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            r#"
            CREATE TABLE radar_chat_endpoints (
                id TEXT PRIMARY KEY NOT NULL,
                display_name TEXT NOT NULL,
                api_base_url TEXT NOT NULL,
                secret_ref TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE radar_chat_endpoint_models (
                endpoint_id TEXT NOT NULL REFERENCES radar_chat_endpoints(id) ON DELETE CASCADE,
                model TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (endpoint_id, model)
            );
            CREATE INDEX idx_radar_chat_endpoint_models_endpoint
                ON radar_chat_endpoint_models(endpoint_id, created_at);

            INSERT INTO schema_migrations(version, applied_at)
            VALUES (13, CAST(strftime('%s', 'now') AS INTEGER) * 1000);
            "#,
        )
        .map_err(|err| format!("执行 SQLite v13 迁移失败: {err}"))
}

/// v14：本机重置卡数量观察。固化 banked_reset_count 的整数增加（到账）
/// 与减少；数量减少不得记为已使用。
fn migrate_v14(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            r#"
            CREATE TABLE banked_reset_observations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
                previous_snapshot_id INTEGER NOT NULL REFERENCES capability_snapshots(id) ON DELETE CASCADE,
                current_snapshot_id INTEGER NOT NULL REFERENCES capability_snapshots(id) ON DELETE CASCADE,
                previous_count INTEGER NOT NULL,
                current_count INTEGER NOT NULL,
                observed_at INTEGER NOT NULL,
                event_id TEXT REFERENCES radar_events(id) ON DELETE SET NULL,
                created_at INTEGER NOT NULL,
                UNIQUE(source_id, previous_snapshot_id, current_snapshot_id)
            );
            CREATE INDEX idx_banked_reset_observations_source
                ON banked_reset_observations(source_id, observed_at DESC);
            CREATE INDEX idx_banked_reset_observations_event
                ON banked_reset_observations(event_id, observed_at DESC);

            INSERT INTO schema_migrations(version, applied_at)
            VALUES (14, CAST(strftime('%s', 'now') AS INTEGER) * 1000);
            "#,
        )
        .map_err(|err| format!("执行 SQLite v14 迁移失败: {err}"))
}

/// v15：雷达来源状态独立持久化 + 时间声明语义类别。
/// - radar_source_status：CodexRadar / WillCodex 分别记录成功时间、失败与缓存状态，
///   单一来源失败不再只留整体日志，快照可分来源展示新鲜度。
/// - radar_time_claims.claim_kind：区分发放/预告时间、资格截止、活动截止与历史时间，
///   预计重置时间的计算不得把截止或历史时间当预告。旧数据回填 unknown（不改变既有行为）。
fn migrate_v15(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            r#"
            CREATE TABLE radar_source_status (
                source_id TEXT PRIMARY KEY NOT NULL,
                last_check_at INTEGER NOT NULL,
                last_success_at INTEGER,
                last_error TEXT,
                last_post_count INTEGER,
                updated_at INTEGER NOT NULL
            );

            ALTER TABLE radar_time_claims ADD COLUMN claim_kind TEXT NOT NULL DEFAULT 'unknown';

            INSERT INTO schema_migrations(version, applied_at)
            VALUES (15, CAST(strftime('%s', 'now') AS INTEGER) * 1000);
            "#,
        )
        .map_err(|err| format!("执行 SQLite v15 迁移失败: {err}"))
}

fn seed_platform_sources(connection: &mut Connection) -> Result<(), String> {
    let now = epoch_ms();
    let transaction = connection
        .transaction()
        .map_err(|err| format!("开始初始化平台模板失败: {err}"))?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO accounts(id, platform_id, display_name, kind, created_at, updated_at) VALUES (?1, 'deepseek', '默认账户', 'default', ?2, ?2)",
            params!["deepseek-default", now],
        )
        .map_err(|err| format!("初始化 DeepSeek 账户失败: {err}"))?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO sources(id, account_id, adapter_id, source_type, display_name, created_at, updated_at) VALUES (?1, 'deepseek-default', ?1, 'api_key', 'API 余额', ?2, ?2)",
            params!["deepseek-balance-api", now],
        )
        .map_err(|err| format!("初始化 DeepSeek 余额来源失败: {err}"))?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO sources(id, account_id, adapter_id, source_type, display_name, created_at, updated_at) VALUES (?1, 'deepseek-default', ?1, 'web_session', '网页用量与缓存', ?2, ?2)",
            params!["deepseek-web-session", now],
        )
        .map_err(|err| format!("初始化 DeepSeek 网页来源失败: {err}"))?;

    transaction
        .execute(
            "INSERT OR IGNORE INTO accounts(id, platform_id, display_name, kind, created_at, updated_at) VALUES (?1, 'openai', '本地 Codex 账户', 'local', ?2, ?2)",
            params!["openai-codex-local", now],
        )
        .map_err(|err| format!("初始化 GPT/Codex 账户失败: {err}"))?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO sources(id, account_id, adapter_id, source_type, display_name, created_at, updated_at) VALUES (?1, 'openai-codex-local', ?1, 'local_cli', '本地 Codex 订阅', ?2, ?2)",
            params!["openai-codex-local", now],
        )
        .map_err(|err| format!("初始化 GPT/Codex 来源失败: {err}"))?;

    for (platform_id, sort_index) in [
        ("deepseek", 0),
        ("openai", 1),
        ("claude_code", 2),
        ("glm", 3),
        ("kimi", 4),
        ("mimo", 5),
        ("minimax", 6),
    ] {
        transaction
            .execute(
                "INSERT OR IGNORE INTO platform_order(platform_id, sort_index) VALUES (?1, ?2)",
                params![platform_id, sort_index],
            )
            .map_err(|err| format!("初始化平台顺序失败: {err}"))?;
    }

    transaction
        .commit()
        .map_err(|err| format!("提交平台模板初始化失败: {err}"))
}

fn epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_snapshot_is_consistent_without_blocking_writers() {
        let path = std::env::temp_dir().join(format!(
            "aqm-read-snapshot-{}-{}.db", std::process::id(), epoch_ms()
        ));
        let database = Database::initialize_at(path.clone()).unwrap();
        database.set_setting_string("snapshot-test", "before").unwrap();
        database.read_snapshot(|reader| {
            assert_eq!(reader.setting_string("snapshot-test")?.as_deref(), Some("before"));
            // 独立连接提交刷新结果，既有读快照仍完整保留原版本。
            database.set_setting_string("snapshot-test", "after")?;
            assert_eq!(reader.setting_string("snapshot-test")?.as_deref(), Some("before"));
            assert_eq!(reader.read_snapshot(|nested| nested.setting_string("snapshot-test"))?.as_deref(), Some("before"));
            assert!(reader.set_setting_string("snapshot-test", "unexpected").is_err());
            Ok(())
        }).unwrap();
        assert_eq!(database.read_snapshot(|reader| reader.setting_string("snapshot-test")).unwrap().as_deref(), Some("after"));
        drop(database);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn read_snapshot_error_releases_connection() {
        let path = std::env::temp_dir().join(format!(
            "aqm-read-error-{}-{}.db", std::process::id(), epoch_ms()
        ));
        let database = Database::initialize_at(path.clone()).unwrap();
        let result: Result<(), String> = database.read_snapshot(|reader| {
            reader.list_sources("openai")?;
            Err("read failed".into())
        });
        assert_eq!(result.unwrap_err(), "read failed");
        database.set_setting_string("after-error", "saved").unwrap();
        let connection = database.connect().unwrap();
        let busy: i64 = connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0)).unwrap();
        assert_eq!(busy, 0, "failed reads must not retain the WAL snapshot");
        drop(connection);
        drop(database);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn creates_v1_schema_and_seed_sources() {
        let path = std::env::temp_dir().join(format!(
            "ai-quota-monitor-schema-{}-{}.db",
            std::process::id(),
            epoch_ms()
        ));
        let database = Database::initialize_at(path.clone()).expect("schema should initialize");
        let connection = database.connect().expect("database should open");
        let source_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM sources", [], |row| row.get(0))
            .expect("seeded sources should exist");
        assert_eq!(source_count, 3);
        drop(connection);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn openai_quota_sources_read_platform_from_accounts() {
        let path = std::env::temp_dir().join(format!(
            "ai-quota-monitor-quota-{}-{}.db",
            std::process::id(),
            epoch_ms()
        ));
        let database = Database::initialize_at(path.clone()).expect("db");
        let sources = database
            .openai_quota_sources()
            .expect("query should not fail");
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].id, "openai-codex-local");
        assert_eq!(sources[0].platform_id, "openai");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn v9_migration_normalizes_legacy_window_ids() {
        let path = std::env::temp_dir().join(format!(
            "ai-quota-monitor-v9-{}-{}.db",
            std::process::id(),
            epoch_ms()
        ));
        let database = Database::initialize_at(path.clone()).expect("db");
        let mut connection = database.connect().expect("connect");
        let transaction = connection.transaction().expect("tx");
        // 让 v9 迁移可重跑：清除版本标记，塞入旧版遗留 id，再执行 migrate_v9。
        transaction
            .execute("DELETE FROM schema_migrations WHERE version = 9", [])
            .expect("clear v9 marker");
        transaction
            .execute(
                "INSERT INTO capability_snapshots(account_id, source_id, capability_id, display_name, value_kind, primary_value, secondary_value, progress, trend_json, captured_at, generation)
                 VALUES ('openai-codex-local','openai-codex-local','quota_window_2592000s','30 天窗口','percent','100%','已使用 0%',1.0,'[]',0,0)",
                [],
            )
            .expect("insert legacy window");
        migrate_v9(&transaction).expect("migrate v9");
        transaction.commit().expect("commit");
        let id: String = connection
            .query_row(
                "SELECT capability_id FROM capability_snapshots WHERE source_id='openai-codex-local' AND display_name='30 天窗口'",
                [],
                |row| row.get(0),
            )
            .expect("read");
        assert_eq!(id, "quota_window_30d");
        let version: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row.get(0))
            .expect("version");
        assert!(version >= 9);
        drop(connection);
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod repair_migration_tests {
    use super::*;
    #[test]
    fn incomplete_v16_is_backed_up_and_upgraded_idempotently() {
        let path=std::env::temp_dir().join(format!("aqm-v17-{}.db",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let db=Database::initialize_at(path.clone()).unwrap();
        db.connect().unwrap().execute_batch("DROP TABLE radar_prepared_analyses; DELETE FROM schema_migrations WHERE version>=17;").unwrap();
        let db=Database::initialize_at(path.clone()).unwrap();
        assert_eq!(read_schema_version(&path).unwrap(),18);
        db.connect().unwrap().execute("INSERT INTO radar_prepared_analyses VALUES('key','{}','input','{}')",[]).unwrap();
        let backup=path.with_extension("v16.bak");
        assert_eq!(read_schema_version(&backup).unwrap(),16);
        Database::initialize_at(path.clone()).unwrap();
        let count:i64=db.connect().unwrap().query_row("SELECT count(*) FROM radar_prepared_analyses",[],|r|r.get(0)).unwrap();
        assert_eq!(count,1);
        drop(db);
        std::fs::remove_file(path).unwrap();std::fs::remove_file(backup).unwrap();
    }
}
