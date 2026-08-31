//! 旧 DeepSeekMonitorWindows 明文配置的一次性、安全导入。
//!
//! 只在用户明确点击导入后读取秘密；检查接口只返回布尔状态，不返回明文。

use crate::domain::PlatformSummaryViewModel;
use crate::storage::database::Database;
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;

pub const COMPLETED_SETTING: &str = "legacy_deepseek_import_completed";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyConfigInspection {
    pub available: bool,
    pub path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyImportResult {
    pub platforms: Vec<PlatformSummaryViewModel>,
    pub imported_source_ids: Vec<String>,
    pub archived_path: Option<String>,
}

#[derive(Debug)]
pub struct LegacySecrets {
    pub path: PathBuf,
    pub api_key: Option<String>,
    pub usage_token: Option<String>,
}

pub fn inspect(database: &Database) -> Result<LegacyConfigInspection, String> {
    if database.setting_bool(COMPLETED_SETTING)? {
        return Ok(LegacyConfigInspection {
            available: false,
            path: None,
        });
    }
    let Some(path) = config_path() else {
        return Ok(empty());
    };
    if !path.is_file() {
        return Ok(empty());
    }
    Ok(LegacyConfigInspection {
        available: true,
        path: Some(path.display().to_string()),
    })
}

pub fn read() -> Result<LegacySecrets, String> {
    let path = config_path().ok_or_else(|| "未找到旧配置目录".to_string())?;
    read_from(path)
}

pub fn mark_completed(database: &Database) -> Result<(), String> {
    database.set_setting_bool(COMPLETED_SETTING, true)
}

pub fn archive(path: &PathBuf) -> Result<PathBuf, String> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "旧配置文件名无效".to_string())?;
    let archived = path.with_file_name(format!("{file_name}.migrated-{timestamp}.bak"));
    std::fs::rename(path, &archived).map_err(|err| format!("归档旧配置失败: {err}"))?;
    Ok(archived)
}

fn config_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("DeepSeekMonitorWindows").join("config.json"))
}

fn read_from(path: PathBuf) -> Result<LegacySecrets, String> {
    let text = std::fs::read_to_string(&path).map_err(|err| format!("读取旧配置失败: {err}"))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|_| "旧配置 JSON 格式无效".to_string())?;
    let provider = value
        .get("providers")
        .and_then(|value| value.get("deepseek"));
    let api_key = provider
        .and_then(|value| value.get("api_key"))
        .and_then(Value::as_str)
        .or_else(|| value.get("api_key").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let usage_token = provider
        .and_then(|value| value.get("extra"))
        .and_then(|value| value.get("usage_token"))
        .and_then(Value::as_str)
        .or_else(|| value.get("usage_token").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Ok(LegacySecrets {
        path,
        api_key,
        usage_token,
    })
}

fn empty() -> LegacyConfigInspection {
    LegacyConfigInspection {
        available: false,
        path: None,
    }
}
