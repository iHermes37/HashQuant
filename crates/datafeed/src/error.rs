use thiserror::Error;

#[derive(Error, Debug)]
pub enum FeedError {
    #[error("交易所错误: {0}")]
    Exchange(#[from] hq_core::error::CoreError),

    #[error("CSV 读取错误: {0}")]
    Csv(#[from] csv::Error),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("数据库错误: {0}")]
    Database(String),

    #[error("数据解析错误: {0}")]
    Parse(String),

    #[error("数据源已关闭")]
    Closed,

    #[error("数据源: {symbol} 无历史数据")]
    NoData { symbol: String },
}

pub type Result<T> = std::result::Result<T, FeedError>;
