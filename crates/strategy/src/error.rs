use thiserror::Error;

#[derive(Error, Debug)]
pub enum StrategyError {
    #[error("交易所错误: {0}")]
    Exchange(#[from] hq_core::error::CoreError),

    #[error("数据不足: 需要 {need} 根K线，当前只有 {have}")]
    NotEnoughData { need: usize, have: usize },

    #[error("无效参数: {0}")]
    InvalidParam(String),

    #[error("策略内部错误: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, StrategyError>;
