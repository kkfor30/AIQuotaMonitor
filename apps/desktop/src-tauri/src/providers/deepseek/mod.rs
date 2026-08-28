//! DeepSeek 双 Source：官方余额与网页登录用量。

pub mod balance;
pub mod web_usage;

pub const BALANCE_SOURCE_ID: &str = "deepseek-balance-api";
pub const WEB_SOURCE_ID: &str = "deepseek-web-session";
