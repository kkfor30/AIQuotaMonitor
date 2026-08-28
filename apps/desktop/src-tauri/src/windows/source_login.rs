//! 隔离网页登录窗口。
//!
//! DeepSeek 捕获路径对齐 DeepSeekMonitorWindows 的 `start_usage_sync`：
//! document-start 注入 fetch/XHR hook，页面加载完成再补一次，先扫本应用
//! WebView2 缓存，再用标题把 token 交给原生 watcher。
//! GLM / MiMo 用 Cookie：优先 `WebviewWindow::cookies()`（含 httpOnly），
//! 标题侧信道作为辅通道。远程页面不获得 Tauri IPC 权限。
//! 验证成功后写入 Windows Credential Manager。

use crate::domain::refresh::SourceRefreshOutput;
use crate::providers::{deepseek, glm, mimo};
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

struct LoginTemplate {
    source_id: &'static str,
    window_label: &'static str,
    window_title: &'static str,
    login_url: &'static str,
    allowed_host_suffixes: &'static [&'static str],
    init_script: &'static str,
    title_prefix: &'static str,
    cookie_host_suffix: Option<&'static str>,
    cookie_required: Option<&'static str>,
    cookie_min_len: usize,
    isolated_profile: bool,
    status_open: &'static str,
    timeout_message: &'static str,
}

const DEEPSEEK_CAPTURE_SCRIPT: &str = r#"
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

const GLM_CAPTURE_SCRIPT: &str = r#"
(function() {
  if (window.__aiqm_glm_hook__) return;
  window.__aiqm_glm_hook__ = true;
  setInterval(function() {
    try {
      var cookie = document.cookie || '';
      if (cookie.indexOf('bigmodel_token_production') !== -1 && cookie.length > 80) {
        var candidate = 'AIQM_GLM_COOKIE:' + cookie;
        if (!document.title.startsWith('AIQM_GLM_COOKIE:') || document.title.length < candidate.length) {
          document.title = candidate;
        }
      }
    } catch (_) {}
  }, 1200);
})();
"#;

const MIMO_CAPTURE_SCRIPT: &str = r#"
(function() {
  if (window.__aiqm_mimo_hook__) return;
  window.__aiqm_mimo_hook__ = true;
  setInterval(function() {
    try {
      var cookie = document.cookie || '';
      if (cookie.indexOf('serviceToken') !== -1 && cookie.length > 40) {
        var candidate = 'AIQM_MIMO_COOKIE:' + cookie;
        if (!document.title.startsWith('AIQM_MIMO_COOKIE:') || document.title.length < candidate.length) {
          document.title = candidate;
        }
      }
    } catch (_) {}
  }, 1200);
})();
"#;

const TEMPLATES: &[LoginTemplate] = &[
    LoginTemplate {
        source_id: deepseek::WEB_SOURCE_ID,
        window_label: "deepseek-source-login",
        window_title: "DeepSeek 用量同步",
        login_url: "https://platform.deepseek.com/usage",
        allowed_host_suffixes: &["platform.deepseek.com"],
        init_script: DEEPSEEK_CAPTURE_SCRIPT,
        title_prefix: "AIQM_USAGE_TOKEN:",
        cookie_host_suffix: None,
        cookie_required: None,
        cookie_min_len: 0,
        isolated_profile: false,
        status_open: "请在登录窗口完成 DeepSeek 登录并打开用量页。同步成功后会自动保存会话并刷新 Token 与缓存。",
        timeout_message: "DeepSeek 网页登录等待超时，请关闭后重试或手动粘贴 usage token。",
    },
    LoginTemplate {
        source_id: glm::WEB_BALANCE_SOURCE_ID,
        window_label: "glm-source-login",
        window_title: "GLM 账号登录",
        login_url: "https://open.bigmodel.cn/usercenter/financialoverview",
        allowed_host_suffixes: &["bigmodel.cn"],
        init_script: GLM_CAPTURE_SCRIPT,
        title_prefix: "AIQM_GLM_COOKIE:",
        cookie_host_suffix: Some("bigmodel.cn"),
        cookie_required: Some("bigmodel_token_production"),
        cookie_min_len: 80,
        isolated_profile: true,
        status_open: "请在登录窗口完成 GLM 登录。同步成功后会自动验证并保存网页个人余额会话。",
        timeout_message: "GLM 网页登录等待超时，请关闭后重试。",
    },
    LoginTemplate {
        source_id: mimo::SOURCE_ID,
        window_label: "mimo-source-login",
        window_title: "MiMo 账号登录",
        login_url: "https://platform.xiaomimimo.com/#/console/balance",
        allowed_host_suffixes: &["xiaomimimo.com"],
        init_script: MIMO_CAPTURE_SCRIPT,
        title_prefix: "AIQM_MIMO_COOKIE:",
        cookie_host_suffix: Some("xiaomimimo.com"),
        cookie_required: Some("serviceToken"),
        cookie_min_len: 40,
        isolated_profile: true,
        status_open: "请在登录窗口完成 MiMo 登录。同步成功后会自动验证并保存网页余额会话。",
        timeout_message: "MiMo 网页登录等待超时，请关闭后重试。",
    },
];

