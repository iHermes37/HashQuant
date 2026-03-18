//! 回测报告打印

use crate::metrics::Metrics;

/// 打印完整回测报告到终端
pub fn print_report(metrics: &Metrics, strategy_name: &str, symbol: &str) {
    let sep = "═".repeat(52);
    let thin = "─".repeat(52);

    println!("\n{}", sep);
    println!("  📊 回测报告");
    println!("  策略: {}   交易对: {}", strategy_name, symbol);
    if let (Some(s), Some(e)) = (metrics.start_time, metrics.end_time) {
        println!("  时间: {} → {}", s.format("%Y-%m-%d"), e.format("%Y-%m-%d"));
        println!("  跨度: {:.1} 天", metrics.duration_days);
    }
    println!("{}", sep);

    // ── 资金 ──────────────────────────────────────────────────────────────────
    println!("  💰 资金");
    println!("  {:<22} {:>12.2} USDT", "初始资金:", metrics.initial_equity);
    println!("  {:<22} {:>12.2} USDT", "最终资金:", metrics.final_equity);
    let sign = if metrics.total_return_pct >= 0.0 { "+" } else { "" };
    println!("  {:<22} {:>11}{:.2}%", "总收益率:", sign, metrics.total_return_pct);
    println!("  {:<22} {:>11}{:.2}%", "年化收益:", sign, metrics.annualized_return);
    println!("  {:<22} {:>12.2} USDT", "累计手续费:", metrics.total_fees);
    println!("{}", thin);

    // ── 交易统计 ──────────────────────────────────────────────────────────────
    println!("  📈 交易统计");
    println!("  {:<22} {:>12}", "总交易次数:", metrics.total_trades);
    println!("  {:<22} {:>11}{:.1}%", "胜率:",
             "", metrics.win_rate);
    println!("  {:<22} {:>6} 盈 / {:>4} 亏",
             "盈亏笔数:", metrics.winning_trades, metrics.losing_trades);
    println!("  {:<22} {:>12.2} USDT", "平均盈利:", metrics.avg_win);
    println!("  {:<22} {:>12.2} USDT", "平均亏损:", metrics.avg_loss);
    let pf = if metrics.profit_factor.is_infinite() {
        "  ∞".to_string()
    } else {
        format!("{:>12.2}", metrics.profit_factor)
    };
    println!("  {:<22} {}", "盈亏比:", pf);
    println!("{}", thin);

    // ── 风险指标 ──────────────────────────────────────────────────────────────
    println!("  ⚠️  风险指标");
    println!("  {:<22} {:>11.2}%", "最大回撤:", metrics.max_drawdown_pct);
    println!("  {:<22} {:>12.2} USDT", "最大回撤(绝对):", metrics.max_drawdown_abs);
    println!("  {:<22} {:>12.2}", "夏普比率:", metrics.sharpe_ratio);
    let calmar = if metrics.calmar_ratio.is_infinite() {
        "  ∞".to_string()
    } else {
        format!("{:>12.2}", metrics.calmar_ratio)
    };
    println!("  {:<22} {}", "卡尔玛比率:", calmar);
    println!("{}", sep);

    // ── 每笔交易明细（最多显示20条）──────────────────────────────────────────
    if !metrics.trade_pnls.is_empty() {
        println!("  📋 交易明细（共{}笔，显示最近{}笔）",
                 metrics.trade_pnls.len(),
                 metrics.trade_pnls.len().min(20));
        println!("  {:<6} {:<12} {:<12} {:<10} {:<10} {}",
                 "序号", "买入价", "卖出价", "数量", "盈亏", "结果");
        println!("  {}", "─".repeat(60));

        let start = metrics.trade_pnls.len().saturating_sub(20);
        for (i, tp) in metrics.trade_pnls[start..].iter().enumerate() {
            let icon = if tp.is_win { "✅" } else { "❌" };
            println!(
                "  #{:<5} {:<12.2} {:<12.2} {:<10.4} {:>+9.2}  {}",
                start + i + 1,
                tp.entry_price, tp.exit_price, tp.qty, tp.pnl, icon
            );
        }
        println!("{}", sep);
    }

    // ── 简单净值曲线（ASCII）──────────────────────────────────────────────────
    if metrics.equity_curve.len() > 2 {
        print_ascii_chart(&metrics.equity_curve);
    }

    println!();
}

/// 简单的 ASCII 净值曲线
fn print_ascii_chart(curve: &[f64]) {
    let height = 8usize;
    let width  = curve.len().min(60);
    let step   = (curve.len() as f64 / width as f64).ceil() as usize;

    let sampled: Vec<f64> = curve.iter().step_by(step.max(1)).copied().collect();
    let min_v = sampled.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_v = sampled.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = (max_v - min_v).max(1.0);

    println!("  📉 净值曲线");
    println!("  {:>10.0} ┐", max_v);

    for row in (0..height).rev() {
        let threshold = min_v + range * row as f64 / (height - 1) as f64;
        let line: String = sampled.iter().map(|&v| {
            if v >= threshold { '█' } else { ' ' }
        }).collect();
        if row == height / 2 {
            println!("  {:>10.0} │{}", (min_v + max_v) / 2.0, line);
        } else {
            println!("             │{}", line);
        }
    }
    println!("  {:>10.0} └{}", min_v, "─".repeat(sampled.len()));
    println!("             开始{}结束", " ".repeat(sampled.len().saturating_sub(4)));
}
