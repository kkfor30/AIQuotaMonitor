//! DeepSeek 隔离网页登录窗口。
//!
//! 捕获路径对齐只读参考仓库 DeepSeekMonitorWindows 的 `start_usage_sync`：
//! document-start 注入 fetch/XHR hook，页面加载完成再补一次，先扫本应用
//! WebView2 缓存，再用标题把 token 交给原生 watcher。
//! 远程页面不获得 Tauri IPC 权限。验证成功后写入 Windows Credential Manager。

use crate::providers::deepseek;
use crate::refresh::RefreshCoordinator;
use crate::storage::database::Database;
use crate::storage::vault;
use std::fs;
use std::io::Read;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::webview::PageLoadEvent;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const WINDOW_LABEL: &str = "deepseek-source-login";
const TOKEN_TITLE_PREFIX: &str = "AIQM_USAGE_TOKEN:";
const CAPTURE_SCRIPT: &str = r#"
(function() {
  if (window.__aiqm_token_hook__) return;
  window.__aiqm_token_hook__ = true;
  function deliver(token) {
    if (!token || typeof token !== 'string') return;
    token = token.trim();
    if (token.length < 20) return;
    try { document.title = 'AIQM_USAGE_TOKEN:' + token; } catch (_) {}
  }
  function fromAuth(value) {
    if (!value) return;
    var match = /Bearer\s+(\S+)/i.exec(String(value));
    if (match && match[1]) deliver(match[1]);
  }
  var originalFetch = window.fetch;
  if (typeof originalFetch === 'function') {
    window.fetch = function(input, init) {
      try {
        var headers = (init && init.headers) || (input && input.headers);
        if (headers) {
          if (typeof Headers !== 'undefined' && headers instanceof Headers) {
            fromAuth(headers.get('authorization'));
          } else if (Array.isArray(headers)) {
            for (var i = 0; i < headers.length; i++) {
              if (headers[i] && String(headers[i][0]).toLowerCase() === 'authorization') fromAuth(headers[i][1]);
            }
          } else if (typeof headers === 'object') {
            for (var key in headers) {
              if (key.toLowerCase() === 'authorization') fromAuth(headers[key]);
            }
          }
        }
      } catch (_) {}
      return originalFetch.apply(this, arguments);
    };
  }
  var originalSetRequestHeader = XMLHttpRequest.prototype.setRequestHeader;
  XMLHttpRequest.prototype.setRequestHeader = function(name, value) {
    try { if (name && String(name).toLowerCase() === 'authorization') fromAuth(value); } catch (_) {}
    return originalSetRequestHeader.apply(this, arguments);
  };
})();
"#;

pub async fn open(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(token) = find_webview_cached_usage_token() {
        match capture(app, &token).await {
            Ok(()) => {
                if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
                    let _ = window.close();
                }
                let _ = app.emit("source-credential-updated", deepseek::WEB_SOURCE_ID);
                return Ok(());
            }
            Err(error) => {
                let _ = app.emit("source-login-error", error);
            }
        }
    }

    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.eval("location.reload();");
        let _ = app.emit("source-login-status", "正在重新加载 DeepSeek 登录页…");
        return Ok(());
    }

    let url = WebviewUrl::External(
        "https://platform.deepseek.com"
            .parse()
            .map_err(|_| "DeepSeek 登录地址无效".to_string())?,
    );
    let window = WebviewWindowBuilder::new(app, WINDOW_LABEL, url)
        .title("DeepSeek 账号登录")
        .inner_size(480.0, 720.0)
        .min_inner_size(360.0, 480.0)
        .resizable(true)
        .center()
        .visible(true)
        .initialization_script(CAPTURE_SCRIPT)
        .on_page_load(|window, payload| {
            if matches!(payload.event(), PageLoadEvent::Finished)
                && payload
                    .url()
                    .host_str()
                    .is_some_and(|host| host == "platform.deepseek.com")
            {
                let _ = window.eval(CAPTURE_SCRIPT);
            }
        })
        .build()
        .map_err(|error| format!("打开 DeepSeek 登录窗口失败：{error}"))?;
    let app_handle = app.clone();
    window.on_window_event(move |event| {
        if matches!(
            event,
            tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed
        ) {
            let _ = app_handle.emit("source-login-closed", deepseek::WEB_SOURCE_ID);
        }
    });
    let _ = app.emit(
        "source-login-status",
        "请在登录窗口完成 DeepSeek 账号登录。登录成功后会自动验证并保存网页会话。",
    );
    start_watcher(app.clone());
    Ok(())
}

