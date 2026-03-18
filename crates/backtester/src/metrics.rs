//! 回测绩效指标
//!
//! 输入：成交记录列表 + 初始资金
//! 输出：胜率、盈亏比、最大回撤、夏普比率、年化收益等

use chrono::{DateTime, Utc};
use hq_core::types::{Trade, OrderSide};

/// 单笔交易的盈亏记录（买卖配对后计算）
#[derive(Debug, Clone)]
pub struct TradePnl {
    pub entry_price:  f64,
    pub exit_price:   f64,
    pub qty:          f64,
    pub pnl:          f64,   // 净盈亏（已扣手续费）
    pub pnl_pct:      f64,   // 盈亏百分比
    pub is_win:       bool,
    pub entry_time:   DateTime<Utc>,
    pub exit_time:    DateTime<Utc>,
}

/// 完整绩效指标
#[derive(Debug, Clone)]
pub struct Metrics {
    // ── 基础统计 ─────────────────────────────────────────────────────────────
    pub initial_equity:    f64,
    pub final_equity:      f64,
    pub total_return_pct:  f64,   // 总收益率 %
    pub annualized_return: f64,   // 年化收益率 %

    // ── 交易统计 ─────────────────────────────────────────────────────────────
    pub total_trades:      usize,
    pub winning_trades:    usize,
    pub losing_trades:     usize,
    pub win_rate:          f64,   // 胜率 %
    pub avg_win:           f64,   // 平均盈利
    pub avg_loss:          f64,   // 平均亏损（正数）
    pub profit_factor:     f64,   // 盈亏比 = 总盈利 / 总亏损
    pub total_fees:        f64,   // 累计手续费

    // ── 风险指标 ─────────────────────────────────────────────────────────────
    pub max_drawdown_pct:  f64,   // 最大回撤 %
    pub max_drawdown_abs:  f64,   // 最大回撤绝对值
    pub sharpe_ratio:      f64,   // 夏普比率（年化，无风险利率=0）
    pub calmar_ratio:      f64,   // 卡尔玛比率 = 年化收益 / 最大回撤

    // ── 时间 ─────────────────────────────────────────────────────────────────
    pub start_time:        Option<DateTime<Utc>>,
    pub end_time:          Option<DateTime<Utc>>,
    pub duration_days:     f64,

    /// 每笔配对交易的详情
    pub trade_pnls:        Vec<TradePnl>,
    /// 净值曲线（每笔交易后的账户净值）
    pub equity_curve:      Vec<f64>,
}

impl Metrics {
    /// 从成交列表和初始资金计算所有指标
    pub fn calculate(trades: &[Trade], initial_equity: f64) -> Self {
        if trades.is_empty() {
            return Self::empty(initial_equity);
        }

        // 把买卖配对，计算每笔交易盈亏
        let trade_pnls = pair_trades(trades);

        // 净值曲线
        let mut equity = initial_equity;
        let mut equity_curve = vec![equity];
        for tp in &trade_pnls {
            equity += tp.pnl;
            equity_curve.push(equity);
        }
        let final_equity = equity;

        // 基础统计
        let total_trades   = trade_pnls.len();
        let winning_trades = trade_pnls.iter().filter(|t| t.is_win).count();
        let losing_trades  = total_trades - winning_trades;
        let win_rate       = if total_trades > 0 {
            winning_trades as f64 / total_trades as f64 * 100.0
        } else { 0.0 };

        let wins:  Vec<f64> = trade_pnls.iter().filter(|t| t.is_win).map(|t| t.pnl).collect();
        let losses: Vec<f64> = trade_pnls.iter().filter(|t| !t.is_win).map(|t| t.pnl.abs()).collect();

        let total_profit: f64 = wins.iter().sum();
        let total_loss:   f64 = losses.iter().sum();
        let avg_win       = if !wins.is_empty() { total_profit / wins.len() as f64 } else { 0.0 };
        let avg_loss      = if !losses.is_empty() { total_loss / losses.len() as f64 } else { 0.0 };
        let profit_factor = if total_loss > 0.0 { total_profit / total_loss } else { f64::INFINITY };

        let total_fees: f64 = trades.iter().map(|t| t.fee).sum();

        // 收益率
        let total_return_pct = (final_equity - initial_equity) / initial_equity * 100.0;

        // 时间跨度
        let start_time = trades.first().map(|t| t.timestamp);
        let end_time   = trades.last().map(|t| t.timestamp);
        let duration_days = match (start_time, end_time) {
            (Some(s), Some(e)) => (e - s).num_hours() as f64 / 24.0,
            _ => 1.0,
        };
        let years = (duration_days / 365.0).max(1.0 / 365.0);

        // 年化收益率（CAGR）
        let annualized_return = if years > 0.0 {
            ((final_equity / initial_equity).powf(1.0 / years) - 1.0) * 100.0
        } else { total_return_pct };

        // 最大回撤
        let (max_drawdown_pct, max_drawdown_abs) = calc_max_drawdown(&equity_curve);

        // 夏普比率（用净值曲线的收益序列）
        let sharpe_ratio = calc_sharpe(&equity_curve);

        // 卡尔玛比率
        let calmar_ratio = if max_drawdown_pct > 0.0 {
            annualized_return / max_drawdown_pct
        } else { f64::INFINITY };

        Metrics {
            initial_equity, final_equity,
            total_return_pct, annualized_return,
            total_trades, winning_trades, losing_trades,
            win_rate, avg_win, avg_loss, profit_factor, total_fees,
            max_drawdown_pct, max_drawdown_abs,
            sharpe_ratio, calmar_ratio,
            start_time, end_time, duration_days,
            trade_pnls, equity_curve,
        }
    }

