//! Claude Code 本机订阅额度 Source。
//!
//! 只读本机 Claude Code 的 OAuth 凭据并查询官方用量接口：
//! - 凭据：`~/.claude/.credentials.json`，`claudeAiOauth`（兼容 `claude.ai_oauth`）
//!   条目下的 `accessToken`；token 刷新由 Claude CLI 自己负责，本模块不代刷新，
//!   过期时提示用户在终端重新 `claude /login`。
//! - 查询：`GET https://api.anthropic.com/api/oauth/usage`（Bearer + `anthropic-beta:
//!   oauth-2025-04-20`），返回 `five_hour` / `seven_day` / `seven_day_opus` /
//!   `seven_day_sonnet` 等窗口的 `{ utilization, resets_at }`（utilization 为已用百分比）。
//!
//! 凭据与端点口径参考 cc-switch `src-tauri/src/services/subscription.rs`（提交 6243e20a）；
//! 改造为 local_cli Source + Capability 快照与结构化 RefreshError。API Key 模式的
//! Claude Code 没有订阅用量接口，未检测到 OAuth 登录时返回 auth_required 并引导终端登录。

use super::coding_plan::window_capability;
use super::grok::{extract_http_url, windows_system_proxy};
use crate::domain::refresh::{CapabilityData, RefreshError, SourceRefreshOutput};
use crate::providers::money::WEB_UA;
use reqwest::{Client, StatusCode};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tauri::Emitter;
use tokio::io::AsyncBufReadExt;

pub const SOURCE_ID: &str = "claude-code-local";

const USAGE_ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const RELOGIN_HINT: &str = "可在应用内点击“重新登录”，或在终端运行 claude 并执行 /login";

/// 已知窗口 → (capability_id, 展示名)。Opus / Sonnet 是同周期内的模型专属子窗口，
/// capability_id 必须与 seven_day 区分，避免同 Source 下互相覆盖。
const KNOWN_WINDOWS: &[(&str, &str, &str)] = &[
    ("five_hour", "quota_window_5h", "5 小时窗口"),
    ("seven_day", "quota_window_7d", "周窗口"),
    ("seven_day_opus", "quota_window_7d_opus", "周窗口 · Opus"),
    (
        "seven_day_sonnet",
        "quota_window_7d_sonnet",
        "周窗口 · Sonnet",
    ),
];

pub fn local_auth_available() -> bool {
    read_access_token().is_some()
}

fn resolve_claude_program() -> Result<PathBuf, String> {
    if let Some(explicit) = std::env::var_os("CLAUDE_BIN") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Ok(path);
        }
    }
    // 原生 claude.exe 可直接 CreateProcess；npm 的 claude.cmd 是 shim，
    // 直接 spawn 会 WinError 2，必须经 cmd /c（spawn 侧判断扩展名处理）。
    let names: &[&str] = if cfg!(windows) {
        &["claude.exe", "claude.cmd", "claude.bat"]
    } else {
        &["claude"]
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
    Err(
        "未找到 Claude Code CLI。请确认终端里可以运行 `claude`，或设置 CLAUDE_BIN 指向可执行文件。"
            .into(),
    )
}

/// 进行中的 Claude 登录会话：CLI 的授权码要粘贴回 stdin（浏览器回调页只展示 code，
/// 没有 localhost 自动回调），前端弹框收码后经 submit_login_code 写入。
static LOGIN_STDIN: std::sync::OnceLock<tokio::sync::Mutex<Option<tokio::process::ChildStdin>>> =
    std::sync::OnceLock::new();

