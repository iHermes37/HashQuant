//! 策略引擎
//!
//! 职责：
//! 1. 从 `DataFeed` 接收 `FeedEvent`
//! 2. 分发给 `Strategy::on_candle` / `on_tick`
//! 3. 把返回的 `Signal` 转成 `PlaceOrderRequest` 发给 `Exchange`
//! 4. 记录所有信号和订单（用于分析）

use std::sync::Arc;
use tracing::{info, warn};
use hq_core::traits::Exchange;
use hq_core::types::{PlaceOrderRequest, OrderType, Order};
use hq_datafeed::stream::{DataFeed, FeedEvent};
use crate::strategy::{Strategy, Signal, SignalKind};
use crate::error::Result;

/// 引擎运行统计
#[derive(Debug, Default, Clone)]
pub struct EngineStats {
    pub candles_processed: u64,
    pub ticks_processed:   u64,
    pub signals_generated: u64,
    pub orders_placed:     u64,
    pub orders_failed:     u64,
}

/// 策略引擎配置
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// 每次最多用账户余额的多少比例下单（风控）
    pub max_position_pct: f64,
    /// 是否在信号触发时打印日志
    pub verbose: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self { max_position_pct: 0.95, verbose: true }
    }
}

pub struct Engine {
    exchange: Arc<dyn Exchange>,
    config:   EngineConfig,
    stats:    EngineStats,
}

impl Engine {
    pub fn new(exchange: Arc<dyn Exchange>) -> Self {
        Self { exchange, config: EngineConfig::default(), stats: EngineStats::default() }
    }

    pub fn with_config(mut self, config: EngineConfig) -> Self {
        self.config = config;
        self
    }

    pub fn stats(&self) -> &EngineStats { &self.stats }

    /// 运行引擎直到 feed 结束
    pub async fn run(
        &mut self,
        feed:     &mut dyn DataFeed,
        strategy: &mut dyn Strategy,
    ) -> Result<()> {
        info!("[Engine] 启动策略: {}", strategy.name());

        loop {
            let event = match feed.next().await {
                Some(e) => e,
                None    => break,
            };

            match event {
                FeedEvent::Candle { candle, .. } => {
                    self.stats.candles_processed += 1;
                    let signals = strategy.on_candle(&candle).await?;
                    self.execute_signals(signals).await;
                }
                FeedEvent::Tick(ticker) => {
                    self.stats.ticks_processed += 1;
                    let signals = strategy.on_tick(&ticker).await?;
                    self.execute_signals(signals).await;
                }
                FeedEvent::End => {
                    info!("[Engine] 数据流结束，策略: {}", strategy.name());
                    break;
                }
                FeedEvent::Book(_) => {} // 当前策略不处理盘口
            }
        }

        info!(
            "[Engine] 完成 | K线={} Tick={} 信号={} 下单成功={} 失败={}",
            self.stats.candles_processed, self.stats.ticks_processed,
            self.stats.signals_generated, self.stats.orders_placed, self.stats.orders_failed
        );
        Ok(())
    }

    /// 将信号转换为订单并提交
    async fn execute_signals(&mut self, signals: Vec<Signal>) {
        for sig in signals {
            self.stats.signals_generated += 1;

            if self.config.verbose {
                info!("[Signal] {:?} {} | {}", sig.side, sig.symbol, sig.reason);
            }

            match self.signal_to_order(&sig).await {
                Ok(Some(req)) => {
                    match self.exchange.place_order(req).await {
                        Ok(order) => {
                            self.stats.orders_placed += 1;
                            if self.config.verbose {
                                info!("[Order] {} {:?} status={:?}", order.order_id, order.side, order.status);
                            }
                        }
                        Err(e) => {
                            self.stats.orders_failed += 1;
                            warn!("[Order] 下单失败: {}", e);
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => warn!("[Signal] 信号转换失败: {}", e),
            }
        }
    }

    /// 信号 → PlaceOrderRequest（查询账户余额，计算数量）
    async fn signal_to_order(&self, sig: &Signal) -> Result<Option<PlaceOrderRequest>> {
        let account = self.exchange.get_account().await?;

        // 从 symbol 推断 quote 资产（BTC-USDT → USDT，BTCUSDT → USDT）
        let quote = if sig.symbol.contains('-') {
            sig.symbol.split('-').nth(1).unwrap_or("USDT")
        } else {
            &sig.symbol[sig.symbol.len().saturating_sub(4)..]
        };
        let base = if sig.symbol.contains('-') {
            sig.symbol.split('-').next().unwrap_or(&sig.symbol)
        } else {
            &sig.symbol[..sig.symbol.len().saturating_sub(4)]
        };

        let size_pct = sig.size_pct.unwrap_or(1.0) * self.config.max_position_pct;

        let qty = match sig.side {
            hq_core::types::OrderSide::Buy => {
                // 用可用 quote 资产计算买入数量
                let free_quote = account.balances.iter()
                    .find(|b| b.asset == quote)
                    .map(|b| b.free)
                    .unwrap_or(0.0);
                if free_quote <= 0.0 { return Ok(None); }

                match sig.price {
                    Some(p) if p > 0.0 => free_quote * size_pct / p,
                    _ => {
                        // 市价：先查 ticker 估算
                        let ticker = self.exchange.get_ticker(&sig.symbol).await?;
                        free_quote * size_pct / ticker.ask
                    }
                }
            }
            hq_core::types::OrderSide::Sell => {
                // 用可用 base 资产计算卖出数量
                let free_base = account.balances.iter()
                    .find(|b| b.asset == base)
                    .map(|b| b.free)
                    .unwrap_or(0.0);
                if free_base <= 0.0 { return Ok(None); }
                free_base * size_pct
            }
        };

        if qty <= 0.0 { return Ok(None); }

        let req = match sig.price {
            Some(p) => PlaceOrderRequest::limit(&sig.symbol, sig.side.clone(), qty, p),
            None    => PlaceOrderRequest::market(&sig.symbol, sig.side.clone(), qty),
        };

        Ok(Some(req))
    }
}
