//! Grok CLI（SuperGrok）本机订阅额度 Source。
//!
//! 只读本机 Grok CLI 的 OAuth 凭据（`~/.grok/auth.json`，scope → 条目 map，
//! `key` 即 Bearer token）。本模块不调用 xAI OAuth token 端点，也不改写
//! `auth.json`；临近过期或 billing 401/403 时后台跑 `grok models`，交给官方
//! CLI 认证管理器静默续期（OIDC refresh_token，不打开浏览器）。交互登录仍走
//! `login_via_cli`（`grok login`）。凭据不写入本项目存储、日志或前端。
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

use crate::providers::money::WEB_UA;
use chrono::{DateTime, TimeDelta, Utc};
use reqwest::{Client, StatusCode};
use serde_json::Value;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

pub const SOURCE_ID: &str = "grok-cli-local";

const BILLING_ENDPOINT: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
/// SuperGrok（OIDC）条目的 scope 前缀。
const OIDC_SCOPE_PREFIX: &str = "https://auth.x.ai::";
const RELOGIN_HINT: &str = "可在来源行点「重新登录」，或在终端运行 grok login";
/// 与 Grok CLI `GROK_AUTH_EARLY_INVALIDATION_SECS` 默认值一致。
const REFRESH_SKEW: TimeDelta = TimeDelta::minutes(5);
const SILENT_REFRESH_TIMEOUT: Duration = Duration::from_secs(45);

/// 本机 `auth.json` 解析结果。不含 refresh_token 原文，禁止写入日志 / SQLite / ViewModel。
struct GrokAuth {
    access_token: String,
    expires_at: Option<DateTime<Utc>>,
    has_refresh_token: bool,
}

pub fn local_auth_available() -> bool {
    read_access_token().is_some()
}

fn grok_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".grok"))
}

fn grok_refresh_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// auth.json 顶层是 scope → 条目 map：优先 SuperGrok OIDC 条目，
/// 回退旧版 `accounts.x.ai/sign-in` 会话条目；`key` 为空的残缺条目不遮蔽可用条目
/// （口径对齐 cc-switch `select_preferred_entry`）。
fn read_access_token() -> Option<String> {
    read_auth().map(|auth| auth.access_token)
}

fn read_auth() -> Option<GrokAuth> {
    let content = std::fs::read_to_string(grok_home()?.join("auth.json")).ok()?;
    let parsed: Value = serde_json::from_str(&content).ok()?;
    select_auth(parsed.as_object()?)
}

fn select_auth(root: &serde_json::Map<String, Value>) -> Option<GrokAuth> {
    let mut oidc = None;
    let mut legacy = None;
    for (scope, value) in root {
        let Some(entry) = select_auth_entry(value) else {
            continue;
        };
        if scope.starts_with(OIDC_SCOPE_PREFIX) {
            oidc = Some(entry);
        } else if scope.contains("/sign-in") {
            legacy = Some(entry);
        }
    }
    oidc.or(legacy)
}

fn select_auth_entry(value: &Value) -> Option<GrokAuth> {
    let entry = value.as_object()?;
    let access_token = entry
        .get("key")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())?
        .to_string();
    let has_refresh_token = entry
        .get("refresh_token")
        .and_then(Value::as_str)
        .is_some_and(|token| !token.is_empty());
    let expires_at = entry
        .get("expires_at")
        .and_then(Value::as_str)
        .and_then(parse_expires_at);
    Some(GrokAuth {
        access_token,
        expires_at,
        has_refresh_token,
    })
}

