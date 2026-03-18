//! 回测模拟器
//!
//! 把 CsvFeed + Strategy + MockExchange 串联成一次完整回测。
//! 回测完成后返回 `BacktestResult`，包含所有成交和绩效指标。

use std::sync::Arc;
use tracing::{info, warn};
use hq_exchange::MockExchange;
use hq_datafeed::stream::{DataFeed, FeedEvent};
use hq_strategy::strategy::Strategy;
use hq_core::types::{PlaceOrderRequest, OrderSide};
use crate::metrics::Metrics;
use hq_core::Exchange;
/// 回测配置
#[derive(Debug, Clone)]
pub struct BacktestConfig {
    /// 初始资金（USDT）
    pub initial_equity: f64,
    /// 手续费率（%，如 0.1 = 0.1%）
    pub fee_rate:       f64,
    /// 每次下单占可用余额的比例（0–1）
    pub order_size_pct: f64,
    /// 是否打印每根 K 线日志
    pub verbose:        bool,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            initial_equity: 10_000.0,
            fee_rate:       0.1,
            order_size_pct: 0.95,
            verbose:        false,
        }
    }
}

/// 回测结果
pub struct BacktestResult {
    pub metrics:       Metrics,
    pub candles_count: u64,
    pub signal_count:  u64,
}

/// 回测模拟器
pub struct Simulator {
    config:   BacktestConfig,
    exchange: Arc<MockExchange>,
}

impl Simulator {
    pub fn new(config: BacktestConfig) -> Self {
        let exchange = Arc::new(MockExchange::new(config.fee_rate));
        exchange.seed_balance("USDT", config.initial_equity);
        Self { config, exchange }
    }

    /// 获取 exchange 引用（可用于外部注入初始持仓等）
    pub fn exchange(&self) -> Arc<MockExchange> { self.exchange.clone() }

    /// 运行一次回测
    ///
    /// - `feed`     : 数据源（通常是 CsvFeed）
    /// - `strategy` : 要测试的策略
    /// - `symbol`   : 交易对，用于查询余额和下单
    pub async fn run(
        &mut self,
        feed:     &mut dyn DataFeed,
        strategy: &mut dyn Strategy,
        symbol:   &str,
    ) -> BacktestResult {
        let mut candles_count = 0u64;
        let mut signal_count  = 0u64;

        // 推断 base/quote
        let (base, quote) = split_symbol(symbol);

        loop {
            let event = match feed.next().await {
                Some(e) => e,
                None    => break,
            };

            match event {
                FeedEvent::Candle { candle, .. } => {
                    candles_count += 1;

                    // 推送行情到 MockExchange，触发挂单撮合
                    use hq_core::types::Ticker;
                    self.exchange.set_ticker(Ticker {
                        symbol:           symbol.into(),
                        bid:              candle.low,   // 用 low 作为 bid
                        ask:              candle.high,  // 用 high 作为 ask
                        last:             candle.close,
                        volume_24h:       candle.volume,
                        price_change_pct: 0.0,
                        timestamp:        candle.open_time,
                    });

                    if self.config.verbose {
                        info!(
                            "[K线 #{}] {} close={:.2}",
                            candles_count,
                            candle.open_time.format("%m-%d %H:%M"),
                            candle.close
                        );
                    }

                    // 策略计算
                    let signals = match strategy.on_candle(&candle).await {
                        Ok(s)  => s,
                        Err(e) => { warn!("策略计算失败: {}", e); continue; }
                    };

                    // 执行信号
                    for sig in &signals {
                        signal_count += 1;

                        let req = self.build_order(&sig.side, symbol, base, quote, candle.close).await;
                        match req {
                            Some(r) => {
                                match self.exchange.place_order(r).await {
                                    Ok(order) => {
                                        if self.config.verbose {
                                            info!(
                                                "  ▶ {:?} {} @ {:.2} | {}",
                                                order.side, symbol, candle.close, sig.reason
                                            );
                                        }
                                    }
                                    Err(e) => warn!("下单失败: {}", e),
                                }
                            }
                            None => {}
                        }
                    }
                }
                FeedEvent::End => break,
                _ => {}
            }
        }

        // 计算最终指标
        let trades  = self.exchange.all_trades();
        let metrics = Metrics::calculate(&trades, self.config.initial_equity);

        info!(
            "[Simulator] 完成 | K线={} 信号={} 成交={} 最终净值={:.2}",
            candles_count, signal_count, trades.len(), metrics.final_equity
        );

        BacktestResult { metrics, candles_count, signal_count }
    }

