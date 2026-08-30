//! Source adapter 与持久化层之间的内部刷新契约。

#[derive(Debug, Clone)]
pub struct StoredTrendPoint {
    pub label: String,
    /// Decimal/整数文本；只在生成图形 ViewModel 时转换为显示用 f64。
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct CapabilityData {
    pub capability_id: String,
    pub display_name: String,
    pub value_kind: String,
    pub primary_value: Option<String>,
    pub secondary_value: Option<String>,
    /// 仅用于进度条显示，不参与金额或额度汇总。
    pub progress: Option<f64>,
    pub trend: Vec<StoredTrendPoint>,
    /// 结构化额度窗口时长（秒）。仅 Codex 额度窗口填写；窗口观察器只读这里，
    /// 禁止反解析 secondary_value 中文文本。
    pub window_seconds: Option<i64>,
    /// 结构化窗口重置时间（epoch 毫秒）；无重置时间信息时为 None。
    pub reset_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct RefreshError {
    pub code: String,
    pub message: String,
    pub auth_required: bool,
}

impl RefreshError {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        auth_required: bool,
        _retryable: bool,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            auth_required,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceRefreshOutput {
    pub capabilities: Vec<CapabilityData>,
    /// 部分成功时 capabilities 非空且 error 为 Some。
    pub error: Option<RefreshError>,
}

impl SourceRefreshOutput {
    pub fn success(capabilities: Vec<CapabilityData>) -> Self {
        Self {
            capabilities,
            error: None,
        }
    }

    pub fn failure(error: RefreshError) -> Self {
        Self {
            capabilities: vec![],
            error: Some(error),
        }
    }
}
