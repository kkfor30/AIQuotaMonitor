//! 存储层。
//!
//! 悬浮球偏好继续使用轻量 JSON；业务数据进入 SQLite，秘密进入 Windows
//! Credential Manager。SQLite 永不保存 API Key、Token 或 Cookie 明文。

pub mod database;
pub mod vault;

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// 悬浮球停靠锚点。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoverbarAnchor {
    /// top | right | bottom | left
    pub edge: String,
    /// 沿边缘的位置比例 0.0..1.0
    pub ratio: f64,
}

impl Default for HoverbarAnchor {
    fn default() -> Self {
        Self {
            edge: "right".into(),
            ratio: 0.4,
        }
    }
}

/// 悬浮详情窗口尺寸偏好（逻辑像素）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoverbarDetailSize {
    pub width: f64,
    pub height: f64,
}

impl Default for HoverbarDetailSize {
    fn default() -> Self {
        Self {
            width: 420.0,
            height: 360.0,
        }
    }
}

/// 悬浮球偏好集合。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoverbarPreferences {
    pub enabled: bool,
    pub anchor: HoverbarAnchor,
    pub detail_size: HoverbarDetailSize,
}

impl Default for HoverbarPreferences {
    fn default() -> Self {
        Self {
            enabled: true,
            anchor: HoverbarAnchor::default(),
            detail_size: HoverbarDetailSize::default(),
        }
    }
}

fn preferences_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("hoverbar.json"))
}

/// 读取偏好；文件缺失或损坏时返回默认值（不中断启动）。
pub fn load_preferences(app: &AppHandle) -> HoverbarPreferences {
    let Some(path) = preferences_path(app) else {
        return HoverbarPreferences::default();
    };
    let Ok(raw) = fs::read_to_string(&path) else {
        return HoverbarPreferences::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

/// 保存偏好；失败仅记录日志，不影响调用方流程。
pub fn save_preferences(app: &AppHandle, prefs: &HoverbarPreferences) {
    let Some(path) = preferences_path(app) else {
        log::warn!("无法定位应用配置目录，悬浮球偏好未保存");
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(prefs) {
        Ok(json) => {
            if let Err(err) = fs::write(&path, json) {
                log::warn!("悬浮球偏好写入失败: {err}");
            }
        }
        Err(err) => log::warn!("悬浮球偏好序列化失败: {err}"),
    }
}
