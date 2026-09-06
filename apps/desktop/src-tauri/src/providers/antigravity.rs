//! Google Antigravity 本机额度监控 Source。
//!
//! 通过探测本机 Antigravity 客户端运行态（Language Server 本地 HTTPS Connect-RPC 服务），
//! 获取 Gemini Models 与 Claude/GPT models 的 5 小时及周额度窗口、订阅套餐计划与账号标识。
//!
//! 零配置检测机制：
//! - 读取本机日志：`AppData/Roaming/Antigravity/logs/main.log`
//!   倒序提取最新的 `--csrf_token <UUID>` 与 `Local: https://127.0.0.1:<PORT>/`。
//! - 请求端点：
//!   1. `POST https://127.0.0.1:<PORT>/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary`
//!   2. `POST https://127.0.0.1:<PORT>/exa.language_server_pb.LanguageServerService/GetUserStatus`
//! - 请求头：`x-codeium-csrf-token: <csrf_token>`，`Connect-Protocol-Version: 1`。

use super::coding_plan::window_capability;
use crate::domain::refresh::{CapabilityData, RefreshError, SourceRefreshOutput};
use reqwest::Client;
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::Duration;

pub const SOURCE_ID: &str = "antigravity-local";

/// 本机 Antigravity 运行态连接凭据
#[derive(Debug, Clone)]
struct AntigravityEndpoint {
    port: u16,
    csrf_token: String,
}

/// 检查本机 Antigravity 日志与运行态是否就绪
pub fn local_auth_available() -> bool {
    resolve_endpoint().is_some()
}

/// 解析日志定位正在运行的端口和 CSRF Token
fn resolve_endpoint() -> Option<AntigravityEndpoint> {
    let log_path = resolve_log_path()?;
    if !log_path.exists() {
        return None;
    }

    let file = File::open(&log_path).ok()?;
    let reader = BufReader::new(file);

    // main.log 可能包含历史追加记录，倒序检索最新的 token 与 port
    let lines: Vec<String> = reader.lines().map_while(Result::ok).collect();

    let mut found_port: Option<u16> = None;
    let mut found_csrf: Option<String> = None;

    for line in lines.iter().rev() {
        if found_port.is_none() {
            if let Some(port) = extract_port_from_line(line) {
                found_port = Some(port);
            }
        }

        if found_csrf.is_none() {
            if let Some(csrf) = extract_csrf_from_line(line) {
                found_csrf = Some(csrf);
            }
        }

        if found_port.is_some() && found_csrf.is_some() {
            break;
        }
    }

    let (port, csrf_token) = (found_port?, found_csrf?);
    Some(AntigravityEndpoint { port, csrf_token })
}

fn extract_port_from_line(line: &str) -> Option<u16> {
    // 匹配 "Local:       https://127.0.0.1:<PORT>/" 或 "Port changed! ... URL: https://127.0.0.1:<PORT>/"
    let marker = "https://127.0.0.1:";
    let idx = line.find(marker)?;
    let after = &line[idx + marker.len()..];
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u16>().ok()
}

fn extract_csrf_from_line(line: &str) -> Option<String> {
    // 匹配 "--csrf_token <UUID>"
    let marker = "--csrf_token ";
    let idx = line.find(marker)?;
    let after = &line[idx + marker.len()..];
    let token: String = after
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '-')
        .collect();
    if token.len() >= 32 {
        Some(token)
    } else {
        None
    }
}

/// 跨平台解析 Antigravity 日志路径
fn resolve_log_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let p = PathBuf::from(appdata)
                .join("Antigravity")
                .join("logs")
                .join("main.log");
            if p.exists() {
                return Some(p);
            }
        }
        if let Some(userprofile) = std::env::var_os("USERPROFILE") {
            let p = PathBuf::from(userprofile)
                .join("AppData")
                .join("Roaming")
                .join("Antigravity")
                .join("logs")
                .join("main.log");
            return Some(p);
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return Some(
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("Antigravity")
                    .join("logs")
                    .join("main.log"),
            );
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return Some(
                PathBuf::from(home)
                    .join(".config")
                    .join("Antigravity")
                    .join("logs")
                    .join("main.log"),
            );
        }
    }

    None
}

