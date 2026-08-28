//! GPT/Codex 本地订阅额度 Source。
//!
//! 先调用 `codex app-server` 的 `account/rateLimits/read`，仅在 CLI/协议不可用时
//! 回退到 ChatGPT WHAM。认证只读本机 Codex OAuth，不复制到本项目数据库或 Vault。

use crate::domain::refresh::{CapabilityData, RefreshError, SourceRefreshOutput};
use reqwest::{Client, StatusCode};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

pub const SOURCE_ID: &str = "openai-codex-local";

#[derive(Debug)]
enum AppServerError {
    Unavailable(String),
    Protocol(String),
    Network(String),
    Credential(String),
    Unsupported(String),
}

impl AppServerError {
    fn message(&self) -> &str {
        match self {
            Self::Unavailable(message)
            | Self::Protocol(message)
            | Self::Network(message)
            | Self::Credential(message)
            | Self::Unsupported(message) => message,
        }
    }

    fn into_refresh(self) -> RefreshError {
        match self {
            Self::Unavailable(message) | Self::Network(message) => {
                RefreshError::new("codex_unavailable", message, false, true)
            }
            Self::Protocol(message) => {
                RefreshError::new("codex_protocol_changed", message, false, false)
            }
            Self::Credential(message) => {
                RefreshError::new("credential_expired", message, true, false)
            }
            Self::Unsupported(message) => {
                RefreshError::new("subscription_unsupported", message, false, false)
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct CodexTokens {
    access_token: String,
    #[serde(default)]
    account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CodexAuth {
    #[serde(default)]
    tokens: Option<CodexTokens>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    auth_mode: Option<String>,
    #[serde(default, rename = "OPENAI_API_KEY")]
    openai_api_key: Option<String>,
    #[serde(default)]
    last_refresh: Option<String>,
}

pub fn local_auth_available() -> bool {
    read_auth().is_ok()
}

pub async fn fetch(client: &Client) -> SourceRefreshOutput {
    match fetch_app_server().await {
        Ok(capabilities) => SourceRefreshOutput::success(capabilities),
        Err(error @ (AppServerError::Credential(_) | AppServerError::Unsupported(_))) => {
            SourceRefreshOutput::failure(error.into_refresh())
        }
        Err(app_error) => match fetch_wham(client).await {
            Ok(capabilities) => SourceRefreshOutput::success(capabilities),
            Err(wham_error) => SourceRefreshOutput::failure(combine_codex_errors(app_error, wham_error)),
        },
    }
}

pub async fn login_cli() -> Result<(), String> {
    let program = resolve_codex_program().map_err(|error| error.message().to_string())?;
    let mut command = Command::new(&program);
    command.arg("login").kill_on_drop(false);
    #[cfg(windows)]
    {
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        command.creation_flags(CREATE_NEW_CONSOLE);
    }
    let status = command
        .status()
        .await
        .map_err(|error| format!("无法启动 Codex 登录：{error}"))?;
    if !status.success() {
        return Err("Codex 登录未完成或已取消".into());
    }
    if !local_auth_available() {
        return Err("登录窗口已关闭，但仍未检测到 Codex ChatGPT 登录".into());
    }
    Ok(())
}

pub fn logout_cli() -> Result<(), String> {
    if let Ok(program) = resolve_codex_program() {
        let mut command = std::process::Command::new(program);
        command.arg("logout");
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let _ = command.output();
    }
    if local_auth_available() {
        archive_auth()?;
    }
    if local_auth_available() {
        return Err("本机 Codex 登录仍存在，未能清除。可在终端执行 `codex logout` 后重试。".into());
    }
    Ok(())
}

fn combine_codex_errors(app_error: AppServerError, mut wham_error: RefreshError) -> RefreshError {
    let app = app_error.message().to_string();
    let stale = token_stale_hint();
    if wham_error.code == "network_error" {
        wham_error.message = format!(
            "GPT 额度未刷新：chatgpt.com 当前无法连接。本机 Codex app-server 也未能读取额度（{app}）。请检查代理/网络{}，或在接入与来源中重新登录 Codex CLI。",
            stale.unwrap_or_default()
        );
    } else {
        wham_error.message = format!(
            "{}。本机 Codex app-server：{app}{}",
            wham_error.message,
            stale.unwrap_or_default()
        );
    }
    wham_error
}

fn token_stale_hint() -> Option<String> {
    let auth = read_auth().ok()?;
    let last_refresh = auth.last_refresh.as_deref()?;
    is_codex_token_stale(last_refresh).then(|| "；本机登录距上次刷新已超过 8 天，建议重新登录".into())
}

fn is_codex_token_stale(last_refresh: &str) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    chrono::DateTime::parse_from_rfc3339(last_refresh)
        .ok()
        .is_some_and(|time| now.saturating_sub(time.timestamp().max(0) as u64) > 8 * 24 * 3600)
}

fn archive_auth() -> Result<(), String> {
    let path = auth_path().map_err(|error| error.message)?;
    if !path.exists() {
        return Ok(());
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup = path.with_file_name(format!("auth.json.bak-{stamp}"));
    std::fs::rename(&path, backup).map_err(|error| format!("备份 Codex 本机登录失败：{error}"))
}

fn resolve_codex_program() -> Result<PathBuf, AppServerError> {
    if let Some(explicit) = std::env::var_os("CODEX_BIN") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Ok(path);
        }
    }
    for dir in codex_search_dirs() {
        if let Some(path) = find_codex_in_dir(&dir) {
            return Ok(path);
        }
    }
    Err(AppServerError::Unavailable(
        "未找到 Codex CLI。请确认终端里可以运行 `codex`，或设置 CODEX_BIN 指向可执行文件。".into(),
    ))
}

fn find_codex_in_dir(dir: &Path) -> Option<PathBuf> {
    let names = if cfg!(windows) {
        ["codex.cmd", "codex.exe", "codex.bat"].as_slice()
    } else {
        ["codex"].as_slice()
    };
    names.iter().map(|name| dir.join(name)).find(|path| path.is_file())
}

fn codex_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    for extra in [
        std::env::var_os("NVM_SYMLINK").map(PathBuf::from),
        std::env::var_os("NVM_HOME").map(|home| PathBuf::from(home).join("nodejs")),
        std::env::var_os("APPDATA").map(|home| PathBuf::from(home).join("npm")),
        std::env::var_os("LOCALAPPDATA").map(|home| PathBuf::from(home).join("pnpm")),
        std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".cargo").join("bin")),
    ]
    .into_iter()
    .flatten()
    {
        if extra.is_dir() && !dirs.iter().any(|dir| dir == &extra) {
            dirs.push(extra);
        }
    }
    dirs
}

