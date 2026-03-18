//! 移动平均线
//!
//! - `sma(prices, period)` — 简单移动平均（最新一个值）
//! - `ema(prices, period)` — 指数移动平均（最新一个值）
//! - `sma_series(prices, period)` — 整条 SMA 序列
//! - `ema_series(prices, period)` — 整条 EMA 序列

use crate::error::{Result, StrategyError};

/// 简单移动平均，返回最新一个值
pub fn sma(prices: &[f64], period: usize) -> Result<f64> {
    if prices.len() < period {
        return Err(StrategyError::NotEnoughData { need: period, have: prices.len() });
    }
    let window = &prices[prices.len() - period..];
    Ok(window.iter().sum::<f64>() / period as f64)
}

/// 指数移动平均，返回最新一个值
/// 使用标准平滑系数 k = 2 / (period + 1)
pub fn ema(prices: &[f64], period: usize) -> Result<f64> {
    let series = ema_series(prices, period)?;
    Ok(*series.last().unwrap())
}

/// 整条 SMA 序列，长度 = prices.len() - period + 1
pub fn sma_series(prices: &[f64], period: usize) -> Result<Vec<f64>> {
    if prices.len() < period {
        return Err(StrategyError::NotEnoughData { need: period, have: prices.len() });
    }
    let result = prices.windows(period)
        .map(|w| w.iter().sum::<f64>() / period as f64)
        .collect();
    Ok(result)
}

/// 整条 EMA 序列，长度 = prices.len() - period + 1
pub fn ema_series(prices: &[f64], period: usize) -> Result<Vec<f64>> {
    if prices.len() < period {
        return Err(StrategyError::NotEnoughData { need: period, have: prices.len() });
    }
    let k = 2.0 / (period as f64 + 1.0);
    // 第一个值用 SMA 作为种子
    let seed: f64 = prices[..period].iter().sum::<f64>() / period as f64;
    let mut result = vec![seed];
    for &price in &prices[period..] {
        let prev = *result.last().unwrap();
        result.push(price * k + prev * (1.0 - k));
    }
    Ok(result)
}

// ── 测试 ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sma_basic() {
        let prices = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(sma(&prices, 3).unwrap(), 4.0); // (3+4+5)/3
    }

    #[test]
    fn sma_not_enough_data() {
        let prices = vec![1.0, 2.0];
        assert!(sma(&prices, 5).is_err());
    }

    #[test]
    fn sma_series_length() {
        let prices = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let s = sma_series(&prices, 3).unwrap();
        assert_eq!(s.len(), 3); // 5 - 3 + 1
        assert_eq!(s[0], 2.0); // (1+2+3)/3
        assert_eq!(s[2], 4.0); // (3+4+5)/3
    }

    #[test]
    fn ema_seed_equals_sma() {
        let prices = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let series = ema_series(&prices, 3).unwrap();
        // 第一个 EMA 值 = SMA(前3个) = (10+20+30)/3 = 20
        assert!((series[0] - 20.0).abs() < 1e-9);
    }

    #[test]
    fn ema_series_length() {
        let prices = vec![1.0; 10];
        let s = ema_series(&prices, 4).unwrap();
        assert_eq!(s.len(), 7); // 10 - 4 + 1
    }

    #[test]
    fn ema_constant_prices_equals_price() {
        // 当价格恒定时 EMA = 价格本身
        let prices = vec![100.0; 20];
        let v = ema(&prices, 5).unwrap();
        assert!((v - 100.0).abs() < 1e-9);
    }
}
