//! 实时行情数据源
//!
//! 通过轮询 `Exchange::get_ticker` 获取实时价格。
//! 支持多 symbol 同时订阅，每个 symbol 独立计时。
//!
//! 生产环境建议改为 WebSocket，此处用轮询简化实现，
//! 方便测试时替换为 MockExchange。

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval, MissedTickBehavior};
use tracing::{debug, warn};
use hq_core::traits::Exchange;
use hq_core::types::Ticker;
use crate::stream::{DataFeed, FeedEvent, Subscription};
use crate::error::Result;

/// 基于轮询的实时行情源
pub struct ExchangeFeed {
    exchange:      Arc<dyn Exchange>,
    subscriptions: Vec<Subscription>,
    /// 轮询间隔（毫秒），默认 1000ms
    poll_ms:       u64,
    /// 内部事件缓冲
    buffer:        Vec<FeedEvent>,
    /// 每个 symbol 上次的 ticker，用于去重（价格没变不推送）
    last_tickers:  HashMap<String, f64>,
}

impl ExchangeFeed {
    pub fn new(exchange: Arc<dyn Exchange>, poll_ms: u64) -> Self {
        Self {
            exchange,
            subscriptions: vec![],
            poll_ms,
            buffer: vec![],
            last_tickers: HashMap::new(),
        }
    }

    /// 添加订阅
    pub fn subscribe(mut self, sub: Subscription) -> Self {
        self.subscriptions.push(sub);
        self
    }

    /// 批量添加订阅
    pub fn subscribe_many(mut self, subs: Vec<Subscription>) -> Self {
        self.subscriptions.extend(subs);
        self
    }

    /// 拉取所有订阅的最新 ticker，填入 buffer
    async fn poll(&mut self) {
        for sub in &self.subscriptions {
            match self.exchange.get_ticker(&sub.symbol).await {
                Ok(ticker) => {
                    // 只有价格变化时才推送，避免无意义的重复事件
                    let changed = self.last_tickers
                        .get(&sub.symbol)
                        .map(|&last| (last - ticker.last).abs() > f64::EPSILON)
                        .unwrap_or(true);

                    if changed {
                        self.last_tickers.insert(sub.symbol.clone(), ticker.last);
                        debug!("[ExchangeFeed] tick {} last={}", ticker.symbol, ticker.last);
                        self.buffer.push(FeedEvent::Tick(ticker));
                    }
                }
                Err(e) => {
                    warn!("[ExchangeFeed] 拉取 {} 失败: {}", sub.symbol, e);
                }
            }
        }
    }
}

#[async_trait]
impl DataFeed for ExchangeFeed {
    fn name(&self) -> &str { "ExchangeFeed" }

    async fn next(&mut self) -> Option<FeedEvent> {
        loop {
            // 先消费缓冲
            if !self.buffer.is_empty() {
                return Some(self.buffer.remove(0));
            }
            // 缓冲空了，等待下一个轮询周期
            let mut ticker = interval(Duration::from_millis(self.poll_ms));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
            ticker.tick().await; // 首次立即触发
            ticker.tick().await; // 等待一个间隔
            self.poll().await;
        }
    }
}

/// 历史 K 线批量拉取（非流式，用于策略初始化）
pub async fn fetch_candles(
    exchange: &dyn Exchange,
    symbol:   &str,
    interval: &str,
    limit:    u32,
) -> Result<Vec<hq_core::types::Candle>> {
    let candles = exchange.get_candles(symbol, interval, limit).await?;
    Ok(candles)
}

/// 快速构建：从 MockExchange 创建测试用实时源
#[cfg(test)]
pub fn mock_feed(exchange: Arc<dyn Exchange>, symbols: Vec<&str>) -> ExchangeFeed {
    let subs = symbols.into_iter()
        .map(|s| Subscription::ticker(s))
        .collect();
    ExchangeFeed::new(exchange, 100).subscribe_many(subs)
}
