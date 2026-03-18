//! 回测入口
//!
//! 运行方式：
//!   cargo run --bin backtest -- --csv data/eth_1h.csv
//!
//! 或直接修改下方常量后运行：
//!   cargo run --bin backtest

use hq_backtester::{Simulator, BacktestConfig};
use hq_backtester::reporter::print_report;
use hq_datafeed::CsvFeed;
use hq_strategy::MaCrossStrategy;
use hq_strategy::strategies::RsiStrategy;

// ── 回测参数（按需修改）──────────────────────────────────────────────────────

const CSV_PATH:      &str = "data/eth_1h.csv";   // CSV 文件路径
const SYMBOL:        &str = "ETHUSDT";            // 交易对
const INTERVAL:      &str = "1h";                 // K 线周期
const INITIAL_EQUITY: f64 = 10_000.0;             // 初始资金 USDT
const FEE_RATE:       f64 = 0.1;                  // 手续费率 %

// MA 交叉参数
const MA_FAST: usize = 9;
const MA_SLOW: usize = 21;

// ─────────────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN) // 回测时只显示警告，减少噪音
        .with_target(false)
        .init();

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  HashQuant 回测系统");
    println!("  交易对: {}  周期: {}", SYMBOL, INTERVAL);
    println!("  数据文件: {}", CSV_PATH);
    println!("  初始资金: {} USDT  手续费: {}%", INITIAL_EQUITY, FEE_RATE);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // ── 策略一：MA 交叉 ───────────────────────────────────────────────────────
    println!("\n[1/2] 运行策略: MA交叉 EMA{} × EMA{}", MA_FAST, MA_SLOW);
    run_backtest_ma().await;

    // ── 策略二：RSI 均值回归 ──────────────────────────────────────────────────
    println!("\n[2/2] 运行策略: RSI均值回归 (period=14, 超卖30, 超买70)");
    run_backtest_rsi().await;
}

async fn run_backtest_ma() {
    let config = BacktestConfig {
        initial_equity: INITIAL_EQUITY,
        fee_rate:       FEE_RATE,
        order_size_pct: 0.95,
        verbose:        false,
    };

    let mut sim   = Simulator::new(config);
    let mut strat = MaCrossStrategy::new(SYMBOL, MA_FAST, MA_SLOW);

    let mut feed = match CsvFeed::from_file(CSV_PATH, SYMBOL, INTERVAL) {
        Ok(f)  => f,
        Err(e) => {
            eprintln!("❌ 无法读取数据文件 {}: {}", CSV_PATH, e);
            eprintln!("   请先准备历史数据，格式：");
            eprintln!("   timestamp,open,high,low,close,volume");
            return;
        }
    };

    let result = sim.run(&mut feed, &mut strat, SYMBOL).await;
    print_report(&result.metrics, "MA交叉", SYMBOL);
}

async fn run_backtest_rsi() {
    let config = BacktestConfig {
        initial_equity: INITIAL_EQUITY,
        fee_rate:       FEE_RATE,
        order_size_pct: 0.95,
        verbose:        false,
    };

    let mut sim   = Simulator::new(config);
    let mut strat = RsiStrategy::default(SYMBOL);

    let mut feed = match CsvFeed::from_file(CSV_PATH, SYMBOL, INTERVAL) {
        Ok(f)  => f,
        Err(e) => {
            eprintln!("❌ 无法读取数据文件: {}", e);
            return;
        }
    };

    let result = sim.run(&mut feed, &mut strat, SYMBOL).await;
    print_report(&result.metrics, "RSI均值回归", SYMBOL);
}