async fn fetch_app_server() -> Result<Vec<CapabilityData>, AppServerError> {
    let program = resolve_codex_program()?;
    let mut command = Command::new(&program);
    command
        .arg("app-server")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|error| {
        AppServerError::Unavailable(format!(
            "无法启动 Codex app-server（{}）：{error}",
            program.display()
        ))
    })?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| AppServerError::Unavailable("Codex app-server 缺少 stdin".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppServerError::Unavailable("Codex app-server 缺少 stdout".into()))?;
    let mut stdout = BufReader::new(stdout);

    let result = async {
        let _ = rpc_call(
            &mut stdin,
            &mut stdout,
            1,
            "initialize",
            Some(json!({
                "clientInfo": { "name": "ai-quota-monitor", "version": "0.1" },
                "capabilities": {}
            })),
            true,
        )
        .await?;
        write_json_line(&mut stdin, &json!({"method": "initialized", "params": {}})).await?;
        let account = rpc_call(
            &mut stdin,
            &mut stdout,
            2,
            "account/read",
            Some(json!({"refreshToken": false})),
            false,
        )
        .await?;
        let rate_limits = rpc_call(
            &mut stdin,
            &mut stdout,
            3,
            "account/rateLimits/read",
            None,
            false,
        )
        .await?;
        parse_app_server(account, rate_limits)
    }
    .await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

