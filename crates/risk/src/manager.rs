//! 风控管理器
//!
//! `RiskManager` 包装 `Engine`，在每次信号触发后、订单提交前做风控检查。
//! 通过风控的信号正常执行，不通过的记录日志并丢弃（不崩溃）。
//!
//! 使用示例：
//! ```rust,no_run
//! use std::sync::Arc;
//! use hq_risk::manager::RiskManager;
//! use hq_risk::limits::RiskLimits;
//! use hq_exchange::MockExchange;
//!
//! let exchange = Arc::new(MockExchange::default_fees());
//! let limits   = RiskLimits::default();
//! let manager  = RiskManager::new(exchange, limits, 10000.0);
//! ```

use std::sync::Arc;
use std::collections::HashMap;
use chrono::{Utc, Datelike};
use tracing::{info, warn};
use hq_core::traits::Exchange;
use hq_core::types::PlaceOrderRequest;
use hq_datafeed::stream::{DataFeed, FeedEvent};
use hq_strategy::strategy::Strategy;
use crate::error::RiskError;
use crate::limits::{RiskLimits, RiskChecker};
use crate::position::PositionTracker;
use crate::monitor::DrawdownMonitor;

/// 风控管理器运行统计
#[derive(Debug, Default, Clone)]
pub struct RiskStats {
    pub signals_received:  u64,
    pub signals_approved:  u64,
    pub signals_rejected:  u64,
    pub orders_placed:     u64,
    pub orders_failed:     u64,
    pub risk_violations:   HashMap<String, u64>,
}

impl RiskStats {
    fn record_violation(&mut self, err: &RiskError) {
        let key = match err {
            RiskError::MaxPositionsExceeded { .. }  => "max_positions",
            RiskError::PositionSizeExceeded { .. }  => "position_size",
            RiskError::DailyLossExceeded { .. }     => "daily_loss",
            RiskError::MaxLossExceeded { .. }        => "max_loss",
            RiskError::EquityTooLow { .. }           => "equity_low",
            RiskError::LeverageExceeded { .. }       => "leverage",
            RiskError::Exchange(_)                   => "exchange",
        };
        *self.violations_mut(key) += 1;
        self.signals_rejected += 1;
    }

    fn violations_mut(&mut self, key: &str) -> &mut u64 {
        self.risk_violations.entry(key.to_string()).or_insert(0)
    }
}

/// 风控管理器
pub struct RiskManager {
    exchange:  Arc<dyn Exchange>,
    checker:   RiskChecker,
    tracker:   PositionTracker,
    drawdown:  DrawdownMonitor,
    stats:     RiskStats,
    /// 当前行情价格缓存，用于估算订单金额
    prices:    HashMap<String, f64>,
    last_day:  u32,
}

impl RiskManager {
    pub fn new(
        exchange:       Arc<dyn Exchange>,
        limits:         RiskLimits,
        initial_equity: f64,
    ) -> Self {
        let drawdown = DrawdownMonitor::new(initial_equity);
        Self {
            exchange,
            checker:  RiskChecker::new(limits, initial_equity),
            tracker:  PositionTracker::new(),
            drawdown,
            stats:    RiskStats::default(),
            prices:   HashMap::new(),
            last_day: Utc::now().day(),
        }
    }

    pub fn stats(&self)    -> &RiskStats     { &self.stats }
    pub fn tracker(&self)  -> &PositionTracker { &self.tracker }
    pub fn drawdown(&self) -> &DrawdownMonitor { &self.drawdown }

