//! 策略接口与信号类型
//!
//! 策略实现 `Strategy` trait，接收 `FeedEvent`，
//! 返回 `Vec<Signal>`（买卖信号），由 `Engine` 执行。

use async_trait::async_trait;
use hq_core::types::{Candle, Ticker, OrderSide, PlaceOrderRequest};
use crate::error::Result;

// ── 信号 ──────────────────────────────────────────────────────────────────────

/// 策略发出的交易信号
#[derive(Debug, Clone)]
pub struct Signal {
    /// 交易标的
    pub symbol:    String,
    /// 买入/卖出方向
    pub side:      OrderSide,
    /// 信号类型
    pub kind:      SignalKind,
    /// 建议仓位比例，0.0–1.0（占账户可用资金的比例）
    /// None 表示由引擎使用默认仓位
    pub size_pct:  Option<f64>,
    /// 建议限价，None 表示市价
    pub price:     Option<f64>,
    /// 策略给出的信号理由（日志/回测分析用）
    pub reason:    String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SignalKind {
    /// 开仓信号
    Enter,
    /// 平仓信号
    Exit,
    /// 加仓信号
    AddPosition,
    /// 减仓信号
    ReducePosition,
}

impl Signal {
    pub fn buy(symbol: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(), side: OrderSide::Buy,
            kind: SignalKind::Enter, size_pct: None,
            price: None, reason: reason.into(),
        }
    }
    pub fn sell(symbol: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(), side: OrderSide::Sell,
            kind: SignalKind::Exit, size_pct: None,
            price: None, reason: reason.into(),
        }
    }
    pub fn with_size(mut self, pct: f64) -> Self {
        self.size_pct = Some(pct.clamp(0.0, 1.0));
        self
    }
    pub fn with_price(mut self, price: f64) -> Self {
        self.price = Some(price);
        self
    }
}

// ── 策略 Trait ────────────────────────────────────────────────────────────────

/// 所有策略必须实现的接口
#[async_trait]
pub trait Strategy: Send {
    /// 策略名称
    fn name(&self) -> &str;

    /// 策略订阅的交易对列表
    fn symbols(&self) -> &[String];

    /// 收到新 K 线时调用，返回信号列表
    async fn on_candle(&mut self, candle: &Candle) -> Result<Vec<Signal>>;

    /// 收到新 Tick 时调用（默认空实现，Tick 驱动的策略才需要重写）
    async fn on_tick(&mut self, _ticker: &Ticker) -> Result<Vec<Signal>> {
        Ok(vec![])
    }

    /// 策略初始化（加载历史数据、预热指标等）
    async fn init(&mut self, _history: &[Candle]) -> Result<()> {
        Ok(())
    }

    /// 策略重置（回测多轮时使用）
    fn reset(&mut self) {}
}