fn template_for(source_id: &str) -> Option<&'static LoginTemplate> {
    TEMPLATES.iter().find(|item| item.source_id == source_id)
}

pub fn is_web_login_source(source_id: &str) -> bool {
    template_for(source_id).is_some()
}

pub async fn open(app: &tauri::AppHandle, source_id: &str) -> Result<(), String> {
    let template = template_for(source_id).ok_or_else(|| "此来源不支持网页登录".to_string())?;
    if let Some(window) = app.get_webview_window(template.window_label) {
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.eval(&format!("location.href = '{}';", template.login_url));
        let _ = app.emit("source-login-status", format!("正在打开 {}…", template.window_title));
        return Ok(());
    }

    let url = WebviewUrl::External(
        template
            .login_url
            .parse()
            .map_err(|_| format!("{}地址无效", template.window_title))?,
    );
    let mut builder = WebviewWindowBuilder::new(app, template.window_label, url)
        .title(template.window_title)
        .inner_size(1200.0, 800.0)
        .min_inner_size(960.0, 640.0)
        .resizable(true)
        .center()
        .visible(true)
        .initialization_script(template.init_script);
    if template.isolated_profile {
        builder = builder.data_directory(session_data_dir(app, template.source_id)?);
    }
    let allowed: Vec<String> = template
        .allowed_host_suffixes
        .iter()
        .map(|value| (*value).to_string())
        .collect();
    let script = template.init_script.to_string();
    let is_deepseek = template.source_id == deepseek::WEB_SOURCE_ID;
    let window = builder
        .on_page_load(move |window, payload| {
            if !matches!(payload.event(), PageLoadEvent::Finished) {
                return;
            }
            let host = payload.url().host_str().unwrap_or_default();
            if !allowed.iter().any(|suffix| host.ends_with(suffix.as_str())) {
                return;
            }
            if is_deepseek {
                let path = payload.url().path();
                if path == "/" || path.is_empty() {
                    let _ = window.eval("location.replace('https://platform.deepseek.com/usage');");
                    return;
                }
            }
            let _ = window.eval(&script);
        })
        .build()
        .map_err(|error| format!("打开登录窗口失败：{error}"))?;
    let app_handle = app.clone();
    let closed_source = template.source_id;
    window.on_window_event(move |event| {
        if matches!(
            event,
            tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed
        ) {
            let _ = app_handle.emit("source-login-closed", closed_source);
        }
    });
    let _ = app.emit("source-login-status", template.status_open);
    start_watcher(app.clone(), template.source_id);
    Ok(())
}

pub fn clear_session(app: &tauri::AppHandle, source_id: &str) -> Result<(), String> {
    let Some(template) = template_for(source_id) else {
        return Ok(());
    };
    if let Some(window) = app.get_webview_window(template.window_label) {
        let _ = window.clear_all_browsing_data();
        let _ = window.destroy().or_else(|_| window.close());
        return Ok(());
    }
    if template.isolated_profile {
        if let Ok(dir) = session_data_dir(app, template.source_id) {
            let _ = fs::remove_dir_all(dir);
        }
        return Ok(());
    }
    let url = WebviewUrl::External(
        "https://platform.deepseek.com"
            .parse()
            .map_err(|_| "DeepSeek 登录地址无效".to_string())?,
    );
    let window = WebviewWindowBuilder::new(app, "deepseek-session-clear", url)
        .visible(false)
        .build()
        .map_err(|error| format!("清理登录会话失败：{error}"))?;
    let _ = window.clear_all_browsing_data();
    let _ = window.destroy().or_else(|_| window.close());
    Ok(())
}

pub fn close(app: &tauri::AppHandle, source_id: &str) -> Result<(), String> {
    let template = template_for(source_id).ok_or_else(|| "此来源没有网页登录页".to_string())?;
    let _ = app.emit("source-login-closed", template.source_id);
    if let Some(window) = app.get_webview_window(template.window_label) {
        if window.destroy().is_err() {
            window
                .close()
                .map_err(|error| format!("关闭登录窗口失败：{error}"))?;
        }
    }
    Ok(())
}

fn session_data_dir(app: &tauri::AppHandle, source_id: &str) -> Result<PathBuf, String> {
    let mut dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法定位登录会话目录：{error}"))?;
    dir.push("web-sessions");
    dir.push(source_id);
    fs::create_dir_all(&dir).map_err(|error| format!("无法创建登录会话目录：{error}"))?;
    Ok(dir)
}

fn is_usage_page(window: &tauri::WebviewWindow) -> bool {
    window
        .url()
        .ok()
        .is_some_and(|url| url.path().starts_with("/usage"))
}

