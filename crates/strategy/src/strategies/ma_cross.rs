//! 均线交叉策略（MA Cross）
//!
//! 逻辑：
//! - 快线（短周期 EMA）上穿慢线（长周期 EMA）→ 买入信号
//! - 快线下穿慢线 → 卖出信号
//!
//! 默认参数：fast=9, slow=21

use async_trait::async_trait;
use hq_core::types::{Candle, Ticker};
use crate::error::{Result, StrategyError};
use crate::strategy::{Strategy, Signal};
use crate::indicators::ma::ema_series;

pub struct MaCrossStrategy {
    symbol:      String,
    symbols_vec: Vec<String>,
    fast_period: usize,
    slow_period: usize,
    /// 历史收盘价缓冲（最多保留 slow_period * 3 根，节省内存）
    closes:      Vec<f64>,
    /// 上一根 K 线的位置关系（true = fast > slow）
    prev_fast_above: Option<bool>,
    /// 当前是否持仓
    in_position: bool,
}

impl MaCrossStrategy {
    pub fn new(symbol: impl Into<String>, fast_period: usize, slow_period: usize) -> Self {
        let sym = symbol.into();
        Self {
            symbols_vec: vec![sym.clone()],
            symbol: sym,
            fast_period,
            slow_period,
            closes: vec![],
            prev_fast_above: None,
            in_position: false,
        }
    }

    /// 默认参数：EMA9 × EMA21
    pub fn default(symbol: impl Into<String>) -> Self {
        Self::new(symbol, 9, 21)
    }

    /// 需要的最少历史 K 线数量
    pub fn min_bars(&self) -> usize { self.slow_period + 1 }
}

#[async_trait]
impl Strategy for MaCrossStrategy {
    fn name(&self) -> &str { "MA-Cross" }
    fn symbols(&self) -> &[String] { &self.symbols_vec }

    async fn init(&mut self, history: &[Candle]) -> Result<()> {
        self.closes = history.iter().map(|c| c.close).collect();
        Ok(())
    }

    async fn on_candle(&mut self, candle: &Candle) -> Result<Vec<Signal>> {
        self.closes.push(candle.close);

        // 保留最近 slow_period * 3 根，避免内存无限增长
        let max_buf = self.slow_period * 3;
        if self.closes.len() > max_buf {
            self.closes.drain(..self.closes.len() - max_buf);
        }

        if self.closes.len() < self.min_bars() {
            return Ok(vec![]); // 数据不足，不发信号
        }

        let fast_ema = ema_series(&self.closes, self.fast_period)?;
        let slow_ema = ema_series(&self.closes, self.slow_period)?;

        let fast_now = *fast_ema.last().unwrap();
        let slow_now = *slow_ema.last().unwrap();
        let fast_above_now = fast_now > slow_now;

        let mut signals = vec![];

        if let Some(was_above) = self.prev_fast_above {
            match (was_above, fast_above_now) {
                // 金叉：fast 上穿 slow
                (false, true) if !self.in_position => {
                    signals.push(
                        Signal::buy(&self.symbol, format!(
                            "金叉: EMA{fast}={fast_now:.2} 上穿 EMA{slow}={slow_now:.2}",
                            fast = self.fast_period, slow = self.slow_period
                        )).with_size(1.0)
                    );
                    self.in_position = true;
                }
                // 死叉：fast 下穿 slow
                (true, false) if self.in_position => {
                    signals.push(
                        Signal::sell(&self.symbol, format!(
                            "死叉: EMA{fast}={fast_now:.2} 下穿 EMA{slow}={slow_now:.2}",
                            fast = self.fast_period, slow = self.slow_period
                        )).with_size(1.0)
                    );
                    self.in_position = false;
                }
                _ => {}
            }
        }

        self.prev_fast_above = Some(fast_above_now);
        Ok(signals)
    }

    fn reset(&mut self) {
        self.closes.clear();
        self.prev_fast_above = None;
        self.in_position = false;
    }
}

// ── 测试 ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_candle(close: f64) -> Candle {
        Candle { open_time: Utc::now(), open: close, high: close, low: close, close, volume: 1.0 }
    }

    #[tokio::test]
    async fn golden_cross_triggers_buy() {
        let mut strat = MaCrossStrategy::new("BTC-USDT", 3, 5);

        // 先喂下跌数据使 fast < slow
        let down = vec![100.0, 98.0, 96.0, 94.0, 92.0, 90.0, 88.0, 86.0];
        for p in &down {
            strat.on_candle(&make_candle(*p)).await.unwrap();
        }

        // 再喂强势上涨，触发金叉
        let up = vec![95.0, 102.0, 110.0, 118.0, 126.0];
        let mut got_buy = false;
        for p in &up {
            let signals = strat.on_candle(&make_candle(*p)).await.unwrap();
            if signals.iter().any(|s| s.side == hq_core::types::OrderSide::Buy) {
                got_buy = true;
            }
        }
        assert!(got_buy, "强势上涨后应触发买入信号");
    }

    #[tokio::test]
    async fn no_signal_without_enough_data() {
        let mut strat = MaCrossStrategy::new("BTC-USDT", 3, 5);
        for p in [100.0, 101.0, 102.0] {
            let signals = strat.on_candle(&make_candle(p)).await.unwrap();
            assert!(signals.is_empty(), "数据不足时不应发出信号");
        }
    }

    #[tokio::test]
    async fn reset_clears_state() {
        let mut strat = MaCrossStrategy::new("BTC-USDT", 3, 5);
        for p in (1..=10).map(|x| x as f64 * 10.0) {
            strat.on_candle(&make_candle(p)).await.unwrap();
        }
        strat.reset();
        assert!(strat.closes.is_empty());
        assert!(strat.prev_fast_above.is_none());
    }
}