/// 拉取 Antigravity 本机额度数据
pub async fn fetch(_shared_client: &Client) -> SourceRefreshOutput {
    let endpoint = match resolve_endpoint() {
        Some(ep) => ep,
        None => {
            return SourceRefreshOutput::failure(RefreshError::new(
                "cli_not_running",
                "未检测到 Antigravity 运行中 · 请启动桌面客户端",
                false,
                true,
            ));
        }
    };

    // 本地自签名证书，使用独立的跳过证书校验 Client
    let local_client = match Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(4))
        .build()
    {
        Ok(c) => c,
        Err(err) => {
            return SourceRefreshOutput::failure(RefreshError::new(
                "network_error",
                format!("创建本地网络请求客户端失败：{err}"),
                false,
                false,
            ));
        }
    };

    let quota_url = format!(
        "https://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary",
        endpoint.port
    );
    let user_status_url = format!(
        "https://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/GetUserStatus",
        endpoint.port
    );

    // 1. 请求额度汇总
    let quota_resp = match local_client
        .post(&quota_url)
        .header("Content-Type", "application/json")
        .header("Connect-Protocol-Version", "1")
        .header("x-codeium-csrf-token", &endpoint.csrf_token)
        .json(&serde_json::json!({ "forceRefresh": false }))
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(err) => {
            return SourceRefreshOutput::failure(RefreshError::new(
                "cli_not_running",
                format!("无法连接 Antigravity 本地服务：{err}"),
                false,
                true,
            ));
        }
    };

    if !quota_resp.status().is_success() {
        let status = quota_resp.status();
        return SourceRefreshOutput::failure(RefreshError::new(
            if status.as_u16() == 401 || status.as_u16() == 403 {
                "credential_expired"
            } else {
                "http_error"
            },
            format!("Antigravity 额度查询失败（HTTP {}）", status.as_u16()),
            status.as_u16() == 401 || status.as_u16() == 403,
            status.is_server_error(),
        ));
    }

    let quota_body: Value = match quota_resp.json().await {
        Ok(v) => v,
        Err(err) => {
            return SourceRefreshOutput::failure(RefreshError::new(
                "response_shape_changed",
                format!("解析 Antigravity 额度数据失败：{err}"),
                false,
                false,
            ));
        }
    };

    // 2. 请求用户信息与套餐
    let user_status_body: Option<Value> = match local_client
        .post(&user_status_url)
        .header("Content-Type", "application/json")
        .header("Connect-Protocol-Version", "1")
        .header("x-codeium-csrf-token", &endpoint.csrf_token)
        .json(&serde_json::json!({}))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => resp.json().await.ok(),
        _ => None,
    };

    let capabilities = parse_antigravity_data(&quota_body, user_status_body.as_ref());

    if capabilities
        .iter()
        .any(|item| item.capability_id.starts_with("quota_window_"))
    {
        SourceRefreshOutput::success(capabilities)
    } else {
        SourceRefreshOutput::failure(RefreshError::new(
            "missing_quota",
            "已连接 Antigravity，但未返回可识别的模型额度窗口",
            false,
            false,
        ))
    }
}