async fn rpc_call(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    id: u64,
    method: &str,
    params: Option<Value>,
    startup_stage: bool,
) -> Result<Value, AppServerError> {
    let mut request = serde_json::Map::new();
    request.insert("id".into(), Value::from(id));
    request.insert("method".into(), Value::from(method));
    if let Some(params) = params {
        request.insert("params".into(), params);
    }
    write_json_line(stdin, &Value::Object(request)).await?;
    tokio::time::timeout(Duration::from_secs(15), read_response(stdout, id, method))
        .await
        .map_err(|_| {
            if startup_stage {
                AppServerError::Unavailable("Codex app-server 初始化超时".into())
            } else {
                AppServerError::Network(format!("Codex app-server {method} 请求超时"))
            }
        })?
}

async fn write_json_line(stdin: &mut ChildStdin, value: &Value) -> Result<(), AppServerError> {
    let mut bytes = serde_json::to_vec(value)
        .map_err(|_| AppServerError::Protocol("无法编码 Codex app-server 请求".into()))?;
    bytes.push(b'\n');
    stdin
        .write_all(&bytes)
        .await
        .map_err(|error| AppServerError::Unavailable(format!("无法写入 Codex app-server：{error}")))?;
    stdin
        .flush()
        .await
        .map_err(|error| AppServerError::Unavailable(format!("无法启动 Codex app-server 请求：{error}")))
}

async fn read_response(
    stdout: &mut BufReader<ChildStdout>,
    id: u64,
    method: &str,
) -> Result<Value, AppServerError> {
    let mut line = String::new();
    for _ in 0..32 {
        line.clear();
        let count = stdout
            .read_line(&mut line)
            .await
            .map_err(|error| AppServerError::Unavailable(format!("读取 Codex app-server 失败：{error}")))?;
        if count == 0 {
            return Err(AppServerError::Protocol(format!("Codex app-server 在 {method} 响应前退出")));
        }
        let response: Value = serde_json::from_str(line.trim())
            .map_err(|_| AppServerError::Protocol("Codex app-server 返回了非 JSON 内容".into()))?;
        if response.get("id").and_then(Value::as_u64) != Some(id) {
            continue;
        }
        if let Some(error) = response.get("error") {
            return Err(classify_rpc_error(error, method));
        }
        return response
            .get("result")
            .cloned()
            .ok_or_else(|| AppServerError::Protocol(format!("Codex app-server {method} 缺少 result")));
    }
    Err(AppServerError::Protocol(format!("Codex app-server 未匹配 {method} 响应")))
}

fn classify_rpc_error(error: &Value, method: &str) -> AppServerError {
    let message = error.get("message").and_then(Value::as_str).unwrap_or("未知错误");
    let detail = format!("Codex app-server {method} 失败：{message}");
    let code = error.get("code").and_then(Value::as_i64);
    let data = error.get("data");
    let kind = data
        .and_then(|value| value.get("kind").or_else(|| value.get("type")))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let status = data
        .and_then(|value| value.get("status").or_else(|| value.get("httpStatus")))
        .and_then(Value::as_i64);
    if matches!(code, Some(-32601 | -32602)) || matches!(kind.as_str(), "protocol" | "unsupported_method" | "method_not_found") {
        AppServerError::Protocol(detail)
    } else if matches!(code, Some(401 | 403)) || matches!(status, Some(401 | 403)) || matches!(kind.as_str(), "credential" | "authentication" | "authorization") {
        AppServerError::Credential(detail)
    } else if matches!(kind.as_str(), "subscription" | "unsupported" | "no_active_plan") {
        AppServerError::Unsupported(detail)
    } else {
        AppServerError::Network(detail)
    }
}