pub async fn submit_login_code(code: &str) -> Result<(), String> {
    let slot = LOGIN_STDIN
        .get()
        .ok_or_else(|| "当前没有进行中的 Claude 登录".to_string())?;
    let mut guard = slot.lock().await;
    let stdin = guard
        .as_mut()
        .ok_or_else(|| "当前没有进行中的 Claude 登录".to_string())?;
    use tokio::io::AsyncWriteExt;
    let result = async {
        stdin.write_all(code.trim().as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok(())
    }
    .await;
    result.map_err(|error: std::io::Error| format!("授权码写入登录进程失败：{error}"))
}

fn take_login_stdin() {
    if let Some(slot) = LOGIN_STDIN.get() {
        if let Ok(mut guard) = slot.try_lock() {
            *guard = None;
        }
    }
}

/// 平台内重新登录：后台静默运行 `claude auth login`（Claude 订阅 OAuth，浏览器授权）。
/// CLI 会自己打开浏览器并打印授权链接（抓到后兜底再开一次）；用户在浏览器完成授权后
/// 页面显示授权码，前端弹框收集并经 submit_login_code 写回 CLI stdin。轮询
/// `.credentials.json` 中 accessToken 出现/变化判定登录完成；超时或取消时把 CLI
/// 的最后输出带回给用户。
pub async fn login_via_cli(app: tauri::AppHandle) -> Result<(), String> {
    let program = resolve_claude_program()?;
    let baseline_token = read_access_token();
    // .cmd/.bat shim 不能直接 CreateProcess，经 cmd /c 包装运行
    let is_exe = program.extension().and_then(|ext| ext.to_str()) == Some("exe");
    let mut command = if is_exe {
        tokio::process::Command::new(&program)
    } else {
        let mut cmd = tokio::process::Command::new("cmd");
        cmd.arg("/c").arg(&program);
        cmd
    };
    command
        .args(["auth", "login", "--claudeai"])
        .kill_on_drop(false);
    command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    // api.anthropic.com / claude.ai 直连大概率不可达：与 Grok 一致注入注册表系统代理
    if let Some(proxy) = windows_system_proxy() {
        command.env("HTTPS_PROXY", &proxy);
        command.env("HTTP_PROXY", &proxy);
    }
    let mut child = command.spawn().map_err(|error| {
        format!("无法启动 Claude 登录：{error}。请确认终端里可以运行 `claude`，或设置 CLAUDE_BIN")
    })?;
    let slot = LOGIN_STDIN.get_or_init(|| tokio::sync::Mutex::new(None));
    if let Some(stdin) = child.stdin.take() {
        if let Ok(mut guard) = slot.try_lock() {
            *guard = Some(stdin);
        }
    }
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
                        // 只打开带参数的链接（真实授权页），裸端点不误开；
                        // CLI 会自己开浏览器，这里兜底再开一次并通知前端弹授权码输入框
                        if !opened_url {
                            if let Some(url) = extract_http_url(&line) {
                                if url.contains('?') {
                                    opened_url = true;
                                    let _ = crate::commands::window_commands::open_http_url(url);
                                    let _ = app.emit("source-login-code-prompt", SOURCE_ID);
                                }
                            }
                        }
                        recent.push_back(line);
                        if recent.len() > 4 {
                            recent.pop_front();
                        }
                    }
                    None => output_closed = true,
                }
            }
            _ = ticker.tick() => {
                if let Some(token) = read_access_token() {
                    if baseline_token.as_deref() != Some(token.as_str()) {
                        // CLI 已把 OAuth token 写回 .credentials.json：登录完成
                        take_login_stdin();
                        return Ok(());
                    }
                }
                if output_closed {
                    // 给 CLI 一点收尾写文件的时间再做最终判定
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    if let Some(token) = read_access_token() {
                        if baseline_token.as_deref() != Some(token.as_str()) {
                            take_login_stdin();
                            return Ok(());
                        }
                    }
                    take_login_stdin();
                    return Err(format!(
                        "Claude 登录未完成或已取消。CLI 最后输出：{}{}",
                        recent.iter().cloned().collect::<Vec<_>>().join(" / "),
                        network_failure_hint(&recent)
                    ));
                }
                if std::time::Instant::now() > deadline {
                    let _ = child.kill().await;
                    take_login_stdin();
                    return Err(format!(
                        "登录超时（3 分钟）。CLI 最后输出：{}{}。也可以在终端手动运行 `claude /login`",
                        recent.iter().cloned().collect::<Vec<_>>().join(" / "),
                        network_failure_hint(&recent)
                    ));
                }
            }
        }
    }
}

/// CLI 输出里出现网络失败特征时，追加可行动的代理提示（Anthropic 域名需代理可达）。
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
        "fetch failed",
    ]
    .iter()
    .any(|mark| joined.contains(mark));
    if failed {
        "。无法连接 Anthropic：若网络需要代理访问 api.anthropic.com / claude.ai，请开启系统代理后重试（登录会自动注入系统代理）；若代理已开仍失败，检查代理软件是否放行相关域名".to_string()
    } else {
        String::new()
    }
}

fn claude_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".claude"))
}

/// `.credentials.json`：`claudeAiOauth`（兼容 `claude.ai_oauth`）→ `accessToken` 非空。
/// 残缺条目不算已登录；expiresAt 过期仍返回 token，由接口 401 给出真实失效错误。
fn read_access_token() -> Option<String> {
    let content = std::fs::read_to_string(claude_home()?.join(".credentials.json")).ok()?;
    let parsed: Value = serde_json::from_str(&content).ok()?;
    let entry = parsed
        .get("claudeAiOauth")
        .or_else(|| parsed.get("claude.ai_oauth"))?;
    entry
        .get("accessToken")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
}

