//! 相对强弱指数（RSI）
//!
//! 标准 Wilder RSI，period 通常取 14。
//! 返回 0–100 之间的值：
//! - RSI > 70 → 超买
//! - RSI < 30 → 超卖

use crate::error::{Result, StrategyError};

/// 计算最新一个 RSI 值
pub fn rsi(prices: &[f64], period: usize) -> Result<f64> {
    let series = rsi_series(prices, period)?;
    Ok(*series.last().unwrap())
}

/// 计算整条 RSI 序列
/// 返回长度 = prices.len() - period
pub fn rsi_series(prices: &[f64], period: usize) -> Result<Vec<f64>> {
    // 需要 period+1 个价格才能算出第一个 RSI
    if prices.len() < period + 1 {
        return Err(StrategyError::NotEnoughData {
            need: period + 1,
            have: prices.len(),
        });
    }

    // 计算每日涨跌
    let changes: Vec<f64> = prices.windows(2)
        .map(|w| w[1] - w[0])
        .collect();

    // 用前 period 个变化量计算初始平均涨/跌（Wilder 平滑）
    let (init_gain, init_loss) = changes[..period].iter().fold((0.0, 0.0), |(g, l), &c| {
        if c >= 0.0 { (g + c, l) } else { (g, l + c.abs()) }
    });

    let mut avg_gain = init_gain / period as f64;
    let mut avg_loss = init_loss / period as f64;
    let mut result = Vec::new();

    // 第一个 RSI
    result.push(wilder_rsi(avg_gain, avg_loss));

    // 后续使用 Wilder 平滑
    for &change in &changes[period..] {
        let gain = if change >= 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { change.abs() } else { 0.0 };
        avg_gain = (avg_gain * (period - 1) as f64 + gain) / period as f64;
        avg_loss = (avg_loss * (period - 1) as f64 + loss) / period as f64;
        result.push(wilder_rsi(avg_gain, avg_loss));
    }

    Ok(result)
}

#[inline]
fn wilder_rsi(avg_gain: f64, avg_loss: f64) -> f64 {
    if avg_loss == 0.0 {
        return 100.0;
    }
    let rs = avg_gain / avg_loss;
    100.0 - (100.0 / (1.0 + rs))
}

// ── 测试 ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rsi_range() {
        // 单调上涨 → RSI 接近 100
        let up: Vec<f64> = (1..=20).map(|x| x as f64).collect();
        let v = rsi(&up, 14).unwrap();
        assert!(v > 90.0, "单调上涨 RSI 应接近100，实际={}", v);

        // 单调下跌 → RSI 接近 0
        let down: Vec<f64> = (1..=20).rev().map(|x| x as f64).collect();
        let v = rsi(&down, 14).unwrap();
        assert!(v < 10.0, "单调下跌 RSI 应接近0，实际={}", v);
    }

    #[test]
    fn rsi_series_length() {
        let prices: Vec<f64> = (1..=20).map(|x| x as f64).collect();
        let s = rsi_series(&prices, 14).unwrap();
        // 20 个价格，14 period → 20 - 14 = 6 个 RSI
        assert_eq!(s.len(), 6);
    }

    #[test]
    fn rsi_not_enough_data() {
        let prices = vec![1.0, 2.0, 3.0];
        assert!(rsi(&prices, 14).is_err());
    }

    #[test]
    fn rsi_constant_prices() {
        // 价格不变，没有涨跌，RSI 通常为 100（avg_loss=0）
        let prices = vec![100.0; 20];
        let v = rsi(&prices, 14).unwrap();
        assert_eq!(v, 100.0);
    }

    #[test]
    fn rsi_all_values_in_range() {
        let prices = vec![
            44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.15,
            43.61, 44.33, 44.83, 45.10, 45.15, 43.61, 44.33, 44.83,
        ];
        let series = rsi_series(&prices, 14).unwrap();
        for v in series {
            assert!(v >= 0.0 && v <= 100.0, "RSI 超出范围: {}", v);
        }
    }
}
