//! K 线定时拉取数据源
//!
//! 工作原理：
//! 1. 启动时拉取最近 N 根历史 K 线，用于策略指标预热
//! 2. 此后每隔 `interval` 时间检查是否有新 K 线收盘
//! 3. 有新 K 线则推送 `FeedEvent::Candle`，没有则继续等待
//!
//! 相比 Tick 驱动，K 线驱动更稳定：
//! - 每根 K 线只触发一次信号
//! - 不受短暂价格噪音影响
//! - 适合 MA / RSI 等中低频策略

use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use chrono::{DateTime, Utc};
use tracing::{info, warn, debug};
use hq_core::traits::Exchange;
use hq_core::types::Candle;
use crate::stream::{DataFeed, FeedEvent};
use crate::error::Result;

/// K 线间隔对应的秒数
fn interval_to_secs(interval: &str) -> u64 {
    match interval {
        "1m"  => 60,
        "3m"  => 180,
        "5m"  => 300,
        "15m" => 900,
        "30m" => 1800,
        "1h"  => 3600,
        "2h"  => 7200,
        "4h"  => 14400,
        "6h"  => 21600,
        "8h"  => 28800,
        "12h" => 43200,
        "1d"  => 86400,
        _     => 60,
    }
}

/// 实时 K 线数据源
///
/// # 示例
/// ```rust,no_run
/// use std::sync::Arc;
/// use hq_datafeed::sources::CandleFeed;
/// use hq_datafeed::stream::{DataFeed, FeedEvent};
/// use hq_exchange::BinanceClient;
///
/// #[tokio::main]
/// async fn main() {
///     let exchange = Arc::new(BinanceClient::testnet("key", "secret"));
///     let mut feed = CandleFeed::new(exchange, "ETHUSDT", "1m", 50).await.unwrap();
///     while let Some(event) = feed.next().await {
///         if let FeedEvent::Candle { candle, .. } = event {
///             println!("新K线 close={}", candle.close);
///         }
///     }
/// }
/// ```
pub struct CandleFeed {
    exchange:         Arc<dyn Exchange>,
    symbol:           String,
    interval:         String,
    /// 上一根已推送的 K 线时间戳，防止重复推送
    last_candle_time: Option<DateTime<Utc>>,
    /// 缓冲：预热历史 K 线待推送
    warmup_buffer:    Vec<Candle>,
    /// 每个 K 线周期检查一次，单位秒
    poll_secs:        u64,
    /// 是否已完成预热历史推送
    warmup_done:      bool,
}

impl CandleFeed {
    /// 创建并预加载历史 K 线
    ///
    /// - `warmup_bars`: 预热 K 线数量（用于指标计算，建议 ≥ 最长周期 * 2）
    pub async fn new(
        exchange:    Arc<dyn Exchange>,
        symbol:      impl Into<String>,
        interval:    impl Into<String>,
        warmup_bars: u32,
    ) -> Result<Self> {
        let symbol   = symbol.into();
        let interval = interval.into();
        let poll_secs = interval_to_secs(&interval);

        info!("[CandleFeed] 初始化 symbol={} interval={} 预热{}根", symbol, interval, warmup_bars);

        // 拉取历史 K 线做预热（+1 是因为最后一根可能还未收盘）
        let history = exchange.get_candles(&symbol, &interval, warmup_bars + 1).await?;

        // 排除最后一根（可能未收盘），只保留已收盘的
        let closed: Vec<Candle> = if history.len() > 1 {
            history[..history.len() - 1].to_vec()
        } else {
            history
        };

        let last_time = closed.last().map(|c| c.open_time);

        info!("[CandleFeed] 预热完成，加载 {} 根历史K线", closed.len());

        Ok(Self {
            exchange,
            symbol,
            interval,
            last_candle_time: last_time,
            warmup_buffer:    closed,
            poll_secs,
            warmup_done:      false,
        })
    }

    /// 不做预热，从现在开始拉实时 K 线
    pub fn new_no_warmup(
        exchange: Arc<dyn Exchange>,
        symbol:   impl Into<String>,
        interval: impl Into<String>,
    ) -> Self {
        let interval = interval.into();
        let poll_secs = interval_to_secs(&interval);
        Self {
            exchange,
            symbol: symbol.into(),
            interval,
            last_candle_time: None,
            warmup_buffer:    vec![],
            poll_secs,
            warmup_done:      true,
        }
    }

    /// 拉取最新已收盘的 K 线，若比上次新则返回
    async fn fetch_latest_closed(&self) -> Option<Candle> {
        match self.exchange.get_candles(&self.symbol, &self.interval, 2).await {
            Ok(candles) if candles.len() >= 2 => {
                // candles[-2] 是已收盘的最新一根，candles[-1] 是当前未收盘
                let closed = &candles[candles.len() - 2];
                // 检查是否比上次推送的新
                let is_new = self.last_candle_time
                    .map(|t| closed.open_time > t)
                    .unwrap_or(true);
                if is_new {
                    debug!("[CandleFeed] 新K线 {} close={}", closed.open_time, closed.close);
                    Some(closed.clone())
                } else {
                    None
                }
            }
            Ok(_) => None,
            Err(e) => {
                warn!("[CandleFeed] 拉取K线失败: {}", e);
                None
            }
        }
    }
}

#[async_trait]
impl DataFeed for CandleFeed {
    fn name(&self) -> &str { "CandleFeed" }

    async fn next(&mut self) -> Option<FeedEvent> {
        // 阶段一：先把预热历史 K 线逐根推送出去
        if !self.warmup_done {
            if !self.warmup_buffer.is_empty() {
                let candle = self.warmup_buffer.remove(0);
                return Some(FeedEvent::Candle {
                    symbol:   self.symbol.clone(),
                    interval: self.interval.clone(),
                    candle,
                });
            }
            self.warmup_done = true;
            info!("[CandleFeed] 预热K线推送完毕，切换为实时模式");
        }

        // 阶段二：实时轮询，等待新 K 线收盘
        loop {
            // 等待一个检查周期（K 线间隔的一半，提高响应速度）
            let check_interval = (self.poll_secs / 2).max(10);
            tokio::time::sleep(Duration::from_secs(check_interval)).await;

            if let Some(candle) = self.fetch_latest_closed().await {
                self.last_candle_time = Some(candle.open_time);
                return Some(FeedEvent::Candle {
                    symbol:   self.symbol.clone(),
                    interval: self.interval.clone(),
                    candle,
                });
            }
        }
    }
}