fn parse_expires_at(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn token_needs_refresh(expires_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    match expires_at {
        Some(expires_at) => expires_at <= now + REFRESH_SKEW,
        None => false,
    }
}

fn token_still_valid(auth: &GrokAuth, now: DateTime<Utc>) -> bool {
    !auth.access_token.is_empty() && !token_needs_refresh(auth.expires_at, now)
}

/// 读取 Windows 系统代理（Clash/V2Ray 等写入注册表的 ProxyServer）。
/// GUI 启动的应用环境里通常没有 HTTP_PROXY/HTTPS_PROXY，第三方 CLI 子进程
/// 也不会读注册表——需要显式注入，否则 grok login 的 auth.x.ai 请求会超时。
pub(crate) fn windows_system_proxy() -> Option<String> {
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
        &["grok.exe", "grok.cmd", "grok.bat"]
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

fn grok_cli_command(program: &std::path::Path) -> tokio::process::Command {
    let is_exe = program.extension().and_then(|ext| ext.to_str()) == Some("exe");
    if is_exe {
        tokio::process::Command::new(program)
    } else {
        let mut cmd = tokio::process::Command::new("cmd");
        cmd.arg("/c").arg(program);
        cmd
    }
}

fn apply_grok_cli_env(command: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    if let Some(proxy) = windows_system_proxy() {
        command.env("HTTPS_PROXY", &proxy);
        command.env("HTTP_PROXY", &proxy);
    }
}

fn auth_error(code: &str, message: impl Into<String>, auth_required: bool) -> RefreshError {
    RefreshError::new(code, message, auth_required, false)
}

fn classify_silent_refresh_failure(
    output: &str,
    timed_out: bool,
    has_refresh_token: bool,
) -> RefreshError {
    if !has_refresh_token {
        return auth_error(
            "credential_expired",
            format!("Grok refresh token 不可用，请重新登录。{RELOGIN_HINT}"),
            true,
        );
    }
    let joined = output.to_lowercase();
    let network_failed = [
        "timed out",
        "timeout",
        "error sending request",
        "connection refused",
        "connection reset",
        "unreachable",
        "network unreachable",
        "dns",
        "proxyerror",
        "proxy error",
        "tunnel",
    ]
    .iter()
    .any(|mark| joined.contains(mark));
    if timed_out || network_failed {
        return auth_error(
            "network_error",
            "Grok 静默续期失败：无法连接认证服务，请检查系统代理后重试",
            false,
        );
    }
    let revoked = [
        "invalid_grant",
        "revoked",
        "refresh token",
        "unauthorized",
        "login required",
        "re-authenticate",
        "not authenticated",
        "no valid credentials",
    ]
    .iter()
    .any(|mark| joined.contains(mark));
    if revoked {
        return auth_error(
            "credential_expired",
            format!("Grok refresh token 已失效或被撤销，请重新登录。{RELOGIN_HINT}"),
            true,
        );
    }
    auth_error(
        "credential_expired",
        format!("Grok 登录已失效，静默续期未成功。{RELOGIN_HINT}"),
        true,
    )
}

/// 后台跑 `grok models`：本机 CLI 1.0.13 会走 AuthManager / `try_ensure_fresh_auth`，
/// OIDC 有 refresh_token 时静默续期且不打开浏览器；该命令只列模型，不产生推理费用。
/// 禁止走 `grok login`。
async fn run_grok_models_refresh() -> Result<String, (bool, String)> {
    let program = resolve_grok_program().map_err(|message| (false, message))?;
    let mut command = grok_cli_command(&program);
    command
        .arg("models")
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    apply_grok_cli_env(&mut command);
    command.current_dir(std::env::temp_dir());
    let mut child = command
        .spawn()
        .map_err(|error| (false, format!("无法启动 Grok CLI：{error}")))?;
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
    drop(line_tx);
    let timed_out = match tokio::time::timeout(SILENT_REFRESH_TIMEOUT, child.wait()).await {
        Ok(Ok(_)) => false,
        Ok(Err(error)) => return Err((false, format!("Grok CLI 退出异常：{error}"))),
        Err(_) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            true
        }
    };
    let mut recent = Vec::new();
    while let Ok(line) = line_rx.try_recv() {
        if recent.len() < 8 {
            recent.push(line);
        }
    }
    let text = recent.join(" / ");
    if timed_out {
        Err((true, text))
    } else {
        Ok(text)
    }
}

async fn silent_refresh(force: bool, previous_token: &str) -> Result<GrokAuth, RefreshError> {
    let _guard = grok_refresh_lock().lock().await;
    let current = read_auth().ok_or_else(|| {
        auth_error(
            "auth_required",
            format!("未检测到本机 Grok CLI 登录。{RELOGIN_HINT}"),
            true,
        )
    })?;
    let now = Utc::now();
    let token_changed = current.access_token != previous_token;
    if token_changed && token_still_valid(&current, now) {
        return Ok(current);
    }
    if !force && !token_needs_refresh(current.expires_at, now) {
        return Ok(current);
    }
    if !current.has_refresh_token {
        return Err(classify_silent_refresh_failure("", false, false));
    }
    let cli_output = match run_grok_models_refresh().await {
        Ok(text) => text,
        Err((timed_out, message)) => {
            if !timed_out
                && (message.contains("未找到 Grok CLI") || message.contains("无法启动 Grok CLI"))
            {
                return Err(auth_error(
                    "network_error",
                    format!("{message}。若网络需要代理，请检查系统代理"),
                    false,
                ));
            }
            return Err(classify_silent_refresh_failure(&message, timed_out, true));
        }
    };
    let updated = read_auth().ok_or_else(|| {
        classify_silent_refresh_failure(&cli_output, false, true)
    })?;
    let now = Utc::now();
    if updated.access_token.is_empty() {
        return Err(classify_silent_refresh_failure(&cli_output, false, true));
    }
    if updated.access_token != current.access_token || token_still_valid(&updated, now) {
        return Ok(updated);
    }
    Err(classify_silent_refresh_failure(&cli_output, false, true))
}

/// 平台内重新登录：后台静默运行 `grok login`，自动抓取 CLI 打印的授权链接并
/// 打开浏览器完成 OAuth；轮询本机 auth.json 中 token 变化判定登录完成。
/// 超时或取消时把 CLI 的最后输出带回给用户（不再开一个无提示的黑窗）。
pub async fn login_via_cli() -> Result<(), String> {
    let program = resolve_grok_program().map_err(|message| message)?;
    let baseline_token = read_access_token();
    // .cmd/.bat shim 不能直接 CreateProcess（WinError 2），经 cmd /c 包装运行
    let mut command = grok_cli_command(&program);
    command.arg("login").kill_on_drop(false);
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    // 关键：注入系统代理。grok login 首步请求 auth.x.ai 的 OIDC 配置，
    // 子进程不会读注册表代理，缺这个会直接超时（应用内重登卡死的根因）。
    apply_grok_cli_env(&mut command);
    let mut child = command.spawn().map_err(|error| {
        format!("无法启动 Grok 登录：{error}。请确认终端里可以运行 `grok`，或设置 GROK_BIN")
    })?;
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
                        "Grok 登录未完成或已取消。CLI 最后输出：{}{}",
                        recent.iter().cloned().collect::<Vec<_>>().join(" / "),
                        network_failure_hint(&recent)
                    ));
                }
                if std::time::Instant::now() > deadline {
                    let _ = child.kill().await;
                    return Err(format!(
                        "登录超时（3 分钟）。CLI 最后输出：{}{}。也可以在终端手动运行 grok login",
                        recent.iter().cloned().collect::<Vec<_>>().join(" / "),
                        network_failure_hint(&recent)
                    ));
                }
            }
        }
    }
}

