use serde::{Deserialize, Serialize};

/// /health 响应中标识本服务的应用名,SDK 依赖它验明身份
pub const APP_ID: &str = "x-notify-service";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const TITLE_MAX: usize = 200;
pub const BODY_MAX: usize = 2000;

#[derive(Debug, Deserialize)]
pub struct NotifyRequest {
    pub title: String,
    pub body: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub app: &'static str,
    pub version: &'static str,
    pub port: u16,
}

/// 通知实际展示渠道,序列化为 "popup" / "system"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NotifyVia {
    Popup,
    System,
}

impl NotifyVia {
    pub fn as_str(self) -> &'static str {
        match self {
            NotifyVia::Popup => "popup",
            NotifyVia::System => "system",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct NotifyResponse {
    pub ok: bool,
    pub via: NotifyVia,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub ok: bool,
    pub error: String,
}

/// 请求校验/解析错误:携带面向调用方的消息与 HTTP 状态码
#[derive(Debug)]
pub enum NotifyError {
    /// JSON 解析失败(400)
    BadJson(String),
    /// 语义校验失败(422)
    EmptyTitle,
    TitleTooLong,
    BodyTooLong,
}

impl NotifyError {
    pub fn status(&self) -> u16 {
        match self {
            NotifyError::BadJson(_) => 400,
            NotifyError::EmptyTitle | NotifyError::TitleTooLong | NotifyError::BodyTooLong => 422,
        }
    }
}

impl std::fmt::Display for NotifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotifyError::BadJson(e) => write!(f, "JSON 解析失败: {e}"),
            NotifyError::EmptyTitle => write!(f, "title 不能为空"),
            NotifyError::TitleTooLong => write!(f, "title 过长(最多 {TITLE_MAX} 字符)"),
            NotifyError::BodyTooLong => write!(f, "body 过长(最多 {BODY_MAX} 字符)"),
        }
    }
}

impl std::error::Error for NotifyError {}

impl NotifyRequest {
    /// 语义校验
    pub fn validate(&self) -> Result<(), NotifyError> {
        let title = self.title.trim();
        if title.is_empty() {
            return Err(NotifyError::EmptyTitle);
        }
        if title.chars().count() > TITLE_MAX {
            return Err(NotifyError::TitleTooLong);
        }
        if let Some(body) = &self.body
            && body.chars().count() > BODY_MAX {
                return Err(NotifyError::BodyTooLong);
            }
        Ok(())
    }
}
