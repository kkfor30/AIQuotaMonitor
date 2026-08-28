//! Source 级刷新协调器：平台去重、并行 Source、generation 防覆盖与部分成功。

use crate::domain::refresh::{RefreshError, SourceRefreshOutput};
use crate::providers::{coding_plan, codex, deepseek, glm, kimi, mimo};
use crate::storage::database::Database;
use crate::storage::repository::SourceRecord;
use crate::storage::vault;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::task::JoinSet;

pub struct RefreshCoordinator {
    client: Client,
    platform_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl RefreshCoordinator {
    pub fn new() -> Result<Self, String> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|err| format!("初始化平台网络客户端失败: {err}"))?;
        Ok(Self {
            client,
            platform_locks: Mutex::new(HashMap::new()),
        })
    }

    pub async fn refresh_platform(&self, database: &Database, provider_id: &str) -> Result<(), String> {
        let lock = {
            let mut locks = self.platform_locks.lock().await;
            locks
                .entry(provider_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let Ok(_guard) = lock.try_lock() else {
            return Ok(());
        };

        let configured = database
            .list_sources(provider_id)?
            .into_iter()
            .filter_map(|source| self.source_secret(database, &source).map(|secret| (source, secret)))
            .collect::<Vec<_>>();
        if configured.is_empty() {
            return Ok(());
        }
        let ids = configured.iter().map(|(source, _)| source.id.clone()).collect::<Vec<_>>();
        let run_id = database.begin_refresh_run(provider_id, &ids)?;
        let mut tasks = JoinSet::new();
        for (source, secret) in configured {
            let generation = database.begin_source_refresh(&source.id)?;
            let client = self.client.clone();
            let api_base_url = database
                .user_platform(&source.platform_id)?
                .and_then(|platform| platform.api_base_url);
            let extra_home = extra_codex_home(database, &source.id);
            tasks.spawn(async move {
                let output = fetch_source(
                    &client,
                    &source,
                    secret.as_deref(),
                    api_base_url.as_deref(),
                    extra_home.as_deref(),
                )
                .await;
                (source, generation, output)
            });
        }
        while let Some(result) = tasks.join_next().await {
            let (source, generation, output) =
                result.map_err(|err| format!("Source 刷新任务异常结束: {err}"))?;
            let current = database.source(&source.id)?;
            let _ = database.complete_source_refresh(&run_id, &current, generation, &output)?;
        }
        database.finish_refresh_run(&run_id)
    }

    pub async fn validate_secret(
        &self,
        source: &SourceRecord,
        secret: &str,
    ) -> Result<SourceRefreshOutput, String> {
        self.validate_secret_at(source, secret, None).await
    }

    pub async fn validate_secret_at(
        &self,
        source: &SourceRecord,
        secret: &str,
        api_base_url: Option<&str>,
    ) -> Result<SourceRefreshOutput, String> {
        if secret.trim().is_empty() {
            return Err("凭据不能为空".into());
        }
        let output = fetch_source(&self.client, source, Some(secret), api_base_url, None).await;
        if let Some(error) = &output.error {
            if error.auth_required || output.capabilities.is_empty() {
                return Err(error.message.clone());
            }
        }
        Ok(output)
    }

    pub fn persist_validated(
        &self,
        database: &Database,
        source: &SourceRecord,
        output: &SourceRefreshOutput,
    ) -> Result<(), String> {
        let run_id = database.begin_refresh_run(&source.platform_id, std::slice::from_ref(&source.id))?;
        let generation = database.begin_source_refresh(&source.id)?;
        let current = database.source(&source.id)?;
        let _ = database.complete_source_refresh(&run_id, &current, generation, output)?;
        database.finish_refresh_run(&run_id)
    }

    fn source_secret(&self, database: &Database, source: &SourceRecord) -> Option<Option<String>> {
        if source.id == codex::SOURCE_ID {
            return codex::local_auth_available().then_some(None);
        }
        if let Some(home) = extra_codex_home(database, &source.id) {
            return codex::auth_available_at(Some(&home)).then_some(None);
        }
        source
            .secret_ref
            .as_deref()
            .and_then(|reference| vault::get(reference).ok().flatten())
            .map(Some)
    }
}

fn extra_codex_home(database: &Database, source_id: &str) -> Option<std::path::PathBuf> {
    let data_dir = database.path().parent()?;
    codex::is_extra_source(source_id).then(|| codex::extra_source_home(data_dir, source_id))
}

async fn fetch_source(
    client: &Client,
    source: &SourceRecord,
    secret: Option<&str>,
    api_base_url: Option<&str>,
    extra_home: Option<&std::path::Path>,
) -> SourceRefreshOutput {
    match source.id.as_str() {
        deepseek::BALANCE_SOURCE_ID => match secret {
            Some(secret) => deepseek::balance::fetch(client, secret, api_base_url).await,
            None => missing_secret("DeepSeek API Key 未配置"),
        },
        deepseek::WEB_SOURCE_ID => match secret {
            Some(secret) => deepseek::web_usage::fetch_current_month(client, secret).await,
            None => missing_secret("DeepSeek 网页会话未配置"),
        },
        id if id == codex::SOURCE_ID => codex::fetch(client).await,
        id if codex::is_extra_source(id) => codex::fetch_at(client, extra_home).await,
        id if coding_plan::is_coding_plan_source(id) => match secret {
            Some(secret) => coding_plan::fetch(client, id, secret, api_base_url).await,
            None => missing_secret("API Key 未配置"),
        },
        kimi::BALANCE_SOURCE_ID => match secret {
            Some(secret) => kimi::fetch(client, secret, api_base_url).await,
            None => missing_secret("Kimi 开放平台 API Key 未配置"),
        },
        glm::WEB_BALANCE_SOURCE_ID => match secret {
            Some(secret) => glm::fetch(client, secret).await,
            None => missing_secret("GLM 网页会话未配置"),
        },
        mimo::SOURCE_ID => match secret {
            Some(secret) => mimo::fetch(client, secret).await,
            None => missing_secret("MiMo 网页会话未配置"),
        },
        _ => SourceRefreshOutput::failure(RefreshError::new(
            "unsupported_source",
            "当前版本尚未实现此数据来源",
            false,
            false,
        )),
    }
}

fn missing_secret(message: &str) -> SourceRefreshOutput {
    SourceRefreshOutput::failure(RefreshError::new("auth_required", message, true, false))
}