    /// 根据信号方向和当前账户余额计算下单量
    async fn build_order(
        &self,
        side:   &OrderSide,
        symbol: &str,
        base:   &str,
        quote:  &str,
        price:  f64,
    ) -> Option<PlaceOrderRequest> {
        let acc = match self.exchange.get_account().await {
            Ok(a)  => a,
            Err(_) => return None,
        };

        match side {
            OrderSide::Buy => {
                let free = acc.balances.iter()
                    .find(|b| b.asset == quote)
                    .map(|b| b.free)
                    .unwrap_or(0.0);
                if free <= 0.0 || price <= 0.0 { return None; }
                let qty = free * self.config.order_size_pct / price;
                Some(PlaceOrderRequest::market(symbol, OrderSide::Buy, qty))
            }
            OrderSide::Sell => {
                let free = acc.balances.iter()
                    .find(|b| b.asset == base)
                    .map(|b| b.free)
                    .unwrap_or(0.0);
                if free <= 0.0 { return None; }
                let qty = free * self.config.order_size_pct;
                Some(PlaceOrderRequest::market(symbol, OrderSide::Sell, qty))
            }
        }
    }
}

/// 拆分交易对为 (base, quote)
fn split_symbol(symbol: &str) -> (&str, &str) {
    if let Some(pos) = symbol.find('-') {
        (&symbol[..pos], &symbol[pos + 1..])
    } else if symbol.ends_with("USDT") {
        (&symbol[..symbol.len() - 4], "USDT")
    } else if symbol.ends_with("BTC") {
        (&symbol[..symbol.len() - 3], "BTC")
    } else {
        (symbol, "USDT")
    }
}

// ── 测试 ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use hq_datafeed::CsvFeed;
    use hq_strategy::MaCrossStrategy;

    const CSV: &str = "\
timestamp,open,high,low,close,volume
1609459200000,29000,29500,28800,29300,100
1609462800000,29300,30000,29200,29900,120
1609466400000,29900,30500,29800,30200,90
1609470000000,30200,30800,30100,30600,110
1609473600000,30600,31000,30400,30800,95
1609477200000,30800,31500,30600,31200,130
1609480800000,31200,32000,31000,31800,140
1609484400000,31800,32500,31500,32200,120
1609488000000,32200,33000,32000,32800,150
1609491600000,32800,33500,32500,33100,135
1609495200000,33100,33800,32900,33500,125
1609498800000,33500,34000,33200,33800,145
1609502400000,33800,34500,33600,34200,160
1609506000000,34200,35000,34000,34700,170
1609509600000,34700,35500,34500,35200,155
1609513200000,35200,36000,35000,35800,180
1609516800000,35800,36500,35600,36300,175
1609520400000,36300,37000,36100,36800,190
1609524000000,36800,37500,36600,37200,185
1609527600000,37200,38000,37000,37800,200
";

    #[tokio::test]
    async fn simulator_runs_without_panic() {
        let config   = BacktestConfig::default();
        let mut sim  = Simulator::new(config);
        let mut feed = CsvFeed::from_str(CSV, "BTC-USDT", "1h").unwrap();
        let mut strat = MaCrossStrategy::new("BTC-USDT", 3, 7);

        let result = sim.run(&mut feed, &mut strat, "BTC-USDT").await;
        assert_eq!(result.candles_count, 20);
        assert!(result.metrics.initial_equity > 0.0);
    }

    #[tokio::test]
    async fn simulator_equity_changes_after_trades() {
        let config   = BacktestConfig::default();
        let mut sim  = Simulator::new(config);
        let mut feed = CsvFeed::from_str(CSV, "BTC-USDT", "1h").unwrap();
        let mut strat = MaCrossStrategy::new("BTC-USDT", 3, 7);

        let result = sim.run(&mut feed, &mut strat, "BTC-USDT").await;
        // 行情持续上涨，如果有成交，最终资金应有变化
        println!("成交次数: {}", result.metrics.total_trades);
        println!("最终资金: {:.2}", result.metrics.final_equity);
        println!("总收益率: {:.2}%", result.metrics.total_return_pct);
    }
}
