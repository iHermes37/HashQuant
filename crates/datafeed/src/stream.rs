//! 统一数据流抽象
//!
//! 所有数据来源（交易所实时、数据库回放、CSV）都实现 `DataFeed` trait。
//! 策略层只依赖这个 trait，不关心底层数据从哪来。
//!
//! 使用示例：
//! ```rust,no_run
//! use hq_datafeed::stream::{DataFeed, FeedEvent};
//!
//! async fn run(mut feed: impl DataFeed) {
//!     while let Some(event) = feed.next().await {
//!         match event {
//!             FeedEvent::Tick(ticker) => { /* 处理实时价格 */ }
//!             FeedEvent::Candle { candle, .. } => { /* 处理 K 线 */ }
//!             FeedEvent::End => break,
//!         }
//!     }
//! }
//! ```

use async_trait::async_trait;
use hq_core::types::{Ticker, Candle, OrderBook};
use crate::error::Result;

/// 数据流事件
#[derive(Debug, Clone)]
pub enum FeedEvent {
    /// 最新 Tick（价格快照）
    Tick(Ticker),
    /// 一根完整 K 线（收盘后推送）
    Candle { symbol: String, interval: String, candle: Candle },
    /// 盘口深度更新
    Book(OrderBook),
    /// 数据流结束（回测/CSV 用）
    End,
}

/// 所有数据源必须实现的统一接口
#[async_trait]
pub trait DataFeed: Send {
    /// 拉取下一个事件，返回 None 表示流结束
    async fn next(&mut self) -> Option<FeedEvent>;

    /// 数据源名称，用于日志
    fn name(&self) -> &str;
}

/// 订阅配置
#[derive(Debug, Clone)]
pub struct Subscription {
    pub symbol:   String,
    /// K 线周期，如 "1m" "5m" "1h"，None 表示只订阅 Tick
    pub interval: Option<String>,
    /// 是否订阅盘口
    pub orderbook: bool,
}

impl Subscription {
    pub fn ticker(symbol: impl Into<String>) -> Self {
        Self { symbol: symbol.into(), interval: None, orderbook: false }
    }
    pub fn candle(symbol: impl Into<String>, interval: impl Into<String>) -> Self {
        Self { symbol: symbol.into(), interval: Some(interval.into()), orderbook: false }
    }
    pub fn full(symbol: impl Into<String>, interval: impl Into<String>) -> Self {
        Self { symbol: symbol.into(), interval: Some(interval.into()), orderbook: true }
    }
}