    /// 主运行循环：DataFeed → Strategy → RiskCheck → Exchange
    pub async fn run(
        &mut self,
        feed:     &mut dyn DataFeed,
        strategy: &mut dyn Strategy,
    ) -> std::result::Result<(), hq_core::error::CoreError> {
        info!("[RiskManager] 启动，策略: {}", strategy.name());

        loop {
            let event = match feed.next().await {
                Some(e) => e,
                None    => break,
            };

            // 每日重置检查
            let today = Utc::now().day();
            if today != self.last_day {
                self.checker.reset_daily();
                self.last_day = today;
                info!("[RiskManager] 每日风控计数已重置");
            }

            match event {
                FeedEvent::Candle { candle, .. } => {
                    self.prices.insert(candle.close.to_string(), candle.close);
                    // 更新该 symbol 的价格
                    let signals = strategy.on_candle(&candle).await
                        .unwrap_or_default();
                    self.process_signals(signals).await;
                }
                FeedEvent::Tick(ticker) => {
                    self.prices.insert(ticker.symbol.clone(), ticker.last);
                    self.checker.update_equity(self.estimate_equity());
                    self.drawdown.update(self.estimate_equity());
                    let signals = strategy.on_tick(&ticker).await
                        .unwrap_or_default();
                    self.process_signals(signals).await;
                }
                FeedEvent::End => {
                    info!("[RiskManager] 数据流结束");
                    break;
                }
                FeedEvent::Book(_) => {}
            }
        }

        self.print_summary(strategy.name());
        Ok(())
    }

    /// 处理信号列表：风控检查 → 下单
    async fn process_signals(&mut self, signals: Vec<hq_strategy::strategy::Signal>) {
        for sig in signals {
            self.stats.signals_received += 1;

            let price = self.prices.get(&sig.symbol).copied().unwrap_or(0.0);
            let qty   = self.estimate_qty(&sig, price);
            if qty <= 0.0 { continue; }

            let req = match sig.price {
                Some(p) => PlaceOrderRequest::limit(&sig.symbol, sig.side.clone(), qty, p),
                None    => PlaceOrderRequest::market(&sig.symbol, sig.side.clone(), qty),
            };

            // 拉取账户做风控检查
            let account = match self.exchange.get_account().await {
                Ok(a)  => a,
                Err(e) => { warn!("[RiskManager] 账户查询失败: {}", e); continue; }
            };

            match self.checker.check_order(&req, &account, &self.tracker, price) {
                Ok(()) => {
                    self.stats.signals_approved += 1;
                    info!("[RiskManager] ✅ 信号通过风控: {:?} {} qty={:.4} reason={}",
                          sig.side, sig.symbol, qty, sig.reason);
                    self.submit_order(req).await;
                }
                Err(e) => {
                    warn!("[RiskManager] ❌ 信号被风控拦截: {} | {}", sig.reason, e);
                    self.stats.record_violation(&e);
                }
            }
        }
    }

    /// 提交订单并更新仓位
    async fn submit_order(&mut self, req: PlaceOrderRequest) {
        match self.exchange.place_order(req).await {
            Ok(order) => {
                self.stats.orders_placed += 1;
                info!("[RiskManager] 订单成功: {} {:?} status={:?}",
                      order.order_id, order.side, order.status);
            }
            Err(e) => {
                self.stats.orders_failed += 1;
                warn!("[RiskManager] 订单失败: {}", e);
            }
        }
    }

    /// 根据信号和当前价格估算下单数量
    fn estimate_qty(&self, sig: &hq_strategy::strategy::Signal, price: f64) -> f64 {
        if price <= 0.0 { return 0.0; }
        let size_pct = sig.size_pct.unwrap_or(1.0);
        let equity   = self.estimate_equity();
        let budget   = equity * size_pct * self.checker.limits.max_position_pct / 100.0;
        budget / price
    }

    /// 简单净值估算（USDT 余额快照）
    fn estimate_equity(&self) -> f64 {
        self.checker.limits.min_equity.max(1000.0) // 最低保护值，实际应从账户拉取
    }

    fn print_summary(&self, strategy_name: &str) {
        info!(
            "[RiskManager] 完成 | 策略={} 信号收到={} 通过={} 拦截={} 下单={} 失败={} | 最大回撤={:.2}%",
            strategy_name,
            self.stats.signals_received, self.stats.signals_approved,
            self.stats.signals_rejected, self.stats.orders_placed,
            self.stats.orders_failed,
            self.drawdown.max_drawdown_pct(),
        );
        if !self.stats.risk_violations.is_empty() {
            info!("[RiskManager] 风控拦截明细: {:?}", self.stats.risk_violations);
        }
    }
}
