use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("HTTP 请求失败: {0}")]
    Http(String),

    #[error("JSON 解析错误: {0}")]
    Json(String),

    #[error("[{exchange}] API 错误 code={code}: {message}")]
    Api { exchange: String, code: i64, message: String },

    #[error("签名/认证错误: {0}")]
    Auth(String),

    #[error("参数错误: {0}")]
    InvalidParam(String),

    #[error("不支持的操作: {0}")]
    Unsupported(String),

    #[error("限速，请稍后重试")]
    RateLimit,

    #[error("数据解析错误: {0}")]
    Parse(String),

    #[error("订单不存在: {0}")]
    OrderNotFound(String),

    #[error("余额不足: 需要 {required}, 可用 {available}")]
    InsufficientBalance { required: String, available: String },
}
