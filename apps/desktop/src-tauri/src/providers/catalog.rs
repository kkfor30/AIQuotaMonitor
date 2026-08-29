//! 用户可添加的平台注册表。端点由产品维护，表单预填官网和官方 API 地址。
//!
//! 官网与 API 请求地址来自 cc-switch 官方预设（不含推广参数），只用于额度查询。

use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub struct CatalogEntry {
    pub id: &'static str,
    pub display_name: &'static str,
    pub official_url: &'static str,
    pub api_base_url: Option<&'static str>,
    pub api_key_url: Option<&'static str>,
    pub api_endpoint_hint: &'static str,
    pub access_hint: &'static str,
    pub needs_api_key: bool,
    pub needs_web_login: bool,
    pub needs_local_cli: bool,
}

pub const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "deepseek",
        display_name: "DeepSeek",
        official_url: "https://platform.deepseek.com",
        api_base_url: Some("https://api.deepseek.com"),
        api_key_url: Some("https://platform.deepseek.com/api_keys"),
        api_endpoint_hint: "默认预填官方端点，用于查询账户余额。请填写完整 URL，不要以斜杠结尾。",
        access_hint: "API Key 查余额；网页登录查用量与缓存",
        needs_api_key: true,
        needs_web_login: true,
        needs_local_cli: false,
    },
    CatalogEntry {
        id: "openai",
        display_name: "GPT / Codex",
        official_url: "https://chatgpt.com/codex",
        api_base_url: None,
        api_key_url: None,
        api_endpoint_hint: "",
        access_hint: "检测本机 Codex 登录；可另加 ChatGPT 账号",
        needs_api_key: false,
        needs_web_login: false,
        needs_local_cli: true,
    },
    CatalogEntry {
        id: "kimi",
        display_name: "Kimi",
        official_url: "https://platform.moonshot.cn",
        api_base_url: Some("https://api.kimi.com/coding"),
        api_key_url: Some("https://platform.moonshot.cn/console/api-keys"),
        api_endpoint_hint: "默认预填官方端点，用于查询 Coding Plan。请填写完整 URL，不要以斜杠结尾。",
        access_hint: "API Key 查 Coding Plan 与个人余额",
        needs_api_key: true,
        needs_web_login: false,
        needs_local_cli: false,
    },
    CatalogEntry {
        id: "glm",
        display_name: "GLM 国内",
        official_url: "https://open.bigmodel.cn",
        api_base_url: Some("https://open.bigmodel.cn"),
        api_key_url: Some("https://open.bigmodel.cn/usercenter/apikeys"),
        api_endpoint_hint: "默认预填官方端点，用于查询 Coding Plan。请填写完整 URL，不要以斜杠结尾。",
        access_hint: "API Key 查 Coding Plan；网页登录查个人余额",
        needs_api_key: true,
        needs_web_login: true,
        needs_local_cli: false,
    },
    CatalogEntry {
        id: "glm_intl",
        display_name: "GLM 国际",
        official_url: "https://z.ai",
        api_base_url: Some("https://api.z.ai"),
        api_key_url: Some("https://z.ai"),
        api_endpoint_hint: "默认预填官方端点，用于查询 Coding Plan。请填写完整 URL，不要以斜杠结尾。",
        access_hint: "API Key 查 Coding Plan",
        needs_api_key: true,
        needs_web_login: false,
        needs_local_cli: false,
    },
    CatalogEntry {
        id: "minimax",
        display_name: "MiniMax 国内",
        official_url: "https://platform.minimaxi.com",
        api_base_url: Some("https://api.minimaxi.com"),
        api_key_url: Some("https://platform.minimaxi.com"),
        api_endpoint_hint: "默认预填官方端点，用于查询 Token Plan。请填写完整 URL，不要以斜杠结尾。",
        access_hint: "API Key 查 Token Plan",
        needs_api_key: true,
        needs_web_login: false,
        needs_local_cli: false,
    },
    CatalogEntry {
        id: "minimax_intl",
        display_name: "MiniMax 国际",
        official_url: "https://www.minimax.io",
        api_base_url: Some("https://api.minimax.io"),
        api_key_url: Some("https://www.minimax.io"),
        api_endpoint_hint: "默认预填官方端点，用于查询 Token Plan。请填写完整 URL，不要以斜杠结尾。",
        access_hint: "API Key 查 Token Plan",
        needs_api_key: true,
        needs_web_login: false,
        needs_local_cli: false,
    },
    CatalogEntry {
        id: "claude_code",
        display_name: "Claude Code",
        official_url: "https://claude.ai",
        api_base_url: None,
        api_key_url: None,
        api_endpoint_hint: "",
        access_hint: "检测本机 Claude 登录",
        needs_api_key: false,
        needs_web_login: false,
        needs_local_cli: true,
    },
    CatalogEntry {
        id: "mimo",
        display_name: "MiMo",
        official_url: "https://platform.xiaomimimo.com",
        api_base_url: None,
        api_key_url: None,
        api_endpoint_hint: "",
        access_hint: "网页登录查余额",
        needs_api_key: false,
        needs_web_login: true,
        needs_local_cli: false,
    },
    CatalogEntry {
        id: "siliconflow",
        display_name: "硅基流动国内",
        official_url: "https://siliconflow.cn",
        api_base_url: Some("https://api.siliconflow.cn"),
        api_key_url: Some("https://cloud.siliconflow.cn/account/ak"),
        api_endpoint_hint: "默认预填官方端点，用于查询账户余额。请填写完整 URL，不要以斜杠结尾。",
        access_hint: "API Key 查账户余额（CNY）",
        needs_api_key: true,
        needs_web_login: false,
        needs_local_cli: false,
    },
    CatalogEntry {
        id: "siliconflow_intl",
        display_name: "硅基流动国际",
        official_url: "https://siliconflow.com",
        api_base_url: Some("https://api.siliconflow.com"),
        api_key_url: Some("https://cloud.siliconflow.com/account/ak"),
        api_endpoint_hint: "默认预填官方端点，用于查询账户余额。请填写完整 URL，不要以斜杠结尾。",
        access_hint: "API Key 查账户余额（USD）；国内/国际 Key 不通用",
        needs_api_key: true,
        needs_web_login: false,
        needs_local_cli: false,
    },
    CatalogEntry {
        id: "stepfun",
        display_name: "StepFun",
        official_url: "https://platform.stepfun.com",
        api_base_url: Some("https://api.stepfun.com"),
        api_key_url: Some("https://platform.stepfun.com/interface-key"),
        api_endpoint_hint: "默认预填官方端点，用于查询账户余额。请填写完整 URL，不要以斜杠结尾。",
        access_hint: "API Key 查账户余额（CNY）",
        needs_api_key: true,
        needs_web_login: false,
        needs_local_cli: false,
    },
    CatalogEntry {
        id: "openrouter",
        display_name: "OpenRouter",
        official_url: "https://openrouter.ai",
        api_base_url: Some("https://openrouter.ai"),
        api_key_url: Some("https://openrouter.ai/keys"),
        api_endpoint_hint: "默认预填官方端点，用于查询 Credits 余额。请填写完整 URL，不要以斜杠结尾。",
        access_hint: "API Key 查 Credits 余额（USD）",
        needs_api_key: true,
        needs_web_login: false,
        needs_local_cli: false,
    },
    CatalogEntry {
        id: "novita",
        display_name: "Novita",
        official_url: "https://novita.ai",
        api_base_url: Some("https://api.novita.ai"),
        api_key_url: Some("https://novita.ai/settings/key-management"),
        api_endpoint_hint: "默认预填官方端点，用于查询账户余额。请填写完整 URL，不要以斜杠结尾。",
        access_hint: "API Key 查账户余额（USD）",
        needs_api_key: true,
        needs_web_login: false,
        needs_local_cli: false,
    },
];