    fn empty(initial_equity: f64) -> Self {
        Metrics {
            initial_equity, final_equity: initial_equity,
            total_return_pct: 0.0, annualized_return: 0.0,
            total_trades: 0, winning_trades: 0, losing_trades: 0,
            win_rate: 0.0, avg_win: 0.0, avg_loss: 0.0,
            profit_factor: 0.0, total_fees: 0.0,
            max_drawdown_pct: 0.0, max_drawdown_abs: 0.0,
            sharpe_ratio: 0.0, calmar_ratio: 0.0,
            start_time: None, end_time: None, duration_days: 0.0,
            trade_pnls: vec![], equity_curve: vec![initial_equity],
        }
    }
}

/// 买卖配对，计算每轮完整交易的盈亏
fn pair_trades(trades: &[Trade]) -> Vec<TradePnl> {
    let mut result = vec![];
    let mut pending_buys: Vec<&Trade> = vec![];

    for trade in trades {
        match trade.side {
            OrderSide::Buy => pending_buys.push(trade),
            OrderSide::Sell => {
                if let Some(buy) = pending_buys.pop() {
                    let gross_pnl = (trade.price - buy.price) * trade.qty.min(buy.qty);
                    let fees      = buy.fee + trade.fee;
                    let pnl       = gross_pnl - fees;
                    let pnl_pct   = pnl / (buy.price * buy.qty) * 100.0;
                    result.push(TradePnl {
                        entry_price: buy.price,
                        exit_price:  trade.price,
                        qty:         trade.qty.min(buy.qty),
                        pnl, pnl_pct,
                        is_win:      pnl > 0.0,
                        entry_time:  buy.timestamp,
                        exit_time:   trade.timestamp,
                    });
                }
            }
        }
    }
    result
}

fn calc_max_drawdown(equity_curve: &[f64]) -> (f64, f64) {
    if equity_curve.len() < 2 { return (0.0, 0.0); }
    let mut peak = equity_curve[0];
    let mut max_dd_pct = 0.0f64;
    let mut max_dd_abs = 0.0f64;
    for &e in equity_curve {
        if e > peak { peak = e; }
        let dd_abs = peak - e;
        let dd_pct = dd_abs / peak * 100.0;
        if dd_pct > max_dd_pct { max_dd_pct = dd_pct; max_dd_abs = dd_abs; }
    }
    (max_dd_pct, max_dd_abs)
}

fn calc_sharpe(equity_curve: &[f64]) -> f64 {
    if equity_curve.len() < 2 { return 0.0; }
    let returns: Vec<f64> = equity_curve.windows(2)
        .map(|w| (w[1] - w[0]) / w[0])
        .collect();
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>()
        / returns.len() as f64;
    let std = variance.sqrt();
    if std == 0.0 { return 0.0; }
    // 年化（假设每笔交易间隔约1小时）
    mean / std * (8760_f64).sqrt()
}

// ── 测试 ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_trade(side: OrderSide, price: f64, qty: f64) -> Trade {
        Trade {
            trade_id: "t".into(), order_id: "o".into(),
            symbol: "BTC-USDT".into(), side, price, qty,
            fee: price * qty * 0.001,
            fee_asset: "USDT".into(), timestamp: Utc::now(),
        }
    }

    #[test]
    fn metrics_profitable_trades() {
        let trades = vec![
            make_trade(OrderSide::Buy,  30000.0, 0.1),
            make_trade(OrderSide::Sell, 35000.0, 0.1),
            make_trade(OrderSide::Buy,  34000.0, 0.1),
            make_trade(OrderSide::Sell, 36000.0, 0.1),
        ];
        let m = Metrics::calculate(&trades, 10000.0);
        assert_eq!(m.total_trades, 2);
        assert_eq!(m.winning_trades, 2);
        assert_eq!(m.win_rate, 100.0);
        assert!(m.final_equity > m.initial_equity);
        assert!(m.profit_factor > 1.0);
    }

    #[test]
    fn metrics_mixed_trades() {
        let trades = vec![
            make_trade(OrderSide::Buy,  30000.0, 0.1),
            make_trade(OrderSide::Sell, 32000.0, 0.1), // 盈利
            make_trade(OrderSide::Buy,  35000.0, 0.1),
            make_trade(OrderSide::Sell, 33000.0, 0.1), // 亏损
        ];
        let m = Metrics::calculate(&trades, 10000.0);
        assert_eq!(m.total_trades, 2);
        assert_eq!(m.winning_trades, 1);
        assert_eq!(m.losing_trades, 1);
        assert!((m.win_rate - 50.0).abs() < 1e-9);
    }

    #[test]
    fn metrics_empty_trades() {
        let m = Metrics::calculate(&[], 10000.0);
        assert_eq!(m.total_trades, 0);
        assert_eq!(m.final_equity, 10000.0);
    }

    #[test]
    fn max_drawdown_correct() {
        let curve = vec![10000.0, 12000.0, 9600.0, 11000.0];
        let (dd_pct, dd_abs) = calc_max_drawdown(&curve);
        // 从 12000 跌到 9600，回撤 = 2400/12000 = 20%
        assert!((dd_pct - 20.0).abs() < 1e-6);
        assert!((dd_abs - 2400.0).abs() < 1e-6);
    }
}