/// 解析配额与账户信息
fn parse_antigravity_data(
    quota_body: &Value,
    user_status_body: Option<&Value>,
) -> Vec<CapabilityData> {
    let mut capabilities = Vec::new();

    // 解析 RetrieveUserQuotaSummary
    if let Some(groups) = quota_body
        .get("response")
        .and_then(|r| r.get("groups"))
        .and_then(Value::as_array)
    {
        for group in groups {
            let group_name = group
                .get("displayName")
                .and_then(Value::as_str)
                .unwrap_or("");
            let is_gemini = group_name.to_lowercase().contains("gemini");
            let is_3p = group_name.to_lowercase().contains("claude")
                || group_name.to_lowercase().contains("gpt");

            let Some(buckets) = group.get("buckets").and_then(Value::as_array) else {
                continue;
            };

            for bucket in buckets {
                let bucket_id = bucket.get("bucketId").and_then(Value::as_str).unwrap_or("");
                let window_kind = bucket.get("window").and_then(Value::as_str).unwrap_or("");

                // remainingFraction 是 0.0 ~ 1.0 的浮点数
                let remaining_fraction = bucket
                    .get("remainingFraction")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                let remaining_percent = (remaining_fraction * 100.0).clamp(0.0, 100.0);
                let reset_time = bucket.get("resetTime");

                if is_gemini || bucket_id.starts_with("gemini-") {
                    if window_kind == "5h" || bucket_id.ends_with("5h") {
                        capabilities.push(window_capability(
                            "quota_window_5h_gemini",
                            "Gemini 5 小时窗口",
                            remaining_percent,
                            reset_time,
                        ));
                    } else if window_kind == "weekly" || bucket_id.ends_with("weekly") {
                        capabilities.push(window_capability(
                            "quota_window_7d_gemini",
                            "Gemini 周窗口",
                            remaining_percent,
                            reset_time,
                        ));
                    }
                } else if is_3p || bucket_id.starts_with("3p-") {
                    if window_kind == "5h" || bucket_id.ends_with("5h") {
                        capabilities.push(window_capability(
                            "quota_window_5h_3p",
                            "Claude/GPT 5 小时窗口",
                            remaining_percent,
                            reset_time,
                        ));
                    } else if window_kind == "weekly" || bucket_id.ends_with("weekly") {
                        capabilities.push(window_capability(
                            "quota_window_7d_3p",
                            "Claude/GPT 周窗口",
                            remaining_percent,
                            reset_time,
                        ));
                    }
                }
            }
        }
    }

    // 解析 GetUserStatus
    if let Some(user_body) = user_status_body {
        let user_status = user_body.get("userStatus");

        // 套餐等级：如 "Google AI Pro"
        if let Some(plan_name) = user_status
            .and_then(|u| u.get("userTier"))
            .and_then(|t| t.get("name").or_else(|| t.get("description")))
            .and_then(Value::as_str)
        {
            capabilities.push(CapabilityData {
                capability_id: "plan_level".into(),
                display_name: "套餐类型".into(),
                value_kind: "text".into(),
                primary_value: Some(plan_name.into()),
                secondary_value: None,
                progress: None,
                trend: vec![],
                window_seconds: None,
                reset_at: None,
            });
        }

        // 账号名：如 "Edwin Blanda"
        if let Some(name) = user_status
            .and_then(|u| u.get("name"))
            .and_then(Value::as_str)
        {
            if !name.trim().is_empty() {
                capabilities.push(CapabilityData {
                    capability_id: "account_name".into(),
                    display_name: "账号".into(),
                    value_kind: "text".into(),
                    primary_value: Some(name.into()),
                    secondary_value: None,
                    progress: None,
                    trend: vec![],
                    window_seconds: None,
                    reset_at: None,
                });
            }
        }
    }

    capabilities
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_antigravity_local_fetch() {
        if !local_auth_available() {
            println!("Antigravity client not detected, skipping live test");
            return;
        }
        let client = Client::new();
        let output = fetch(&client).await;
        assert!(output.error.is_none(), "fetch error: {:?}", output.error);
        assert!(!output.capabilities.is_empty(), "capabilities empty");
        for cap in &output.capabilities {
            println!("Capability: {} ({}) -> {:?}", cap.display_name, cap.capability_id, cap.primary_value);
        }
        let has_gemini = output.capabilities.iter().any(|c| c.capability_id.contains("gemini"));
        assert!(has_gemini, "should contain gemini capabilities");
    }
}