/// CLI 输出里出现网络失败特征时，追加可行动的代理提示（auth.x.ai 需要代理可达）。
fn network_failure_hint(recent: &std::collections::VecDeque<String>) -> String {
    let joined = recent
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let failed = [
        "timed out",
        "error sending request",
        "connection refused",
        "connection reset",
        "unreachable",
        "proxy",
    ]
    .iter()
    .any(|mark| joined.contains(mark));
    if failed {
        "。无法连接 auth.x.ai：若网络需要代理访问 xAI，请开启系统代理后重试（登录会自动注入系统代理）；若代理已开仍失败，检查代理软件是否放行 auth.x.ai".to_string()
    } else {
        String::new()
    }
}

/// 从 CLI 输出行里提取第一个 http(s) 链接（授权页/设备码页）。
pub(crate) fn extract_http_url(line: &str) -> Option<&str> {
    let start = line.find("http://").or_else(|| line.find("https://"))?;
    let rest = &line[start..];
    let end = rest
        .find(|ch: char| ch.is_whitespace() || ch == ')' || ch == '"' || ch == '\'' || ch == '`')
        .unwrap_or(rest.len());
    let url = &rest[..end];
    (url.len() > 8).then_some(url)
}

pub async fn fetch(client: &Client) -> SourceRefreshOutput {
    let Some(mut auth) = read_auth() else {
        return SourceRefreshOutput::failure(RefreshError::new(
            "auth_required",
            format!("未检测到本机 Grok CLI 登录。{RELOGIN_HINT}"),
            true,
            false,
        ));
    };
    let mut refreshed = false;
    if token_needs_refresh(auth.expires_at, Utc::now()) {
        match silent_refresh(false, &auth.access_token).await {
            Ok(updated) => {
                auth = updated;
                refreshed = true;
            }
            Err(error) => return SourceRefreshOutput::failure(error),
        }
    }
    match fetch_inner(client, &auth.access_token).await {
        Ok(capabilities) => SourceRefreshOutput::success(capabilities),
        Err(error) if is_grok_auth_status(&error) && !refreshed => {
            match silent_refresh(true, &auth.access_token).await {
                Ok(updated) => match fetch_inner(client, &updated.access_token).await {
                    Ok(capabilities) => SourceRefreshOutput::success(capabilities),
                    Err(retry_error) => SourceRefreshOutput::failure(retry_error),
                },
                Err(refresh_error) => SourceRefreshOutput::failure(refresh_error),
            }
        }
        Err(error) => SourceRefreshOutput::failure(error),
    }
}

