//! Source 级刷新协调器：平台去重、并行 Source、generation 防覆盖与部分成功。

use crate::domain::refresh::{RefreshError, SourceRefreshOutput};
use crate::providers::{codex, deepseek};
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
            .connect_timeout(Duration::from_secs(8))
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
            .filter_map(|source| self.source_secret(&source).map(|secret| (source, secret)))
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
            tasks.spawn(async move {
                let output = fetch_source(&client, &source, secret.as_deref()).await;
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
        if secret.trim().is_empty() {
            return Err("凭据不能为空".into());
        }
        let output = fetch_source(&self.client, source, Some(secret)).await;
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

    fn source_secret(&self, source: &SourceRecord) -> Option<Option<String>> {
        if source.id == codex::SOURCE_ID {
            return codex::local_auth_available().then_some(None);
        }
        source
            .secret_ref
            .as_deref()
            .and_then(|reference| vault::get(reference).ok().flatten())
            .map(Some)
    }
}

async fn fetch_source(client: &Client, source: &SourceRecord, secret: Option<&str>) -> SourceRefreshOutput {
    match source.id.as_str() {
        deepseek::BALANCE_SOURCE_ID => match secret {
            Some(secret) => deepseek::balance::fetch(client, secret).await,
            None => missing_secret("DeepSeek API Key 未配置"),
        },
        deepseek::WEB_SOURCE_ID => match secret {
            Some(secret) => deepseek::web_usage::fetch_current_month(client, secret).await,
            None => missing_secret("DeepSeek 网页会话未配置"),
        },
        codex::SOURCE_ID => codex::fetch(client).await,
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