fn parse_app_server(account: Value, rate_limits: Value) -> Result<Vec<CapabilityData>, AppServerError> {
    let account = account.get("account").unwrap_or(&account);
    if account_uses_api_key(account) {
        return Err(AppServerError::Unsupported("API Key 登录不支持个人订阅额度".into()));
    }
    let body = rate_limits
        .get("rateLimits")
        .or_else(|| rate_limits.get("rate_limits"))
        .unwrap_or(&rate_limits);
    let mut capabilities = parse_rate_limit_container(body);
    if let Some(by_id) = rate_limits
        .get("rateLimitsByLimitId")
        .or_else(|| rate_limits.get("rate_limits_by_limit_id"))
        .and_then(Value::as_object)
    {
        for value in by_id.values() {
            capabilities.extend(parse_rate_limit_container(value));
        }
    }
    let plan = account
        .get("planType")
        .or_else(|| account.get("plan_type"))
        .or_else(|| account.get("plan"))
        .and_then(Value::as_str);
    append_plan_and_credits(&mut capabilities, plan, body);
    if body != &rate_limits {
        append_plan_and_credits(&mut capabilities, None, &rate_limits);
    }
    deduplicate_capabilities(&mut capabilities);
    if capabilities.iter().all(|value| !value.capability_id.starts_with("quota_window_")) {
        return Err(AppServerError::Protocol("Codex app-server 未返回可识别的订阅窗口".into()));
    }
    Ok(capabilities)
}

async fn fetch_wham(client: &Client) -> Result<Vec<CapabilityData>, RefreshError> {
    let auth = read_auth()?;
    let tokens = auth.tokens.as_ref().ok_or_else(|| {
        RefreshError::new("auth_required", "未登录 GPT：Codex CLI 缺少 OAuth 凭据", true, false)
    })?;
    let account_id = tokens.account_id.as_deref().or(auth.account_id.as_deref());
    for attempt in 0..2 {
        let mut request = client
            .get("https://chatgpt.com/backend-api/wham/usage")
            .bearer_auth(tokens.access_token.trim())
            .header("Accept", "application/json")
            .header("User-Agent", "codex-cli");
        if let Some(account_id) = account_id.filter(|value| !value.trim().is_empty()) {
            request = request.header("ChatGPT-Account-Id", account_id);
        }
        let response = match request.timeout(Duration::from_secs(15)).send().await {
            Ok(response) => response,
            Err(_) if attempt == 0 => {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
            Err(error) => {
                return Err(RefreshError::new(
                    "network_error",
                    format!("GPT 额度服务暂时无法连接：{error}"),
                    false,
                    true,
                ));
            }
        };
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(RefreshError::new(
                    "credential_expired",
                    "GPT 登录凭据无效或已过期，请重新登录 Codex CLI",
                    true,
                    false,
                ));
            }
            StatusCode::TOO_MANY_REQUESTS => {
                return Err(RefreshError::new("rate_limited", "GPT 额度服务请求过于频繁", false, true));
            }
            status if status.is_server_error() && attempt == 0 => {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
            status if !status.is_success() => {
                return Err(RefreshError::new(
                    "http_error",
                    format!("GPT 额度查询失败（HTTP {}）", status.as_u16()),
                    false,
                    status.is_server_error(),
                ));
            }
            _ => {}
        }
        let body = response.json::<Value>().await.map_err(|_| {
            RefreshError::new("response_shape_changed", "GPT 额度返回格式发生变化", false, false)
        })?;
        let mut capabilities = body
            .get("rate_limit")
            .map(parse_rate_limit_container)
            .unwrap_or_default();
        if let Some(additional) = body.get("additional_rate_limits").and_then(Value::as_array) {
            for limit in additional {
                if let Some(value) = limit.get("rate_limit").or_else(|| limit.get("limit")) {
                    capabilities.extend(parse_rate_limit_container(value));
                }
            }
        }
        append_plan_and_credits(
            &mut capabilities,
            body.get("plan_type").and_then(Value::as_str),
            &body,
        );
        deduplicate_capabilities(&mut capabilities);
        if capabilities.iter().all(|value| !value.capability_id.starts_with("quota_window_")) {
            return Err(RefreshError::new(
                "missing_quota",
                "GPT 已登录，但未返回个人套餐额度",
                false,
                false,
            ));
        }
        return Ok(capabilities);
    }
    unreachable!()
}