pub fn entry(id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|item| item.id == id)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformCatalogItem {
    pub id: String,
    pub display_name: String,
    pub official_url: String,
    pub api_base_url: Option<String>,
    pub api_key_url: Option<String>,
    pub access_hint: String,
    pub needs_api_key: bool,
    pub needs_web_login: bool,
    pub needs_local_cli: bool,
    pub added: bool,
    pub supports_multiple_accounts: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformSetupViewModel {
    pub platform_id: String,
    pub display_name: String,
    pub notes: String,
    pub official_url: String,
    pub api_key_url: Option<String>,
    pub api_base_url: String,
    pub official_api_base_url: String,
    pub api_endpoint_hint: String,
    pub api_key_source_id: Option<String>,
    pub api_key_configured: bool,
    pub local_cli_source_id: Option<String>,
    pub needs_api_key: bool,
    pub needs_web_login: bool,
    pub needs_local_cli: bool,
}

impl From<&CatalogEntry> for PlatformCatalogItem {
    fn from(entry: &CatalogEntry) -> Self {
        Self {
            id: entry.id.into(),
            display_name: entry.display_name.into(),
            official_url: entry.official_url.into(),
            api_base_url: entry.api_base_url.map(str::to_string),
            api_key_url: entry.api_key_url.map(str::to_string),
            access_hint: entry.access_hint.into(),
            needs_api_key: entry.needs_api_key,
            needs_web_login: entry.needs_web_login,
            needs_local_cli: entry.needs_local_cli,
            added: false,
            supports_multiple_accounts: false,
        }
    }
}

/// 额度查询只接受完整 http(s) URL，去掉末尾斜杠后交给各 Source adapter。
pub fn normalize_api_base_url(value: &str) -> Result<String, String> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("请填写 API 请求地址".into());
    }
    let rest = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"));
    match rest {
        Some(host_and_path) if !host_and_path.is_empty() => Ok(trimmed.to_string()),
        Some(_) => Err("API 请求地址不完整".into()),
        None => Err("API 请求地址必须是 http(s) 完整 URL".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_api_base_url;

    #[test]
    fn keeps_official_https_url_without_trailing_slash() {
        assert_eq!(
            normalize_api_base_url(" https://api.deepseek.com/ ").unwrap(),
            "https://api.deepseek.com"
        );
    }

    #[test]
    fn rejects_host_without_scheme() {
        assert!(normalize_api_base_url("api.deepseek.com").is_err());
    }
}
