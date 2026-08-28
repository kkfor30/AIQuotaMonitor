//! DeepSeek 隔离网页登录窗口。远程页面不获得 Tauri IPC 权限，只通过临时标题
//! 向原生 watcher 交付捕获到的 Bearer token。

use crate::providers::deepseek;
use crate::refresh::RefreshCoordinator;
use crate::storage::database::Database;
use crate::storage::vault;
use std::time::Duration;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const WINDOW_LABEL: &str = "deepseek-source-login";
const TOKEN_TITLE_PREFIX: &str = "AIQM_USAGE_TOKEN:";
const CAPTURE_SCRIPT: &str = r#"
(function() {
  if (location.hostname !== 'platform.deepseek.com') return;
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

pub fn open(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.eval("location.reload();");
        return Ok(());
    }
    let url = WebviewUrl::External(
        "https://platform.deepseek.com"
            .parse()
            .map_err(|_| "DeepSeek 登录地址无效".to_string())?,
    );
    WebviewWindowBuilder::new(app, WINDOW_LABEL, url)
        .title("DeepSeek 账号登录")
        .inner_size(480.0, 720.0)
        .min_inner_size(360.0, 480.0)
        .resizable(true)
        .center()
        .visible(true)
        .initialization_script(CAPTURE_SCRIPT)
        .on_page_load(|window, payload| {
            if payload
                .url()
                .host_str()
                .is_some_and(|host| host == "platform.deepseek.com")
            {
                let _ = window.eval(CAPTURE_SCRIPT);
            }
        })
        .build()
        .map_err(|error| format!("打开 DeepSeek 登录窗口失败：{error}"))?;
    start_watcher(app.clone());
    Ok(())
}

fn start_watcher(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        for _ in 0..1200 {
            let Some(window) = app.get_webview_window(WINDOW_LABEL) else {
                return;
            };
            let is_deepseek = window
                .url()
                .ok()
                .and_then(|url| url.host_str().map(str::to_string))
                .is_some_and(|host| host == "platform.deepseek.com");
            if !is_deepseek {
                tokio::time::sleep(Duration::from_millis(1500)).await;
                continue;
            }
            if let Ok(title) = window.title() {
                if let Some(token) = title.strip_prefix(TOKEN_TITLE_PREFIX) {
                    let token = token.trim().to_string();
                    let _ = window.set_title("DeepSeek 账号登录");
                    if capture(&app, &token).await.is_ok() {
                        let _ = window.close();
                        let _ = app.emit("source-credential-updated", deepseek::WEB_SOURCE_ID);
                        return;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
    });
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
