//! 布林带（Bollinger Bands）
//!
//! 标准参数：period=20, multiplier=2.0
//!
//! - `upper`  = SMA + multiplier * std_dev
//! - `middle` = SMA(period)
//! - `lower`  = SMA - multiplier * std_dev

use crate::error::{Result, StrategyError};

/// 布林带计算结果
#[derive(Debug, Clone)]
pub struct BollResult {
    pub upper:  f64,
    pub middle: f64,
    pub lower:  f64,
    /// 带宽百分比 = (upper - lower) / middle * 100
    pub bandwidth: f64,
    /// %B = (price - lower) / (upper - lower)，衡量价格在带内位置
    pub percent_b: f64,
}

/// 计算最新一个布林带值
pub fn bollinger_bands(
    prices:     &[f64],
    period:     usize,  // 通常 20
    multiplier: f64,    // 通常 2.0
) -> Result<BollResult> {
    if prices.len() < period {
        return Err(StrategyError::NotEnoughData { need: period, have: prices.len() });
    }
    let window = &prices[prices.len() - period..];
    let last   = *prices.last().unwrap();
    compute(window, last, multiplier)
}

/// 计算整条布林带序列
pub fn bollinger_series(
    prices:     &[f64],
    period:     usize,
    multiplier: f64,
) -> Result<Vec<BollResult>> {
    if prices.len() < period {
        return Err(StrategyError::NotEnoughData { need: period, have: prices.len() });
    }
    prices.windows(period)
        .enumerate()
        .map(|(i, window)| {
            let last = prices[i + period - 1];
            compute(window, last, multiplier)
        })
        .collect()
}

fn compute(window: &[f64], last_price: f64, multiplier: f64) -> Result<BollResult> {
    let n   = window.len() as f64;
    let mid = window.iter().sum::<f64>() / n;
    let variance = window.iter().map(|x| (x - mid).powi(2)).sum::<f64>() / n;
    let std_dev  = variance.sqrt();

    let upper = mid + multiplier * std_dev;
    let lower = mid - multiplier * std_dev;
    let bandwidth = if mid != 0.0 { (upper - lower) / mid * 100.0 } else { 0.0 };
    let percent_b = if (upper - lower).abs() > f64::EPSILON {
        (last_price - lower) / (upper - lower)
    } else { 0.5 };

    Ok(BollResult { upper, middle: mid, lower, bandwidth, percent_b })
}

// ── 测试 ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boll_constant_prices() {
        // 价格恒定时，std_dev=0，upper=middle=lower
        let prices = vec![100.0; 20];
        let r = bollinger_bands(&prices, 20, 2.0).unwrap();
        assert!((r.upper  - 100.0).abs() < 1e-9);
        assert!((r.middle - 100.0).abs() < 1e-9);
        assert!((r.lower  - 100.0).abs() < 1e-9);
        assert_eq!(r.bandwidth, 0.0);
    }

    #[test]
    fn boll_upper_above_lower() {
        let prices: Vec<f64> = (1..=25).map(|x| x as f64 + (x % 3) as f64).collect();
        let r = bollinger_bands(&prices, 20, 2.0).unwrap();
        assert!(r.upper > r.lower, "上轨必须高于下轨");
        assert!(r.upper > r.middle && r.middle > r.lower);
    }

    #[test]
    fn boll_not_enough_data() {
        let prices = vec![1.0, 2.0, 3.0];
        assert!(bollinger_bands(&prices, 20, 2.0).is_err());
    }

    #[test]
    fn boll_series_length() {
        let prices: Vec<f64> = (1..=30).map(|x| x as f64).collect();
        let s = bollinger_series(&prices, 20, 2.0).unwrap();
        assert_eq!(s.len(), 11); // 30 - 20 + 1
    }

    #[test]
    fn percent_b_bounds() {
        // 用固定窗口验证 %B 上下界：
        //   last = upper → %B = 1.0
        //   last = lower → %B = 0.0
        let prices: Vec<f64> = (1..=20).map(|x| 100.0 + (x % 5) as f64).collect();
        // 先算出 upper / lower（不修改 prices，保持窗口不变）
        let n = prices.len() as f64;
        let mid = prices.iter().sum::<f64>() / n;
        let std_dev = (prices.iter().map(|x| (x - mid).powi(2)).sum::<f64>() / n).sqrt();
        let upper = mid + 2.0 * std_dev;
        let lower = mid - 2.0 * std_dev;

        // 构造 last = upper 的输入（前19个不变，最后1个 = upper）
        let mut p_upper = prices[..19].to_vec();
        p_upper.push(upper);
        let r_upper = bollinger_bands(&p_upper, 20, 2.0).unwrap();
        assert!(r_upper.percent_b > 0.9, "%B 接近上轨应接近1.0，实际={}", r_upper.percent_b);

        // 构造 last = lower 的输入
        let mut p_lower = prices[..19].to_vec();
        p_lower.push(lower);
        let r_lower = bollinger_bands(&p_lower, 20, 2.0).unwrap();
        assert!(r_lower.percent_b < 0.1, "%B 接近下轨应接近0.0，实际={}", r_lower.percent_b);
    }
}