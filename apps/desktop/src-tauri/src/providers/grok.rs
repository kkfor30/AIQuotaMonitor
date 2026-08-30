//! Grok CLI（SuperGrok）本机订阅额度 Source。
//!
//! 只读本机 Grok CLI 的 OAuth 凭据（`~/.grok/auth.json`，scope → 条目 map，
//! `key` 即 Bearer token）；token 刷新由 Grok CLI 自己负责，本模块不代刷新、
//! 不把凭据写入本项目存储或前端。过期时提示用户在终端重新 `grok login`。
//!
//! 查询端点来自本机 Grok CLI 实测（`~/.grok/logs/unified.jsonl` 抓取）：
//! `GET https://cli-chat-proxy.grok.com/v1/billing?format=credits`
//! 返回 `{ config: { creditUsagePercent, currentPeriod { start, end }, prepaidBalance } }`，
//! `creditUsagePercent` 是已用百分比（图 /usage 面板的 Weekly limit）。
//! 该端点无公开文档；对照实现为 cc-switch `src-tauri/src/services/subscription_grok.rs`
//! （提交 6243e20a，移植自 CodexBar，走 grok.com gRPC-web 备用路径）。
//! 本模块改造为：JSON 端点 + Capability 快照 + 结构化 RefreshError；
//! proto3 会省略零值 percent 字段，仅有周期无百分比时按 0% 特判，
//! 两者都缺才报 response_shape_changed，不补零不造数。

use crate::domain::refresh::{CapabilityData, RefreshError, SourceRefreshOutput};
use tokio::io::AsyncBufReadExt;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use crate::providers::money::WEB_UA;
use reqwest::{Client, StatusCode};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

pub const SOURCE_ID: &str = "grok-cli-local";

const BILLING_ENDPOINT: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
/// SuperGrok（OIDC）条目的 scope 前缀。
const OIDC_SCOPE_PREFIX: &str = "https://auth.x.ai::";
const RELOGIN_HINT: &str = "可在来源行点「重新登录」，或在终端运行 grok login";

pub fn local_auth_available() -> bool {
    read_access_token().is_some()
}

fn grok_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".grok"))
}

/// auth.json 顶层是 scope → 条目 map：优先 SuperGrok OIDC 条目，
/// 回退旧版 `accounts.x.ai/sign-in` 会话条目；`key` 为空的残缺条目不遮蔽可用条目
/// （口径对齐 cc-switch `select_preferred_entry`）。
fn read_access_token() -> Option<String> {
    let content = std::fs::read_to_string(grok_home()?.join("auth.json")).ok()?;
    let parsed: Value = serde_json::from_str(&content).ok()?;
    select_access_token(parsed.as_object()?)
}

fn select_access_token(root: &serde_json::Map<String, Value>) -> Option<String> {
    let mut oidc = None;
    let mut legacy = None;
    for (scope, value) in root {
        let Some(entry) = value.as_object() else { continue };
        let Some(key) = entry
            .get("key")
            .and_then(Value::as_str)
            .filter(|key| !key.is_empty())
        else {
            continue;
        };
        if scope.starts_with(OIDC_SCOPE_PREFIX) {
            oidc = Some(key.to_string());
        } else if scope.contains("/sign-in") {
            legacy = Some(key.to_string());
        }
    }
    oidc.or(legacy)
}

/// 读取 Windows 系统代理（Clash/V2Ray 等写入注册表的 ProxyServer）。
/// GUI 启动的应用环境里通常没有 HTTP_PROXY/HTTPS_PROXY，第三方 CLI 子进程
/// 也不会读注册表——需要显式注入，否则 grok login 的 auth.x.ai 请求会超时。
fn windows_system_proxy() -> Option<String> {
    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    const NO_WINDOW: u32 = 0x0800_0000;
    let query = |name: &str| -> Option<String> {
        let output = std::process::Command::new("reg")
            .args(["query", KEY, "/v", name])
            .creation_flags(NO_WINDOW)
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        // 行形如 "    ProxyEnable    REG_DWORD    0x1" / "    ProxyServer    REG_SZ    127.0.0.1:7890"
        // DWORD 与 SZ 类型都按空白分段取末位值
        text.lines().find_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.first()?.eq_ignore_ascii_case(name) {
                parts.last().map(|value| value.to_string())
            } else {
                None
            }
        })
    };
    let enabled = query("ProxyEnable")?;
    if enabled != "0x1" {
        return None;
    }
    let server = query("ProxyServer")?;
    // 统一格式 "127.0.0.1:7890" 或分协议 "http=...;https=...;ftp=..."
    if let Some(pos) = server.find("https=") {
        let rest = &server[pos + "https=".len()..];
        let end = rest.find(';').unwrap_or(rest.len());
        let https = rest[..end].trim();
        return (!https.is_empty()).then(|| format!("http://{https}"));
    }
    if server.starts_with("http://") || server.starts_with("https://") {
        return Some(server);
    }
    (!server.is_empty()).then(|| format!("http://{server}"))
}

