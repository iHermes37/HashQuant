//! MACD（移动平均收敛/发散）
//!
//! 标准参数：fast=12, slow=26, signal=9
//!
//! - `macd_line`  = EMA(fast) - EMA(slow)
//! - `signal`     = EMA(macd_line, signal_period)
//! - `histogram`  = macd_line - signal

use crate::error::{Result, StrategyError};
use crate::indicators::ma::ema_series;

/// 单个 MACD 计算结果
#[derive(Debug, Clone)]
pub struct MacdResult {
    pub macd_line: f64,
    pub signal:    f64,
    pub histogram: f64,
}

/// 计算最新一个 MACD 值
pub fn macd(
    prices:        &[f64],
    fast_period:   usize,  // 通常 12
    slow_period:   usize,  // 通常 26
    signal_period: usize,  // 通常 9
) -> Result<MacdResult> {
    let series = macd_series(prices, fast_period, slow_period, signal_period)?;
    Ok(series.into_iter().last().unwrap())
}

/// 计算整条 MACD 序列
pub fn macd_series(
    prices:        &[f64],
    fast_period:   usize,
    slow_period:   usize,
    signal_period: usize,
) -> Result<Vec<MacdResult>> {
    let min_len = slow_period + signal_period;
    if prices.len() < min_len {
        return Err(StrategyError::NotEnoughData { need: min_len, have: prices.len() });
    }

    let fast_ema = ema_series(prices, fast_period)?;
    let slow_ema = ema_series(prices, slow_period)?;

    // fast_ema 比 slow_ema 长 (slow_period - fast_period) 个
    // 对齐到相同长度（取 slow_ema 的长度）
    let offset = fast_ema.len() - slow_ema.len();
    let macd_line: Vec<f64> = fast_ema[offset..].iter()
        .zip(slow_ema.iter())
        .map(|(f, s)| f - s)
        .collect();

    // signal = EMA(macd_line, signal_period)
    let signal_line = ema_series(&macd_line, signal_period)?;

    // 对齐 macd_line 和 signal_line
    let macd_offset = macd_line.len() - signal_line.len();
    let result = macd_line[macd_offset..].iter()
        .zip(signal_line.iter())
        .map(|(&m, &s)| MacdResult {
            macd_line: m,
            signal:    s,
            histogram: m - s,
        })
        .collect();

    Ok(result)
}

// ── 测试 ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rising_prices(n: usize) -> Vec<f64> {
        (1..=n).map(|x| x as f64 * 10.0).collect()
    }

    #[test]
    fn macd_returns_result() {
        let prices = rising_prices(50);
        let r = macd(&prices, 12, 26, 9).unwrap();
        // 上涨趋势中 fast EMA > slow EMA，macd_line > 0
        assert!(r.macd_line > 0.0, "上涨趋势 MACD 线应>0");
    }

    #[test]
    fn macd_not_enough_data() {
        let prices = rising_prices(30); // 需要 26+9=35
        assert!(macd(&prices, 12, 26, 9).is_err());
    }

    #[test]
    fn macd_series_length() {
        let prices = rising_prices(60);
        let s = macd_series(&prices, 12, 26, 9).unwrap();
        // slow_ema 长度 = 60-26+1=35，signal 长度 = 35-9+1=27
        assert_eq!(s.len(), 27);
    }

    #[test]
    fn histogram_equals_macd_minus_signal() {
        let prices = rising_prices(50);
        let series = macd_series(&prices, 12, 26, 9).unwrap();
        for r in &series {
            let diff = (r.histogram - (r.macd_line - r.signal)).abs();
            assert!(diff < 1e-9);
        }
    }
}
