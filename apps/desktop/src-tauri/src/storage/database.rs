//! SQLite v1 schema 与迁移入口。
//!
//! 迁移机制参考 cc-switch（审计基线 6243e20a）的版本表与升级前备份思路，
//! 业务表按 AIQuotaMonitor 的 Account → Source → Capability → Snapshot 重新设计。

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const CURRENT_SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone)]
pub struct Database {
    path: PathBuf,
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
        Ok(Self { path })
    }

    pub fn connect(&self) -> Result<Connection, String> {
        open_connection(&self.path)
    }

    #[allow(dead_code)]
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
    let connection = Connection::open(path).map_err(|err| format!("读取 SQLite 版本失败: {err}"))?;
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
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row.get::<_, Option<i64>>(0))
        .optional()
        .map_err(|err| format!("读取 SQLite schema 版本失败: {err}"))
        .map(|value| value.flatten().unwrap_or(0))
}

fn backup_before_upgrade(path: &Path, version: i64) -> Result<(), String> {
    let backup = path.with_extension(format!("v{version}.bak"));
    fs::copy(path, &backup)
        .map(|_| ())
        .map_err(|err| format!("升级前备份 SQLite 失败: {err}"))
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

fn seed_platform_sources(connection: &mut Connection) -> Result<(), String> {
    let now = epoch_ms();
    let transaction = connection
        .transaction()
        .map_err(|err| format!("开始初始化平台模板失败: {err}"))?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO accounts(id, platform_id, display_name, created_at, updated_at) VALUES (?1, 'deepseek', '默认账户', ?2, ?2)",
            params!["deepseek-default", now],
        )
        .map_err(|err| format!("初始化 DeepSeek 账户失败: {err}"))?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO sources(id, account_id, source_type, display_name, created_at, updated_at) VALUES (?1, 'deepseek-default', 'api_key', 'API 余额', ?2, ?2)",
            params!["deepseek-balance-api", now],
        )
        .map_err(|err| format!("初始化 DeepSeek 余额来源失败: {err}"))?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO sources(id, account_id, source_type, display_name, created_at, updated_at) VALUES (?1, 'deepseek-default', 'web_session', '网页用量与缓存', ?2, ?2)",
            params!["deepseek-web-session", now],
        )
        .map_err(|err| format!("初始化 DeepSeek 网页来源失败: {err}"))?;

    transaction
        .execute(
            "INSERT OR IGNORE INTO accounts(id, platform_id, display_name, created_at, updated_at) VALUES (?1, 'openai', '本地 Codex 账户', ?2, ?2)",
            params!["openai-codex-local", now],
        )
        .map_err(|err| format!("初始化 GPT/Codex 账户失败: {err}"))?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO sources(id, account_id, source_type, display_name, created_at, updated_at) VALUES (?1, 'openai-codex-local', 'local_cli', '本地 Codex 订阅', ?2, ?2)",
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
}