pub async fn fetch(client: &Client) -> SourceRefreshOutput {
    let Some(token) = read_access_token() else {
        return SourceRefreshOutput::failure(RefreshError::new(
            "auth_required",
            format!("未检测到本机 Claude Code 登录。{RELOGIN_HINT}"),
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
            .get(USAGE_ENDPOINT)
            .bearer_auth(token)
            .header("anthropic-beta", OAUTH_BETA)
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
                    format!("Claude 登录已失效。{RELOGIN_HINT}"),
                    true,
                    false,
                ));
            }
            StatusCode::TOO_MANY_REQUESTS => {
                return Err(RefreshError::new(
                    "rate_limited",
                    "Claude 用量请求过于频繁，请稍后重试",
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
                    format!("Claude 用量查询失败（HTTP {}）", status.as_u16()),
                    false,
                    status.is_server_error(),
                ));
            }
            _ => {}
        }
        let body: Value = response.json().await.map_err(|_| {
            RefreshError::new(
                "response_shape_changed",
                "Claude 用量返回格式发生变化",
                false,
                false,
            )
        })?;
        return parse(&body);
    }
    Err(RefreshError::new(
        "network_error",
        last_transport
            .map(|error| format!("Claude 用量服务暂时无法连接：{error}"))
            .unwrap_or_else(|| "Claude 用量服务暂时无法连接".to_string()),
        false,
        true,
    ))
}

/// 每个窗口 `{ utilization, resets_at }`（utilization 已用百分比）转剩余展示。
/// 无任何已知窗口视为结构变化，不补零；utilization 缺失的窗口跳过。
fn parse(body: &Value) -> Result<Vec<CapabilityData>, RefreshError> {
    let mut capabilities = Vec::new();
    for (key, capability_id, label) in KNOWN_WINDOWS {
        let Some(window) = body.get(*key) else {
            continue;
        };
        let Some(used) = window.get("utilization").and_then(Value::as_f64) else {
            continue;
        };
        let used = used.clamp(0.0, 100.0);
        let reset = window
            .get("resets_at")
            .and_then(Value::as_str)
            .filter(|r| !r.is_empty());
        let reset = reset.map(|value| Value::String(value.to_string()));
        capabilities.push(window_capability(
            capability_id,
            label,
            100.0 - used,
            reset.as_ref(),
        ));
    }
    if capabilities.is_empty() {
        return Err(RefreshError::new(
            "response_shape_changed",
            "Claude 用量返回格式发生变化",
            false,
            false,
        ));
    }
    Ok(capabilities)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_known_windows_as_remaining_percent() {
        let values = parse(&json!({
            "five_hour": { "utilization": 36.2, "resets_at": "2026-08-30T12:00:00Z" },
            "seven_day": { "utilization": 12.0, "resets_at": "2026-09-02T08:00:00Z" },
            "seven_day_opus": { "utilization": 71.0 }
        }))
        .expect("claude usage");
        assert_eq!(values.len(), 3);
        assert_eq!(values[0].capability_id, "quota_window_5h");
        assert_eq!(values[0].primary_value.as_deref(), Some("63.8%"));
        assert_eq!(values[0].progress, Some(0.638));
        assert!(values[0]
            .secondary_value
            .as_deref()
            .unwrap()
            .contains("已使用 36.2%"));
        assert!(values[0]
            .secondary_value
            .as_deref()
            .unwrap()
            .contains("重置 "));
        assert_eq!(values[1].capability_id, "quota_window_7d");
        assert_eq!(values[2].capability_id, "quota_window_7d_opus");
        assert_eq!(values[2].display_name, "周窗口 · Opus");
    }

    #[test]
    fn missing_windows_is_shape_change_not_zero() {
        let error = parse(&json!({ "unexpected": true })).expect_err("empty");
        assert_eq!(error.code, "response_shape_changed");
        // utilization 缺失的窗口跳过，不按 0 处理
        let error = parse(&json!({ "five_hour": {} })).expect_err("missing utilization");
        assert_eq!(error.code, "response_shape_changed");
        // 有任一有效窗口即可，其余缺失窗口不出现
        let values = parse(&json!({
            "five_hour": { "utilization": 10.0 },
            "seven_day": { "utilization": "bad" }
        }))
        .expect("one valid window");
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].capability_id, "quota_window_5h");
    }

    #[test]
    fn reads_oauth_token_across_key_aliases() {
        assert_eq!(
            read_token_from(&json!({ "claudeAiOauth": { "accessToken": " token " } })).as_deref(),
            Some("token")
        );
        assert_eq!(
            read_token_from(&json!({ "claude.ai_oauth": { "accessToken": "t2" } })).as_deref(),
            Some("t2")
        );
        assert_eq!(
            read_token_from(&json!({ "claudeAiOauth": { "accessToken": "" } })),
            None
        );
        assert_eq!(read_token_from(&json!({ "other": {} })), None);
    }

    fn read_token_from(content: &Value) -> Option<String> {
        let entry = content
            .get("claudeAiOauth")
            .or_else(|| content.get("claude.ai_oauth"))?;
        entry
            .get("accessToken")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_string)
    }
}