fn resolve_grok_program() -> Result<std::path::PathBuf, String> {
    if let Some(explicit) = std::env::var_os("GROK_BIN") {
        let path = std::path::PathBuf::from(explicit);
        if path.is_file() {
            return Ok(path);
        }
    }
    let names: &[&str] = if cfg!(windows) {
        &["grok.cmd", "grok.exe", "grok.bat"]
    } else {
        &["grok"]
    };
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            for name in names {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }
    }
    Err("未找到 Grok CLI。请确认终端里可以运行 `grok`，或设置 GROK_BIN 指向可执行文件。".into())
}

/// 平台内重新登录：后台静默运行 `grok login`，自动抓取 CLI 打印的授权链接并
/// 打开浏览器完成 OAuth；轮询本机 auth.json 中 token 变化判定登录完成。
/// 超时或取消时把 CLI 的最后输出带回给用户（不再开一个无提示的黑窗）。
pub async fn login_via_cli() -> Result<(), String> {
    let program = resolve_grok_program().map_err(|message| message)?;
    let baseline_token = read_access_token();
    let mut command = tokio::process::Command::new(program);
    command.arg("login").kill_on_drop(false);
    command.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    // 关键：注入系统代理。grok login 首步请求 auth.x.ai 的 OIDC 配置，
    // 子进程不会读注册表代理，缺这个会直接超时（应用内重登卡死的根因）。
    if let Some(proxy) = windows_system_proxy() {
        command.env("HTTPS_PROXY", &proxy);
        command.env("HTTP_PROXY", &proxy);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("无法启动 Grok 登录：{error}。请确认终端里可以运行 `grok`，或设置 GROK_BIN"))?;
    let (line_tx, mut line_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    if let Some(stdout) = child.stdout.take() {
        let tx = line_tx.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx.send(line);
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let tx = line_tx.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx.send(line);
            }
        });
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(500));
    let mut opened_url = false;
    let mut recent: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    let mut output_closed = false;
    loop {
        tokio::select! {
            maybe_line = line_rx.recv() => {
                match maybe_line {
                    Some(line) => {
                        // 只打开带参数的链接（真实授权页）；discovery JSON 里的裸端点不误开
                        if !opened_url {
                            if let Some(url) = extract_http_url(&line) {
                                if url.contains('?') {
                                    opened_url = true;
                                    let _ = crate::commands::window_commands::open_http_url(url);
                                }
                            }
                        }
                        recent.push_back(line);
                        if recent.len() > 4 {
                            recent.pop_front();
                        }
                    }
                    None => {
                        // 两个输出流都已关闭：进程即将/已经退出
                        output_closed = true;
                    }
                }
            }
            _ = ticker.tick() => {
                if let Some(token) = read_access_token() {
                    if baseline_token.as_deref() != Some(token.as_str()) {
                        // CLI 已把新 token 写回 auth.json：登录完成
                        return Ok(());
                    }
                }
                if output_closed {
                    // 给 CLI 一点收尾写文件的时间再做最终判定
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    if let Some(token) = read_access_token() {
                        if baseline_token.as_deref() != Some(token.as_str()) {
                            return Ok(());
                        }
                    }
                    return Err(format!(
                        "Grok 登录未完成或已取消。CLI 最后输出：{}",
                        recent.iter().cloned().collect::<Vec<_>>().join(" / ")
                    ));
                }
                if std::time::Instant::now() > deadline {
                    let _ = child.kill().await;
                    return Err(format!(
                        "登录超时（3 分钟）。CLI 最后输出：{}。也可以在终端手动运行 grok login",
                        recent.iter().cloned().collect::<Vec<_>>().join(" / ")
                    ));
                }
            }
        }
    }
}

/// 从 CLI 输出行里提取第一个 http(s) 链接（授权页/设备码页）。
fn extract_http_url(line: &str) -> Option<&str> {
    let start = line.find("http://").or_else(|| line.find("https://"))?;
    let rest = &line[start..];
    let end = rest
        .find(|ch: char| ch.is_whitespace() || ch == ')' || ch == '"' || ch == '\'' || ch == '`')
        .unwrap_or(rest.len());
    let url = &rest[..end];
    (url.len() > 8).then_some(url)
}

pub async fn fetch(client: &Client) -> SourceRefreshOutput {
    let Some(token) = read_access_token() else {
        return SourceRefreshOutput::failure(RefreshError::new(
            "auth_required",
            format!("未检测到本机 Grok CLI 登录。{RELOGIN_HINT}"),
            true,
            false,
        ));
    };
    match fetch_inner(client, &token).await {
        Ok(capabilities) => SourceRefreshOutput::success(capabilities),
        Err(error) => SourceRefreshOutput::failure(error),
    }
}

