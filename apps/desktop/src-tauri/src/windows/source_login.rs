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
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
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
  function deliver(token) {
    if (!token || typeof token !== 'string') return;
    token = String(token).trim().replace(/^Bearer\s+/i, '');
    if (token.length < 20 || token.length > 4096 || /\s/.test(token)) return;
    if (/^(null|undefined)$/i.test(token)) return;
    if (window.__aiqm_token__ === token) {
      try {
        if (window.chrome && window.chrome.webview && window.chrome.webview.postMessage) {
          window.chrome.webview.postMessage('AIQM_USAGE_TOKEN:' + token);
        }
      } catch (_) {}
      return;
    }
    window.__aiqm_token__ = token;
    try {
      if (window.chrome && window.chrome.webview && window.chrome.webview.postMessage) {
        window.chrome.webview.postMessage('AIQM_USAGE_TOKEN:' + token);
      }
    } catch (_) {}
    try { document.title = 'AIQM_USAGE_TOKEN:' + token; } catch (_) {}
    try {
      if (location.pathname.indexOf('/usage') === 0) {
        var hash = '#AIQM_USAGE_TOKEN=' + encodeURIComponent(token);
        if (location.hash !== hash) history.replaceState(null, '', location.pathname + location.search + hash);
      }
    } catch (_) {}
  }
  function fromAuth(value) {
    if (!value) return;
    var match = /Bearer\s+(\S+)/i.exec(String(value));
    if (match && match[1]) deliver(match[1]);
  }
  function fromUserToken(raw) {
    if (!raw) return;
    try {
      var parsed = JSON.parse(raw);
      if (parsed && typeof parsed === 'object') {
        deliver(parsed.value || parsed.token || parsed.access_token || parsed.accessToken || '');
        return;
      }
    } catch (_) {}
    deliver(raw);
  }
  function scanStores() {
    try { fromUserToken(localStorage.getItem('userToken')); } catch (_) {}
    try { fromUserToken(sessionStorage.getItem('userToken')); } catch (_) {}
    try {
      for (var i = 0; i < localStorage.length; i++) {
        var key = localStorage.key(i) || '';
        if (!/token/i.test(key) || /csrf|captcha|hcaptcha|turnstile|apdid/i.test(key)) continue;
        fromUserToken(localStorage.getItem(key));
      }
    } catch (_) {}
    if (window.__aiqm_token__) deliver(window.__aiqm_token__);
  }
  if (!window.__aiqm_token_hook__) {
    window.__aiqm_token_hook__ = true;
    var originalFetch = window.fetch;
    if (typeof originalFetch === 'function') {
      window.fetch = function(input, init) {
        try {
          var headers = (init && init.headers) || (input && input.headers);
          if (headers) {
            if (typeof Headers !== 'undefined' && headers instanceof Headers) {
              fromAuth(headers.get('authorization') || headers.get('Authorization'));
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
    setInterval(scanStores, 800);
  }
  scanStores();
})();
"#;

const GLM_CAPTURE_SCRIPT: &str = r#"
(function() {
  function deliverToken(token) {
    if (!token || typeof token !== 'string') return;
    token = String(token).trim();
    if (/^Bearer\s+/i.test(token)) token = token.replace(/^Bearer\s+/i, '');
    if (token.length < 20) return;
    var title = 'AIQM_GLM_TOKEN:' + token;
    try {
      if (!document.title.startsWith('AIQM_GLM_TOKEN:') || document.title.length < title.length) {
        document.title = title;
      }
    } catch (_) {}
  }
  function markReady() {
    try {
      if (!document.title.startsWith('AIQM_GLM_TOKEN:') && document.title !== 'AIQM_GLM_READY') {
        document.title = 'AIQM_GLM_READY';
      }
    } catch (_) {}
  }
  function looksLikeBalance(text) {
    return /availableBalance|currentBalance|totalBalance|accountBalance|cashBalance|giveAmount|available_balance|操作成功/.test(text || '');
  }
  function pickAmount(value) {
    if (!value || typeof value !== 'object') return '';
    var keys = ['availableBalance','available_balance','currentBalance','current_balance','cashBalance','totalBalance','accountBalance','balanceAmount','balance'];
    for (var i = 0; i < keys.length; i++) {
      if (value[keys[i]] !== undefined && value[keys[i]] !== null && value[keys[i]] !== '') return String(value[keys[i]]);
    }
    for (var key in value) {
      if (!Object.prototype.hasOwnProperty.call(value, key)) continue;
      var nested = pickAmount(value[key]);
      if (nested) return nested;
    }
    return '';
  }
  function deliverBalanceJson(text) {
    try {
      var body = JSON.parse(text);
      var amount = pickAmount(body.data || body);
      if (amount) {
        try { document.title = 'AIQM_GLM_BALANCE:' + amount; } catch (_) {}
        return;
      }
      if (looksLikeBalance(text)) markReady();
    } catch (_) {}
  }
  function fromAuth(value) {
    if (!value) return;
    var text = String(value);
    var match = /Bearer\s+(\S+)/i.exec(text);
    deliverToken(match ? match[1] : text);
  }
  function probeBalance() {
    var urls = [
      'https://open.bigmodel.cn/api/biz/account/query-customer-account-report',
      'https://open.bigmodel.cn/api/biz/customer/getCustomerInfo'
    ];
    urls.forEach(function(url) {
      fetch(url, { credentials: 'include', headers: { Accept: 'application/json' } })
        .then(function(response) { return response.text().then(function(text) { return { ok: response.ok, text: text }; }); })
        .then(function(result) {
          if (result && result.ok && result.text) deliverBalanceJson(result.text);
        })
        .catch(function() {});
    });
  }
  function scanStores() {
    try {
      var cookie = document.cookie || '';
      if (cookie) {
        cookie.split(';').forEach(function(part) {
          var pair = part.split('=');
          var name = (pair[0] || '').trim().toLowerCase();
          var value = pair.slice(1).join('=').trim();
          if (value.length >= 20 && (name.indexOf('token') !== -1 || name.indexOf('auth') !== -1) && name.indexOf('csrf') === -1 && name.indexOf('expire') === -1) {
            deliverToken(value);
          }
        });
      }
    } catch (_) {}
    try {
      for (var i = 0; i < localStorage.length; i++) {
        var key = localStorage.key(i) || '';
        if (/token|auth/i.test(key)) deliverToken(localStorage.getItem(key));
      }
    } catch (_) {}
    try {
      for (var j = 0; j < sessionStorage.length; j++) {
        var skey = sessionStorage.key(j) || '';
        if (/token|auth/i.test(skey)) deliverToken(sessionStorage.getItem(skey));
      }
    } catch (_) {}
  }
  if (!window.__aiqm_glm_hook__) {
    window.__aiqm_glm_hook__ = true;
    var originalFetch = window.fetch;
    if (typeof originalFetch === 'function') {
      window.fetch = function(input, init) {
        try {
          var headers = (init && init.headers) || (input && input.headers);
          if (headers) {
            if (typeof Headers !== 'undefined' && headers instanceof Headers) {
              fromAuth(headers.get('authorization') || headers.get('Authorization'));
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
        return originalFetch.apply(this, arguments).then(function(response) {
          try {
            var url = '';
            if (typeof input === 'string') url = input;
            else if (input && input.url) url = String(input.url);
            if (/account|customer|balance|financial|query-customer/i.test(url)) {
              var clone = response.clone();
              clone.text().then(function(text) { deliverBalanceJson(text); }).catch(function() {});
            }
          } catch (_) {}
          return response;
        });
      };
    }
    var originalSetRequestHeader = XMLHttpRequest.prototype.setRequestHeader;
    XMLHttpRequest.prototype.setRequestHeader = function(name, value) {
      try { if (name && String(name).toLowerCase() === 'authorization') fromAuth(value); } catch (_) {}
      return originalSetRequestHeader.apply(this, arguments);
    };
    setInterval(function() { scanStores(); probeBalance(); }, 1600);
  }
  scanStores();
  probeBalance();
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
        status_open: "请在登录窗口完成 DeepSeek 登录。捕获到会话后会自动关闭登录页并填入 Token，然后请点击「验证连接」再保存。",
        timeout_message: "DeepSeek 网页登录等待超时，请关闭后重试或手动粘贴 usage token。",
    },
    LoginTemplate {
        source_id: glm::WEB_BALANCE_SOURCE_ID,
        window_label: "glm-source-login",
        window_title: "GLM 账号登录",
        login_url: "https://open.bigmodel.cn/usercenter/financialoverview",
        allowed_host_suffixes: &["bigmodel.cn", "chatglm.cn", "zhipuai.cn"],
        init_script: GLM_CAPTURE_SCRIPT,
        title_prefix: "AIQM_GLM_TOKEN:",
        cookie_host_suffix: Some("bigmodel.cn"),
        cookie_required: Some("bigmodel_token_production"),
        cookie_min_len: 20,
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

fn watcher_generation() -> &'static Mutex<HashMap<String, u64>> {
    static GENERATION: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    GENERATION.get_or_init(|| Mutex::new(HashMap::new()))
}

fn bump_watcher(source_id: &str) -> u64 {
    let mut map = watcher_generation().lock().unwrap_or_else(|error| error.into_inner());
    let next = map.get(source_id).copied().unwrap_or(0).saturating_add(1);
    map.insert(source_id.to_string(), next);
    next
}

fn current_watcher(source_id: &str) -> u64 {
    watcher_generation()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(source_id)
        .copied()
        .unwrap_or(0)
}

fn pending_secrets() -> &'static Mutex<HashMap<String, String>> {
    static PENDING: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn take_captured_secret(source_id: &str) -> Option<String> {
    pending_secrets()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(source_id)
}

fn store_captured_secret(source_id: &str, secret: &str) {
    pending_secrets()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(source_id.to_string(), secret.to_string());
}

fn captures_in_flight() -> &'static Mutex<HashSet<String>> {
    static CAPTURES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    CAPTURES.get_or_init(|| Mutex::new(HashSet::new()))
}

fn begin_capture(source_id: &str) -> bool {
    captures_in_flight()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(source_id.to_string())
}

fn end_capture(source_id: &str) {
    captures_in_flight()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(source_id);
}

pub async fn open(app: &tauri::AppHandle, source_id: &str) -> Result<(), String> {
    let template = template_for(source_id).ok_or_else(|| "此来源不支持网页登录".to_string())?;
    let _ = take_captured_secret(template.source_id);
    let generation = bump_watcher(template.source_id);
    if let Some(window) = app.get_webview_window(template.window_label) {
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.eval(template.init_script);
        let _ = window.eval(&format!("location.href = '{}';", template.login_url));
        attach_deepseek_native_hooks(app, &window, template.source_id);
        let _ = app.emit("source-login-status", format!("正在打开 {}…", template.window_title));
        start_watcher(app.clone(), template.source_id, generation);
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
    let closed_label = template.window_label;
    window.on_window_event(move |event| {
        if matches!(
            event,
            tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed
        ) {
            unhook_native_capture(closed_label);
            let _ = app_handle.emit("source-login-closed", closed_source);
        }
    });
    attach_deepseek_native_hooks(app, &window, template.source_id);
    let _ = app.emit("source-login-status", template.status_open);
    start_watcher(app.clone(), template.source_id, generation);
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

const DEEPSEEK_POLL_SCRIPT: &str = r#"
(() => {
  function looks(token) {
    token = String(token || '').trim().replace(/^Bearer\s+/i, '');
    if (token.length < 20 || token.length > 4096 || /\s/.test(token)) return '';
    if (/^(null|undefined)$/i.test(token)) return '';
    return token;
  }
  function fromValue(value, depth) {
    if (depth > 6 || value == null) return '';
    if (typeof value === 'string') return looks(value);
    if (Array.isArray(value)) {
      for (var i = 0; i < value.length; i++) {
        var nested = fromValue(value[i], depth + 1);
        if (nested) return nested;
      }
      return '';
    }
    if (typeof value === 'object') {
      var prefer = ['value', 'token', 'userToken', 'access_token', 'accessToken'];
      for (var p = 0; p < prefer.length; p++) {
        if (value[prefer[p]]) {
          var nested = fromValue(value[prefer[p]], depth + 1);
          if (nested) return nested;
        }
      }
      for (var key in value) {
        if (!Object.prototype.hasOwnProperty.call(value, key)) continue;
        if (!/token/i.test(key)) continue;
        var nested = fromValue(value[key], depth + 1);
        if (nested) return nested;
      }
    }
    return '';
  }
  function readStore(store) {
    if (!store) return '';
    try {
      var raw = store.getItem('userToken');
      if (raw) {
        try {
          var hit = fromValue(JSON.parse(raw), 0);
          if (hit) return hit;
        } catch (_) {
          var hit = looks(raw);
          if (hit) return hit;
        }
      }
      for (var i = 0; i < store.length; i++) {
        var key = store.key(i) || '';
        if (!/token/i.test(key) || /csrf|captcha|hcaptcha|turnstile|apdid/i.test(key)) continue;
        raw = store.getItem(key) || '';
        try {
          var hit = fromValue(JSON.parse(raw), 0);
          if (hit) return hit;
        } catch (_) {
          var hit = looks(raw);
          if (hit) return hit;
        }
      }
    } catch (_) {}
    return '';
  }
  return looks(window.__aiqm_token__) || readStore(localStorage) || readStore(sessionStorage) || '';
})()
"#;

fn looks_like_usage_token(secret: &str) -> bool {
    let secret = secret.trim();
    secret.len() >= 20
        && secret.len() <= 4096
        && !secret.chars().any(char::is_whitespace)
        && !secret.eq_ignore_ascii_case("null")
        && !secret.eq_ignore_ascii_case("undefined")
}

fn token_from_prefix(value: &str) -> Option<String> {
    let secret = value.strip_prefix("AIQM_USAGE_TOKEN:")?.trim();
    looks_like_usage_token(secret).then(|| secret.to_string())
}

fn token_from_location(window: &tauri::WebviewWindow) -> Option<String> {
    let url = window.url().ok()?;
    let fragment = url.fragment()?;
    let raw = fragment.strip_prefix("AIQM_USAGE_TOKEN=")?;
    looks_like_usage_token(raw).then(|| raw.to_string())
}

fn parse_script_string(result: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(result).ok()?;
    match value {
        serde_json::Value::String(secret) if looks_like_usage_token(&secret) => Some(secret),
        _ => None,
    }
}

fn native_hooks_attached() -> &'static Mutex<HashSet<String>> {
    static HOOKS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    HOOKS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn unhook_native_capture(window_label: &str) {
    native_hooks_attached()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(window_label);
}

fn spawn_deepseek_capture(app: tauri::AppHandle, source_id: &'static str, secret: String) {
    if !looks_like_usage_token(&secret) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let Some(template) = template_for(source_id) else {
            return;
        };
        let Some(window) = app.get_webview_window(template.window_label) else {
            return;
        };
        let _ = capture_and_finish(&app, &window, source_id, &secret, true).await;
    });
}

fn attach_deepseek_native_hooks(app: &tauri::AppHandle, window: &tauri::WebviewWindow, source_id: &'static str) {
    if source_id != deepseek::WEB_SOURCE_ID {
        return;
    }
    #[cfg(windows)]
    {
        attach_deepseek_native_hooks_windows(app, window, source_id);
    }
    #[cfg(not(windows))]
    {
        let _ = (app, window, source_id);
    }
}

fn poll_deepseek_page_token(app: &tauri::AppHandle, window: &tauri::WebviewWindow, source_id: &'static str) {
    #[cfg(windows)]
    {
        poll_deepseek_page_token_windows(app, window, source_id);
    }
    #[cfg(not(windows))]
    {
        let _ = window.eval(DEEPSEEK_POLL_SCRIPT);
        let _ = (app, source_id);
    }
}

#[cfg(windows)]
fn attach_deepseek_native_hooks_windows(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    source_id: &'static str,
) {
    let label = window.label().to_string();
    {
        let mut hooked = native_hooks_attached().lock().unwrap_or_else(|error| error.into_inner());
        if !hooked.insert(label.clone()) {
            return;
        }
    }
    let app_for_webview = app.clone();
    let result = window.with_webview(move |webview| unsafe {
        use webview2_com::{DocumentTitleChangedEventHandler, WebMessageReceivedEventHandler};

        let controller = webview.controller();
        let Ok(core) = controller.CoreWebView2() else {
            return;
        };
        let app_for_message = app_for_webview.clone();
        let message_handler = WebMessageReceivedEventHandler::create(Box::new(move |_webview, args| {
            let Some(args) = args else {
                return Ok(());
            };
            let mut raw = windows_core::PWSTR::null();
            let message = if args.TryGetWebMessageAsString(&mut raw).is_ok() {
                webview2_com::take_pwstr(raw)
            } else {
                let mut json = windows_core::PWSTR::null();
                if args.WebMessageAsJson(&mut json).is_err() {
                    return Ok(());
                }
                webview2_com::take_pwstr(json).trim_matches('"').to_string()
            };
            if let Some(token) = token_from_prefix(&message) {
                spawn_deepseek_capture(app_for_message.clone(), source_id, token);
            }
            Ok(())
        }));
        let mut message_token = 0i64;
        let _ = core.add_WebMessageReceived(&message_handler, &mut message_token);

        let title_handler = DocumentTitleChangedEventHandler::create(Box::new(move |webview, _args| {
            let Some(core) = webview else {
                return Ok(());
            };
            let mut title = windows_core::PWSTR::null();
            if core.DocumentTitle(&mut title).is_err() {
                return Ok(());
            }
            let title = webview2_com::take_pwstr(title);
            if let Some(token) = token_from_prefix(&title) {
                spawn_deepseek_capture(app_for_webview.clone(), source_id, token);
            }
            Ok(())
        }));
        let mut title_token = 0i64;
        let _ = core.add_DocumentTitleChanged(&title_handler, &mut title_token);
    });
    if result.is_err() {
        unhook_native_capture(&label);
        let _ = app.emit("source-login-status", "登录窗口暂不可直接读取会话，正在改用页面脚本重试…");
    }
}

#[cfg(windows)]
fn poll_deepseek_page_token_windows(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    source_id: &'static str,
) {
    let app_for_webview = app.clone();
    let _ = window.with_webview(move |webview| unsafe {
        use webview2_com::ExecuteScriptCompletedHandler;
        use windows_core::HSTRING;

        let controller = webview.controller();
        let Ok(core) = controller.CoreWebView2() else {
            return;
        };
        let mut title = windows_core::PWSTR::null();
        if core.DocumentTitle(&mut title).is_ok() {
            let title = webview2_com::take_pwstr(title);
            if let Some(token) = token_from_prefix(&title) {
                spawn_deepseek_capture(app_for_webview.clone(), source_id, token);
            }
        }
        let handler = ExecuteScriptCompletedHandler::create(Box::new(move |error_code, result: String| {
            if error_code.is_ok() {
                if let Some(token) = parse_script_string(&result) {
                    spawn_deepseek_capture(app_for_webview.clone(), source_id, token);
                }
            }
            Ok(())
        }));
        let script = HSTRING::from(DEEPSEEK_POLL_SCRIPT);
        let _ = core.ExecuteScript(&script, &handler);
    });
}

fn start_watcher(app: tauri::AppHandle, source_id: &'static str, generation: u64) {
    tauri::async_runtime::spawn(async move {
        let Some(template) = template_for(source_id) else {
            return;
        };
        tokio::time::sleep(Duration::from_millis(400)).await;
        let mut cache_scan_failed = false;
        let mut usage_status_sent = false;
        for tick in 0..1200 {
            if current_watcher(template.source_id) != generation {
                return;
            }
            let Some(window) = app.get_webview_window(template.window_label) else {
                return;
            };
            if tick % 2 == 0 {
                let _ = window.eval(template.init_script);
            }
            if template.source_id == deepseek::WEB_SOURCE_ID {
                if is_usage_page(&window) && !usage_status_sent {
                    usage_status_sent = true;
                    let _ = app.emit("source-login-status", "已打开用量页，正在读取登录会话…");
                }
                if let Some(token) = token_from_location(&window) {
                    if matches!(
                        capture_and_finish(&app, &window, template.source_id, &token, true).await,
                        CaptureOutcome::Success
                    ) {
                        return;
                    }
                }
                poll_deepseek_page_token(&app, &window, template.source_id);
                if !cache_scan_failed && tick >= 2 {
                    if let Some(token) = find_webview_cached_usage_token() {
                        match capture_and_finish(&app, &window, template.source_id, &token, true).await {
                            CaptureOutcome::Success => return,
                            CaptureOutcome::Retry => {}
                            CaptureOutcome::Failed => cache_scan_failed = true,
                        }
                    }
                }
            }
            if template.cookie_host_suffix.is_some() {
                request_native_cookies(&app, &window, template);
            }
            maybe_open_glm_finance(&window, template);
            if let Ok(title) = window.title() {
                if title == "AIQM_GLM_READY"
                    || title.starts_with("AIQM_GLM_READY")
                    || title.starts_with("AIQM_GLM_BALANCE:")
                {
                    let _ = window.set_title(template.window_title);
                    request_native_cookies(&app, &window, template);
                    let _ = app.emit(
                        "source-login-status",
                        "已在财务页读到余额，正在读取登录 Cookie…",
                    );
                } else if let Some(secret) = title_secret(&title, template) {
                    let _ = window.set_title(template.window_title);
                    let allow_blank = template.source_id != deepseek::WEB_SOURCE_ID || is_usage_page(&window);
                    if matches!(
                        capture_and_finish(&app, &window, template.source_id, &secret, allow_blank).await,
                        CaptureOutcome::Success
                    ) {
                        return;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        if current_watcher(template.source_id) == generation {
            let _ = app.emit("source-login-error", template.timeout_message);
        }
    });
}

fn title_secret(title: &str, template: &LoginTemplate) -> Option<String> {
    if let Some(secret) = title.strip_prefix(template.title_prefix) {
        let secret = secret.trim();
        if secret.len() >= 20 {
            return Some(secret.to_string());
        }
    }
    if template.source_id == glm::WEB_BALANCE_SOURCE_ID {
        if let Some(secret) = title.strip_prefix("AIQM_GLM_COOKIE:") {
            let secret = secret.trim();
            if secret.len() >= 20 {
                return Some(secret.to_string());
            }
        }
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CaptureOutcome {
    Success,
    Retry,
    Failed,
}

fn cookie_ready(header: &str, template: &LoginTemplate) -> bool {
    if header.len() < template.cookie_min_len {
        return false;
    }
    match template.cookie_required {
        Some("serviceToken") => mimo::cookie_looks_logged_in(header),
        Some("bigmodel_token_production") => crate::providers::money::extract_token_cookie(header).is_some(),
        Some(name) => crate::providers::money::cookie_named(header, name),
        None => false,
    }
}

fn maybe_open_glm_finance(window: &tauri::WebviewWindow, template: &LoginTemplate) {
    if template.source_id != glm::WEB_BALANCE_SOURCE_ID {
        return;
    }
    let Ok(url) = window.url() else {
        return;
    };
    let host = url.host_str().unwrap_or_default();
    if !host.ends_with("bigmodel.cn") {
        return;
    }
    let path = url.path();
    if path.starts_with("/usercenter") || is_glm_login_flow(path) {
        return;
    }
    let _ = window.eval(
        "if (!/login|oauth|passport|sso|auth/i.test(location.pathname)) location.replace('https://open.bigmodel.cn/usercenter/financialoverview');",
    );
}

fn is_glm_login_flow(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.contains("login")
        || lower.contains("oauth")
        || lower.contains("passport")
        || lower.contains("sso")
        || lower.contains("auth")
}

fn cookie_query_urls(window: &tauri::WebviewWindow, template: &LoginTemplate) -> Vec<String> {
    let mut urls = Vec::new();
    let push = |urls: &mut Vec<String>, value: String| {
        if !value.is_empty() && !urls.iter().any(|existing| existing == &value) {
            urls.push(value);
        }
    };
    push(&mut urls, cookie_query_url(template.login_url));
    push(
        &mut urls,
        template
            .login_url
            .split('#')
            .next()
            .unwrap_or(template.login_url)
            .to_string(),
    );
    if let Ok(current) = window.url() {
        push(&mut urls, current.to_string());
        push(&mut urls, cookie_query_url(&current.to_string()));
    }
    if template.source_id == glm::WEB_BALANCE_SOURCE_ID {
        for extra in [
            "https://open.bigmodel.cn/usercenter/financialoverview",
            "https://open.bigmodel.cn/api/biz/account/query-customer-account-report",
            "https://open.bigmodel.cn/api/biz/customer/getCustomerInfo",
        ] {
            push(&mut urls, extra.to_string());
        }
    }
    urls
}

fn cookie_query_url(login_url: &str) -> String {
    let without_hash = login_url.split('#').next().unwrap_or(login_url);
    let Some(scheme_end) = without_hash.find("://") else {
        return without_hash.to_string();
    };
    let host = without_hash[scheme_end + 3..].split('/').next().unwrap_or_default();
    format!("{}://{host}/", &without_hash[..scheme_end])
}

fn request_native_cookies(app: &tauri::AppHandle, window: &tauri::WebviewWindow, template: &LoginTemplate) {
    #[cfg(windows)]
    {
        request_native_cookies_windows(app, window, template);
    }
    #[cfg(not(windows))]
    {
        let _ = (app, window, template);
    }
}

#[cfg(windows)]
fn request_native_cookies_windows(app: &tauri::AppHandle, window: &tauri::WebviewWindow, template: &LoginTemplate) {
    let urls = cookie_query_urls(window, template);
    let app_for_webview = app.clone();
    let source_id = template.source_id;
    let result = window.with_webview(move |webview| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_2;
        use windows_core::Interface;

        let controller = webview.controller();
        let Ok(core) = controller.CoreWebView2() else {
            return;
        };
        let Ok(core2) = core.cast::<ICoreWebView2_2>() else {
            return;
        };
        let Ok(manager) = core2.CookieManager() else {
            return;
        };
        for query_url in &urls {
            let uri = windows_core::HSTRING::from(query_url.as_str());
            let app_for_handler = app_for_webview.clone();
            let handler = webview2_com::GetCookiesCompletedHandler::create(Box::new(
                move |error_code: windows_core::Result<()>,
                      list: Option<webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2CookieList>| {
                    let parse = || -> Option<String> {
                        error_code.ok()?;
                        let list = list?;
                        let mut count = 0u32;
                        list.Count(&mut count).ok()?;
                        let mut parts = Vec::with_capacity(count as usize);
                        for index in 0..count {
                            let cookie = list.GetValueAtIndex(index).ok()?;
                            let mut name = windows_core::PWSTR::null();
                            let mut value = windows_core::PWSTR::null();
                            if cookie.Name(&mut name).is_err() || cookie.Value(&mut value).is_err() {
                                continue;
                            }
                            let name = webview2_com::take_pwstr(name);
                            let value = webview2_com::take_pwstr(value);
                            if !name.is_empty() && !value.is_empty() {
                                parts.push(format!("{name}={value}"));
                            }
                        }
                        (!parts.is_empty()).then_some(parts.join("; "))
                    };
                    if let Some(cookie) = parse() {
                        let Some(item) = template_for(source_id) else {
                            return Ok(());
                        };
                        if cookie_ready(&cookie, item) {
                            let app = app_for_handler.clone();
                            tauri::async_runtime::spawn(async move {
                                let Some(window) = app.get_webview_window(
                                    template_for(source_id).map(|item| item.window_label).unwrap_or_default(),
                                ) else {
                                    return;
                                };
                                let _ = capture_and_finish(&app, &window, source_id, &cookie, true).await;
                            });
                        } else if source_id == glm::WEB_BALANCE_SOURCE_ID && cookie.split(';').count() >= 3 {
                            let _ = app_for_handler.emit(
                                "source-login-status",
                                format!(
                                    "已读到 {} 个 Cookie，但仍缺少可用 Token，请停留在财务总览页。",
                                    cookie.split(';').count()
                                ),
                            );
                        }
                    }
                    Ok(())
                },
            ));
            let _ = manager.GetCookies(&uri, &handler);
        }
    });
    if result.is_err() {
        let _ = app.emit("source-login-status", "登录窗口暂不可读取 Cookie，正在重试…");
    }
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

async fn capture_and_finish(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    source_id: &str,
    secret: &str,
    allow_blank: bool,
) -> CaptureOutcome {
    if !begin_capture(source_id) {
        return CaptureOutcome::Retry;
    }
    let outcome = if source_id == deepseek::WEB_SOURCE_ID {
        store_captured_secret(source_id, secret);
        let _ = bump_watcher(source_id);
        let _ = window.close();
        let _ = app.emit("source-login-captured", source_id);
        let _ = app.emit(
            "source-login-status",
            "已捕获网页会话。登录页已关闭，请点击「验证连接」，通过后再保存。",
        );
        CaptureOutcome::Success
    } else {
        match capture(app, source_id, secret, allow_blank).await {
            Ok(()) => {
                let _ = window.close();
                let _ = app.emit("source-credential-updated", source_id);
                let _ = app.emit("source-login-status", "已验证并保存网页会话。");
                CaptureOutcome::Success
            }
            Err(error) if error.contains("尚未就绪") => {
                let _ = app.emit(
                    "source-login-status",
                    "已捕获登录过程中的临时会话，用量仍为空。请留在平台页等待自动同步，不要关闭窗口。",
                );
                CaptureOutcome::Retry
            }
            Err(error) => {
                let _ = app.emit(
                    "source-login-status",
                    format!("已读取登录态，正在等待余额接口就绪：{error}"),
                );
                CaptureOutcome::Failed
            }
        }
    };
    end_capture(source_id);
    outcome
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

    #[test]
    fn cookie_query_url_strips_path_and_hash() {
        assert_eq!(
            cookie_query_url("https://open.bigmodel.cn/usercenter/financialoverview"),
            "https://open.bigmodel.cn/"
        );
        assert_eq!(
            cookie_query_url("https://platform.xiaomimimo.com/#/console/balance"),
            "https://platform.xiaomimimo.com/"
        );
    }

    #[test]
    fn glm_login_flow_detects_auth_paths() {
        assert!(is_glm_login_flow("/user/login"));
        assert!(is_glm_login_flow("/oauth/authorize"));
        assert!(!is_glm_login_flow("/usercenter/financialoverview"));
    }

    #[test]
    fn captured_secret_is_taken_once() {
        store_captured_secret("deepseek-web-session", "km/example-token-value-12345");
        assert_eq!(
            take_captured_secret("deepseek-web-session").as_deref(),
            Some("km/example-token-value-12345")
        );
        assert!(take_captured_secret("deepseek-web-session").is_none());
    }

    #[test]
    fn usage_token_accepts_deepseek_km_prefix() {
        assert!(looks_like_usage_token("km/KD4EfN5tDqXoapbetnJGdB3abl8SAENjhgk"));
        assert!(!looks_like_usage_token("short"));
        assert_eq!(
            token_from_prefix("AIQM_USAGE_TOKEN:km/KD4EfN5tDqXoapbetnJGdB3abl8SAENjhgk").as_deref(),
            Some("km/KD4EfN5tDqXoapbetnJGdB3abl8SAENjhgk")
        );
        assert_eq!(
            parse_script_string("\"km/KD4EfN5tDqXoapbetnJGdB3abl8SAENjhgk\"").as_deref(),
            Some("km/KD4EfN5tDqXoapbetnJGdB3abl8SAENjhgk")
        );
    }
}