fn start_watcher(app: tauri::AppHandle, source_id: &'static str) {
    tauri::async_runtime::spawn(async move {
        let Some(template) = template_for(source_id) else {
            return;
        };
        tokio::time::sleep(Duration::from_secs(3)).await;
        let mut cache_scan_failed = false;
        for _ in 0..1200 {
            let Some(window) = app.get_webview_window(template.window_label) else {
                return;
            };
            if template.source_id == deepseek::WEB_SOURCE_ID && !cache_scan_failed {
                if let Some(token) = find_webview_cached_usage_token() {
                    let allow_blank = is_usage_page(&window);
                    match capture(&app, template.source_id, &token, allow_blank).await {
                        Ok(()) => {
                            let _ = window.close();
                            let _ = app.emit("source-credential-updated", template.source_id);
                            return;
                        }
                        Err(error) if error.contains("尚未就绪") => {}
                        Err(error) => {
                            cache_scan_failed = true;
                            let _ = app.emit("source-login-error", error);
                        }
                    }
                }
            }
            if let Some(suffix) = template.cookie_host_suffix {
                if let Some(cookie) = read_cookie_header(&window, suffix).await {
                    if cookie_ready(&cookie, template) {
                        match capture(&app, template.source_id, &cookie, true).await {
                            Ok(()) => {
                                let _ = window.close();
                                let _ = app.emit("source-credential-updated", template.source_id);
                                return;
                            }
                            Err(error) => {
                                let _ = app.emit(
                                    "source-login-status",
                                    format!("已读取登录 Cookie，正在等待余额接口就绪：{error}"),
                                );
                            }
                        }
                    }
                }
            }
            if let Ok(title) = window.title() {
                if let Some(secret) = title.strip_prefix(template.title_prefix) {
                    let secret = secret.trim().to_string();
                    let _ = window.set_title(template.window_title);
                    let allow_blank = template.source_id != deepseek::WEB_SOURCE_ID || is_usage_page(&window);
                    match capture(&app, template.source_id, &secret, allow_blank).await {
                        Ok(()) => {
                            let _ = window.close();
                            let _ = app.emit("source-credential-updated", template.source_id);
                            return;
                        }
                        Err(error) if error.contains("尚未就绪") => {
                            let _ = app.emit(
                                "source-login-status",
                                "已捕获登录过程中的临时会话，用量仍为空。请留在平台页等待自动同步，不要关闭窗口。",
                            );
                        }
                        Err(error) => {
                            let _ = app.emit("source-login-error", error);
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        let _ = app.emit("source-login-error", template.timeout_message);
    });
}

fn cookie_ready(header: &str, template: &LoginTemplate) -> bool {
    if header.len() < template.cookie_min_len {
        return false;
    }
    match template.cookie_required {
        Some("serviceToken") => mimo::cookie_looks_logged_in(header),
        Some(name) => crate::providers::money::cookie_named(header, name),
        None => false,
    }
}

async fn read_cookie_header(window: &tauri::WebviewWindow, host_suffix: &str) -> Option<String> {
    let window = window.clone();
    let suffix = host_suffix.trim_start_matches('.').to_string();
    tokio::task::spawn_blocking(move || {
        let cookies = window.cookies().ok()?;
        let parts: Vec<String> = cookies
            .iter()
            .filter(|cookie| {
                cookie
                    .domain()
                    .map(|domain| domain.trim_start_matches('.').ends_with(suffix.as_str()))
                    .unwrap_or(true)
            })
            .filter(|cookie| !cookie.name().is_empty() && !cookie.value().is_empty())
            .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
            .collect();
        (!parts.is_empty()).then_some(parts.join("; "))
    })
    .await
    .ok()
    .flatten()
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

fn usage_is_blank(output: &SourceRefreshOutput) -> bool {
    output.capabilities.iter().all(|capability| match capability.capability_id.as_str() {
        "usage_trend" => true,
        "cache_hit_rate" => matches!(capability.primary_value.as_deref(), None | Some("0%") | Some("0.0%")),
        "today_spend" | "month_spend" => matches!(capability.primary_value.as_deref(), None | Some("¥0.00")),
        "model_usage_v4_flash"
        | "model_usage_v4_pro"
        | "request_count"
        | "prompt_tokens"
        | "cache_hit_tokens"
        | "cache_miss_tokens"
        | "response_tokens" => matches!(capability.primary_value.as_deref(), None | Some("0")),
        _ => true,
    })
}

async fn capture(app: &tauri::AppHandle, source_id: &str, secret: &str, allow_blank: bool) -> Result<(), String> {
    let database = app.state::<Database>();
    let coordinator = app.state::<RefreshCoordinator>();
    let source = database.source(source_id)?;
    let output = coordinator.validate_secret(&source, secret).await?;
    if source_id == deepseek::WEB_SOURCE_ID && !allow_blank && usage_is_blank(&output) {
        return Err("用量会话尚未就绪".into());
    }
    let reference = vault::secret_ref(&source.account_id, &source.id);
    let previous = vault::get(&reference)?;
    vault::set(&reference, secret)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_deepseek_glm_and_mimo_login() {
        assert!(is_web_login_source(deepseek::WEB_SOURCE_ID));
        assert!(is_web_login_source(glm::WEB_BALANCE_SOURCE_ID));
        assert!(is_web_login_source(mimo::SOURCE_ID));
        assert!(!is_web_login_source("kimi-balance-api"));
    }
}
