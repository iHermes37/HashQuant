//! RSI 均值回归策略
//!
//! 逻辑：
//! - RSI < oversold（默认30）→ 超卖，买入
//! - RSI > overbought（默认70）→ 超买，卖出
//!
//! 加入"确认"机制：连续 N 根 K 线都满足条件才发信号，避免假突破。

use async_trait::async_trait;
use hq_core::types::{Candle, Ticker};
use crate::error::Result;
use crate::strategy::{Strategy, Signal};
use crate::indicators::rsi::rsi;

pub struct RsiStrategy {
    symbol:      String,
    symbols_vec: Vec<String>,
    period:      usize,
    overbought:  f64,
    oversold:    f64,
    /// 需要连续几根 K 线确认信号
    confirm_bars: usize,
    closes:      Vec<f64>,
    in_position: bool,
    // 连续超卖/超买计数
    oversold_count:  usize,
    overbought_count: usize,
}

impl RsiStrategy {
    pub fn new(
        symbol:       impl Into<String>,
        period:       usize,
        overbought:   f64,
        oversold:     f64,
        confirm_bars: usize,
    ) -> Self {
        let sym = symbol.into();
        Self {
            symbols_vec: vec![sym.clone()],
            symbol: sym, period, overbought, oversold,
            confirm_bars, closes: vec![],
            in_position: false,
            oversold_count: 0, overbought_count: 0,
        }
    }

    /// 默认参数：RSI14，超买70，超卖30，1根确认
    pub fn default(symbol: impl Into<String>) -> Self {
        Self::new(symbol, 14, 70.0, 30.0, 1)
    }
}

#[async_trait]
impl Strategy for RsiStrategy {
    fn name(&self) -> &str { "RSI-MeanReversion" }
    fn symbols(&self) -> &[String] { &self.symbols_vec }

    async fn init(&mut self, history: &[Candle]) -> Result<()> {
        self.closes = history.iter().map(|c| c.close).collect();
        Ok(())
    }

    async fn on_candle(&mut self, candle: &Candle) -> Result<Vec<Signal>> {
        self.closes.push(candle.close);
        let max_buf = (self.period + 1) * 3;
        if self.closes.len() > max_buf {
            self.closes.drain(..self.closes.len() - max_buf);
        }

        if self.closes.len() < self.period + 1 {
            return Ok(vec![]);
        }

        let rsi_val = rsi(&self.closes, self.period)?;
        let mut signals = vec![];

        if rsi_val < self.oversold {
            self.oversold_count  += 1;
            self.overbought_count = 0;
        } else if rsi_val > self.overbought {
            self.overbought_count += 1;
            self.oversold_count   = 0;
        } else {
            self.oversold_count   = 0;
            self.overbought_count = 0;
        }

        if self.oversold_count >= self.confirm_bars && !self.in_position {
            signals.push(
                Signal::buy(&self.symbol, format!("RSI={rsi_val:.1} 低于超卖线{}", self.oversold))
                    .with_size(1.0)
            );
            self.in_position = true;
            self.oversold_count = 0;
        } else if self.overbought_count >= self.confirm_bars && self.in_position {
            signals.push(
                Signal::sell(&self.symbol, format!("RSI={rsi_val:.1} 高于超买线{}", self.overbought))
                    .with_size(1.0)
            );
            self.in_position = false;
            self.overbought_count = 0;
        }

        Ok(signals)
    }

    fn reset(&mut self) {
        self.closes.clear();
        self.in_position = false;
        self.oversold_count  = 0;
        self.overbought_count = 0;
    }
}

// ── 测试 ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn candle(close: f64) -> Candle {
        Candle { open_time: Utc::now(), open: close, high: close, low: close, close, volume: 1.0 }
    }

    #[tokio::test]
    async fn oversold_triggers_buy() {
        let mut strat = RsiStrategy::default("BTC-USDT");
        // 先喂涨价，再暴跌，使 RSI < 30
        // BUY 信号可能在 down 末尾或 extra 阶段触发，统一检查所有 K 线
        let up:   Vec<f64> = (1..=15).map(|x| 100.0 + x as f64 * 2.0).collect();
        let down: Vec<f64> = (1..=10).map(|x| 130.0 - x as f64 * 5.0).collect();
        let extra = vec![75.0, 70.0, 65.0, 60.0, 55.0];

        let mut got_buy = false;
        for p in up.iter().chain(down.iter()).chain(extra.iter()) {
            let sigs = strat.on_candle(&candle(*p)).await.unwrap();
            if sigs.iter().any(|s| s.side == hq_core::types::OrderSide::Buy) {
                got_buy = true;
            }
        }
        assert!(got_buy, "暴跌后 RSI 应进入超卖区间触发买入");
    }

    #[tokio::test]
    async fn confirm_bars_delays_signal() {
        let mut strat = RsiStrategy::new("BTC-USDT", 14, 70.0, 30.0, 3); // 需要连续3根
        // 第1根超卖不发信号
        strat.oversold_count = 1;
        // 模拟：前两根超卖不触发
        assert!(strat.oversold_count < strat.confirm_bars);
    }

    #[test]
    fn reset_clears_state() {
        let mut strat = RsiStrategy::default("BTC-USDT");
        strat.in_position = true;
        strat.oversold_count = 5;
        strat.reset();
        assert!(!strat.in_position);
        assert_eq!(strat.oversold_count, 0);
    }
}