async fn fetch_inner(client: &Client, token: &str) -> Result<Vec<CapabilityData>, RefreshError> {
    let mut last_transport = None;
    for attempt in 0..2 {
        let response = client
            .get(BILLING_ENDPOINT)
            .bearer_auth(token)
            .header("Accept", "application/json")
            .header("User-Agent", WEB_UA)
            .timeout(Duration::from_secs(15))
            .send()
            .await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                last_transport = Some(error);
                if attempt == 0 {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    continue;
                }
                break;
            }
        };
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(RefreshError::new(
                    "credential_expired",
                    format!("Grok CLI 登录已失效。{RELOGIN_HINT}"),
                    true,
                    false,
                ));
            }
            StatusCode::TOO_MANY_REQUESTS => {
                return Err(RefreshError::new(
                    "rate_limited",
                    "Grok 额度请求过于频繁，请稍后重试",
                    false,
                    true,
                ));
            }
            status if status.is_server_error() && attempt == 0 => {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
            status if !status.is_success() => {
                return Err(RefreshError::new(
                    "http_error",
                    format!("Grok 额度查询失败（HTTP {}）", status.as_u16()),
                    false,
                    status.is_server_error(),
                ));
            }
            _ => {}
        }
        let body: Value = response.json().await.map_err(|_| {
            RefreshError::new("response_shape_changed", "Grok 额度返回格式发生变化", false, false)
        })?;
        return parse(&body);
    }
    Err(RefreshError::new(
        "network_error",
        last_transport
            .map(|error| format!("Grok 额度服务暂时无法连接：{error}"))
            .unwrap_or_else(|| "Grok 额度服务暂时无法连接".to_string()),
        false,
        true,
    ))
}

fn parse(body: &Value) -> Result<Vec<CapabilityData>, RefreshError> {
    let config = body
        .get("config")
        .ok_or_else(|| RefreshError::new("response_shape_changed", "Grok 额度返回格式发生变化", false, false))?;
    let period_end = config
        .pointer("/currentPeriod/end")
        .and_then(Value::as_str)
        .filter(|end| !end.is_empty());
    // proto3 零值省略：有周期但无 creditUsagePercent 视为 0%；两者都缺才是结构变化。
    let used = match (
        config.get("creditUsagePercent").and_then(Value::as_f64),
        config.get("currentPeriod").is_some(),
    ) {
        (Some(value), _) => value,
        (None, true) => 0.0,
        (None, false) => {
            return Err(RefreshError::new(
                "response_shape_changed",
                "Grok 额度返回格式发生变化",
                false,
                false,
            ))
        }
    };
    let used = used.clamp(0.0, 100.0);
    let remaining = 100.0 - used;
    let reset = period_end.map(|end| Value::String(end.to_string()));
    Ok(vec![super::coding_plan::window_capability(
        "quota_window_7d",
        "周窗口",
        remaining,
        reset.as_ref(),
    )])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_weekly_window_as_remaining_percent() {
        let values = parse(&json!({
            "config": {
                "creditUsagePercent": 73.0,
                "currentPeriod": {
                    "type": "USAGE_PERIOD_TYPE_WEEKLY",
                    "start": "2026-08-28T07:19:04.698807+00:00",
                    "end": "2026-09-04T07:19:04.698807+00:00"
                },
                "prepaidBalance": { "val": 0 }
            }
        }))
        .expect("grok billing");
        assert_eq!(values[0].capability_id, "quota_window_7d");
        assert_eq!(values[0].value_kind, "percent");
        assert_eq!(values[0].primary_value.as_deref(), Some("27%"));
        assert!(values[0].secondary_value.as_deref().unwrap().contains("已使用 73%"));
        assert!(values[0].secondary_value.as_deref().unwrap().contains("重置 "));
        assert_eq!(values[0].progress, Some(0.27));
    }

    #[test]
    fn zero_usage_without_percent_field_reads_as_zero() {
        let values = parse(&json!({
            "config": { "currentPeriod": { "end": "2026-09-04T07:19:04+00:00" } }
        }))
        .expect("zero usage");
        assert_eq!(values[0].primary_value.as_deref(), Some("100%"));
    }

    #[test]
    fn missing_percent_and_period_is_shape_change() {
        let error = parse(&json!({ "config": { "prepaidBalance": { "val": 0 } } })).expect_err("shape");
        assert_eq!(error.code, "response_shape_changed");
        let error = parse(&json!({ "unexpected": true })).expect_err("no config");
        assert_eq!(error.code, "response_shape_changed");
    }

    #[test]
    fn auth_entry_selection_prefers_oidc_over_legacy() {
        let oidc = format!("{OIDC_SCOPE_PREFIX}client-id");
        let content = json!({
            "https://accounts.x.ai/sign-in": { "key": "legacy-token" },
            &oidc: { "key": "oidc-token", "expires_at": "2099-01-01T00:00:00Z" }
        });
        assert_eq!(select_access_token(content.as_object().unwrap()).as_deref(), Some("oidc-token"));

        let broken = json!({ &oidc: { "key": "" }, "https://accounts.x.ai/sign-in": { "key": "legacy-token" } });
        assert_eq!(
            select_access_token(broken.as_object().unwrap()).as_deref(),
            Some("legacy-token")
        );

        let other = json!({ "other": { "key": "x" } });
        assert_eq!(select_access_token(other.as_object().unwrap()), None);
    }
}