fn parse_rate_limit_container(value: &Value) -> Vec<CapabilityData> {
    let value = value.get("rate_limit").or_else(|| value.get("rateLimit")).unwrap_or(value);
    ["primary_window", "secondary_window", "primary", "secondary"]
        .iter()
        .filter_map(|field| value.get(field))
        .filter_map(window_capability)
        .collect()
}

fn window_capability(window: &Value) -> Option<CapabilityData> {
    let used = number(
        window
            .get("used_percent")
            .or_else(|| window.get("utilization_percent"))
            .or_else(|| window.get("usedPercent")),
    )?
    .clamp(0.0, 100.0);
    let duration = number(
        window
            .get("duration_seconds")
            .or_else(|| window.get("limit_window_seconds"))
            .or_else(|| window.get("windowDurationSeconds")),
    )
    .map(|value| value.round() as u64)
    .or_else(|| {
        number(window.get("window_duration_mins").or_else(|| window.get("windowDurationMins")))
            .map(|value| (value * 60.0).round() as u64)
    })?;
    let (id, label) = match duration {
        18_000 => ("quota_window_5h".to_string(), "5 小时窗口".to_string()),
        604_800 => ("quota_window_7d".to_string(), "7 天窗口".to_string()),
        value => (format!("quota_window_{value}s"), duration_label(value)),
    };
    let remaining = (100.0 - used).clamp(0.0, 100.0);
    let reset = reset_label(window);
    Some(CapabilityData {
        capability_id: id,
        display_name: label,
        value_kind: "percent".into(),
        primary_value: Some(format!("{remaining:.1}%")),
        secondary_value: Some(match reset {
            Some(reset) => format!("已使用 {used:.1}% · {reset}"),
            None => format!("已使用 {used:.1}%"),
        }),
        progress: Some(remaining / 100.0),
        trend: vec![],
    })
}

fn append_plan_and_credits(capabilities: &mut Vec<CapabilityData>, plan: Option<&str>, body: &Value) {
    if let Some(plan) = plan.filter(|value| !value.trim().is_empty()) {
        capabilities.push(CapabilityData {
            capability_id: "plan_level".into(),
            display_name: "订阅计划".into(),
            value_kind: "text".into(),
            primary_value: Some(title_case(plan)),
            secondary_value: Some("Codex 本地账户".into()),
            progress: None,
            trend: vec![],
        });
    }
    let credits = body
        .get("credits")
        .and_then(|value| value.get("balance"))
        .and_then(decimal_text);
    if let Some(credits) = credits {
        capabilities.push(CapabilityData {
            capability_id: "credits".into(),
            display_name: "Credits 余额".into(),
            value_kind: "credits".into(),
            primary_value: Some(credits),
            secondary_value: Some("仅展示额度接口实际返回值".into()),
            progress: None,
            trend: vec![],
        });
    }
}

fn deduplicate_capabilities(capabilities: &mut Vec<CapabilityData>) {
    let mut seen = std::collections::HashSet::new();
    capabilities.retain(|value| seen.insert(value.capability_id.clone()));
}

fn account_uses_api_key(account: &Value) -> bool {
    ["authMode", "auth_mode", "loginType", "login_type", "type"]
        .iter()
        .filter_map(|key| account.get(key).and_then(Value::as_str))
        .any(|value| matches!(value.to_ascii_lowercase().as_str(), "api_key" | "apikey" | "api key"))
}

fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn decimal_text(value: &Value) -> Option<String> {
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => return None,
    };
    Decimal::from_str(&text).ok().map(|value| value.normalize().to_string())
}

fn reset_label(window: &Value) -> Option<String> {
    let value = window.get("reset_at").or_else(|| window.get("resetsAt"))?;
    if let Some(text) = value.as_str().filter(|text| text.contains('T')) {
        return Some(format!("重置 {text}"));
    }
    let timestamp = value
        .as_i64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<i64>().ok()))?;
    let seconds = if timestamp > 10_000_000_000 { timestamp / 1000 } else { timestamp };
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|time| format!("重置 {}", time.with_timezone(&chrono::Local).format("%m-%d %H:%M")))
}

fn duration_label(seconds: u64) -> String {
    if seconds % 86_400 == 0 {
        format!("{} 天窗口", seconds / 86_400)
    } else if seconds % 3_600 == 0 {
        format!("{} 小时窗口", seconds / 3_600)
    } else {
        format!("{} 分钟窗口", seconds.div_ceil(60))
    }
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| format!("{}{}", first.to_uppercase(), chars.as_str()))
        .unwrap_or_else(|| "GPT".into())
}

fn auth_path() -> Result<PathBuf, RefreshError> {
    let profile = std::env::var_os("USERPROFILE").ok_or_else(|| {
        RefreshError::new("auth_required", "无法定位 Codex CLI 本机登录信息", true, false)
    })?;
    Ok(PathBuf::from(profile).join(".codex").join("auth.json"))
}

fn read_auth() -> Result<CodexAuth, RefreshError> {
    let text = std::fs::read_to_string(auth_path()?).map_err(|_| {
        RefreshError::new("auth_required", "未登录 GPT：未找到 Codex CLI 本机登录信息", true, false)
    })?;
    let auth: CodexAuth = serde_json::from_str(&text).map_err(|_| {
        RefreshError::new("auth_invalid", "Codex CLI 本机登录信息无效", true, false)
    })?;
    if auth.openai_api_key.as_ref().is_some_and(|value| !value.trim().is_empty())
        || auth.auth_mode.as_deref().is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "apikey" | "api_key"))
    {
        return Err(RefreshError::new(
            "subscription_unsupported",
            "API Key 登录不支持个人订阅额度",
            false,
            false,
        ));
    }
    if auth.tokens.as_ref().is_none_or(|value| value.access_token.trim().is_empty()) {
        return Err(RefreshError::new(
            "auth_required",
            "Codex CLI 本机登录信息缺少 OAuth 凭据",
            true,
            false,
        ));
    }
    Ok(auth)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_window_and_optional_credits() {
        let body = json!({
            "plan_type": "plus",
            "rate_limit": {
                "primary_window": {"used_percent": 37.5, "duration_seconds": 18000}
            },
            "credits": {"balance": "12.34"}
        });
        let mut values = parse_rate_limit_container(body.get("rate_limit").unwrap());
        append_plan_and_credits(&mut values, Some("plus"), &body);
        assert_eq!(values[0].primary_value.as_deref(), Some("62.5%"));
        assert!(values.iter().any(|value| value.capability_id == "credits"));
    }

    #[cfg(windows)]
    #[test]
    fn prefers_windows_cmd_shim_over_unix_script() {
        let dir = std::env::temp_dir().join(format!("codex-shim-{}-{}", std::process::id(), epoch_for_test()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join("codex"), "#!/bin/sh\n").expect("unix shim");
        std::fs::write(dir.join("codex.cmd"), "@echo off\n").expect("cmd shim");
        let found = find_codex_in_dir(&dir).expect("should prefer cmd");
        assert_eq!(found.file_name().and_then(|name| name.to_str()), Some("codex.cmd"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn epoch_for_test() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}