fn is_grok_auth_status(error: &RefreshError) -> bool {
    error.code == "credential_expired" || error.auth_required
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
            RefreshError::new(
                "response_shape_changed",
                "Grok 额度返回格式发生变化",
                false,
                false,
            )
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
    let config = body.get("config").ok_or_else(|| {
        RefreshError::new(
            "response_shape_changed",
            "Grok 额度返回格式发生变化",
            false,
            false,
        )
    })?;
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
        assert!(values[0]
            .secondary_value
            .as_deref()
            .unwrap()
            .contains("已使用 73%"));
        assert!(values[0]
            .secondary_value
            .as_deref()
            .unwrap()
            .contains("重置 "));
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
        let error =
            parse(&json!({ "config": { "prepaidBalance": { "val": 0 } } })).expect_err("shape");
        assert_eq!(error.code, "response_shape_changed");
        let error = parse(&json!({ "unexpected": true })).expect_err("no config");
        assert_eq!(error.code, "response_shape_changed");
    }

    #[test]
    fn auth_entry_selection_prefers_oidc_over_legacy() {
        let oidc = format!("{OIDC_SCOPE_PREFIX}client-id");
        let content = json!({
            "https://accounts.x.ai/sign-in": { "key": "legacy-token" },
            &oidc: {
                "key": "oidc-token",
                "refresh_token": "refresh-token",
                "expires_at": "2099-01-01T00:00:00Z"
            }
        });
        let auth = select_auth(content.as_object().unwrap()).expect("oidc");
        assert_eq!(auth.access_token, "oidc-token");
        assert!(auth.has_refresh_token);
        assert_eq!(
            auth.expires_at.map(|value| value.to_rfc3339()),
            Some("2099-01-01T00:00:00+00:00".into())
        );

        let broken = json!({ &oidc: { "key": "" }, "https://accounts.x.ai/sign-in": { "key": "legacy-token" } });
        let legacy = select_auth(broken.as_object().unwrap()).expect("legacy");
        assert_eq!(legacy.access_token, "legacy-token");
        assert!(!legacy.has_refresh_token);

        let other = json!({ "other": { "key": "x" } });
        assert!(select_auth(other.as_object().unwrap()).is_none());
    }

    #[test]
    fn token_needs_refresh_uses_five_minute_skew() {
        let now = DateTime::parse_from_rfc3339("2026-09-01T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let within_skew = DateTime::parse_from_rfc3339("2026-09-01T12:04:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let just_outside = DateTime::parse_from_rfc3339("2026-09-01T12:05:01Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(token_needs_refresh(Some(now), now));
        assert!(token_needs_refresh(Some(within_skew), now));
        assert!(!token_needs_refresh(Some(just_outside), now));
        assert!(!token_needs_refresh(None, now));
    }

    #[test]
    fn silent_refresh_errors_distinguish_revoked_and_proxy() {
        let revoked = classify_silent_refresh_failure("invalid_grant refresh token revoked", false, true);
        assert_eq!(revoked.code, "credential_expired");
        assert!(revoked.auth_required);
        assert!(revoked.message.contains("重新登录"));

        let network = classify_silent_refresh_failure("error sending request timed out", false, true);
        assert_eq!(network.code, "network_error");
        assert!(!network.auth_required);
        assert!(network.message.contains("系统代理"));

        let missing = classify_silent_refresh_failure("", false, false);
        assert_eq!(missing.code, "credential_expired");
        assert!(missing.message.contains("refresh token 不可用"));
    }
}