pub fn close(app: &tauri::AppHandle) -> Result<(), String> {
    let _ = app.emit("source-login-closed", deepseek::WEB_SOURCE_ID);
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        if window.destroy().is_err() {
            window
                .close()
                .map_err(|error| format!("关闭 DeepSeek 登录窗口失败：{error}"))?;
        }
    }
    Ok(())
}

fn start_watcher(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let mut cache_scan_failed = false;
        for _ in 0..1200 {
            let Some(window) = app.get_webview_window(WINDOW_LABEL) else {
                return;
            };
            if !cache_scan_failed {
                if let Some(token) = find_webview_cached_usage_token() {
                    match capture(&app, &token).await {
                        Ok(()) => {
                            let _ = window.close();
                            let _ = app.emit("source-credential-updated", deepseek::WEB_SOURCE_ID);
                            return;
                        }
                        Err(error) => {
                            cache_scan_failed = true;
                            let _ = app.emit("source-login-error", error);
                        }
                    }
                }
            }
            if let Ok(title) = window.title() {
                if let Some(token) = title.strip_prefix(TOKEN_TITLE_PREFIX) {
                    let token = token.trim().to_string();
                    let _ = window.set_title("DeepSeek 账号登录");
                    match capture(&app, &token).await {
                        Ok(()) => {
                            let _ = window.close();
                            let _ = app.emit("source-credential-updated", deepseek::WEB_SOURCE_ID);
                            return;
                        }
                        Err(error) => {
                            let _ = app.emit("source-login-error", error);
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        let _ = app.emit(
            "source-login-error",
            "DeepSeek 网页登录等待超时，请关闭后重试或手动粘贴 usage token。",
        );
    });
}

fn read_shared_text(path: &Path) -> Option<String> {
    let mut file = fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2 | 0x4)
        .open(path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    if metadata.len() == 0 || metadata.len() > 20 * 1024 * 1024 {
        return None;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).replace('\0', ""))
}

fn extract_user_api_token(text: &str) -> Option<String> {
    let mut search_from = 0;
    let marker = "\"token\":\"";
    while let Some(relative_index) = text[search_from..].find(marker) {
        let token_start = search_from + relative_index + marker.len();
        let token_end = token_start + text[token_start..].find('"')?;
        let token = &text[token_start..token_end];
        let context_end = (token_end + 1800).min(text.len());
        let context = &text[token_end..context_end];
        if token.len() > 20
            && context.contains("\"id_profile\"")
            && context.contains("\"feature_gates\"")
        {
            return Some(token.to_string());
        }
        search_from = token_end + 1;
    }
    None
}

fn find_webview_cached_usage_token() -> Option<String> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")?;
    let cache_dir = PathBuf::from(local_app_data)
        .join("com.aiquotamonitor.desktop")
        .join("EBWebView")
        .join("Default")
        .join("Cache")
        .join("Cache_Data");
    let entries = fs::read_dir(cache_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Some(token) = read_shared_text(&path).and_then(|text| extract_user_api_token(&text)) {
            return Some(token);
        }
    }
    None
}

async fn capture(app: &tauri::AppHandle, token: &str) -> Result<(), String> {
    let database = app.state::<Database>();
    let coordinator = app.state::<RefreshCoordinator>();
    let source = database.source(deepseek::WEB_SOURCE_ID)?;
    let output = coordinator.validate_secret(&source, token).await?;
    let reference = vault::secret_ref(&source.account_id, &source.id);
    let previous = vault::get(&reference)?;
    vault::set(&reference, token)?;
    if let Err(error) = database.save_secret_ref(&source.id, &reference) {
        restore_vault(&reference, previous.as_deref());
        return Err(error);
    }
    if let Err(error) = coordinator.persist_validated(&database, &source, &output) {
        restore_vault(&reference, previous.as_deref());
        if source.secret_ref.is_none() {
            let _ = database.clear_secret_ref(&source.id);
        }
        return Err(error);
    }
    Ok(())
}

fn restore_vault(reference: &str, previous: Option<&str>) {
    if let Some(previous) = previous {
        let _ = vault::set(reference, previous);
    } else {
        let _ = vault::delete(reference);
    }
}
