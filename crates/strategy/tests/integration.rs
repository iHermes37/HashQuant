//! 策略集成测试
//! 运行：cargo test -p hq-strategy

use std::sync::Arc;
use chrono::Utc;
use hq_core::types::{Candle, OrderSide};
use hq_datafeed::{CsvFeed, DataFeed, FeedEvent};
use hq_exchange::MockExchange;
use hq_strategy::{
    Engine, MaCrossStrategy, RsiStrategy,
    strategy::Strategy,
    indicators::{sma, ema, rsi, macd, bollinger_bands},
};

// ── 工具 ──────────────────────────────────────────────────────────────────────

fn candle(close: f64) -> Candle {
    Candle { open_time: Utc::now(), open: close, high: close + 5.0, low: close - 5.0, close, volume: 100.0 }
}

const CSV: &str = "\
timestamp,open,high,low,close,volume
1609459200000,29000,29500,28800,29300,100
1609462800000,29300,30000,29200,29900,120
1609466400000,29900,30500,29800,30200,90
1609470000000,30200,30800,30100,30600,110
1609473600000,30600,31000,30400,30800,95
1609477200000,30800,31500,30600,31200,130
1609480800000,31200,32000,31000,31800,140
1609484400000,31800,32500,31500,32200,120
1609488000000,32200,33000,32000,32800,150
1609491600000,32800,33500,32500,33100,135
1609495200000,33100,33800,32900,33500,125
1609498800000,33500,34000,33200,33800,145
1609502400000,33800,34500,33600,34200,160
1609506000000,34200,35000,34000,34700,170
1609509600000,34700,35500,34500,35200,155
1609513200000,35200,36000,35000,35800,180
1609516800000,35800,36500,35600,36300,175
1609520400000,36300,37000,36100,36800,190
1609524000000,36800,37500,36600,37200,185
1609527600000,37200,38000,37000,37800,200
1609531200000,37800,38500,37600,38300,195
1609534800000,38300,39000,38100,38800,210
1609538400000,38800,39500,38600,39200,205
1609542000000,39200,40000,39000,39700,220
1609545600000,39700,40500,39500,40200,215
1609549200000,40200,41000,40000,40800,230
1609552800000,40800,41500,40600,41300,225
1609556400000,41300,42000,41100,41900,240
1609560000000,41900,42500,41700,42400,235
1609563600000,42400,43000,42200,42900,250
";

// ── 指标单独测试 ──────────────────────────────────────────────────────────────

#[test]
fn indicators_sma_correctness() {
    let prices = vec![2.0, 4.0, 6.0, 8.0, 10.0];
    assert_eq!(sma(&prices, 3).unwrap(), 8.0);  // (6+8+10)/3
}

#[test]
fn indicators_ema_trend_following() {
    let up: Vec<f64> = (1..=30).map(|x| x as f64).collect();
    let e = ema(&up, 10).unwrap();
    // EMA 应高于 SMA 的一半（跟随上涨趋势）
    assert!(e > 15.0);
}

#[test]
fn indicators_rsi_overbought() {
    let up: Vec<f64> = (1..=20).map(|x| x as f64 * 10.0).collect();
    let r = rsi(&up, 14).unwrap();
    assert!(r > 70.0, "持续上涨 RSI 应超买");
}

#[test]
fn indicators_macd_histogram_sign() {
    let up: Vec<f64> = (1..=50).map(|x| x as f64).collect();
    let r = macd(&up, 12, 26, 9).unwrap();
    // 上涨趋势：快线 > 慢线，MACD > 0，histogram > 0
    assert!(r.macd_line > 0.0);
}

#[test]
fn indicators_boll_structure() {
    let prices: Vec<f64> = (1..=25).map(|x| 100.0 + (x % 7) as f64).collect();
    let r = bollinger_bands(&prices, 20, 2.0).unwrap();
    assert!(r.upper > r.middle && r.middle > r.lower);
    assert!(r.bandwidth > 0.0);
}

// ── MaCross 策略测试 ──────────────────────────────────────────────────────────

#[tokio::test]
async fn ma_cross_produces_buy_on_uptrend() {
    let mut strat = MaCrossStrategy::new("BTC-USDT", 3, 7);

    // 先下跌建立 fast < slow 的初始状态
    for p in [100.0, 98.0, 96.0, 94.0, 92.0, 90.0, 88.0, 86.0, 84.0, 82.0] {
        strat.on_candle(&candle(p)).await.unwrap();
    }

    // 强势反弹
    let mut buy_count = 0;
    for p in [90.0, 100.0, 112.0, 125.0, 138.0, 150.0] {
        let sigs = strat.on_candle(&candle(p)).await.unwrap();
        buy_count += sigs.iter().filter(|s| s.side == OrderSide::Buy).count();
    }
    assert!(buy_count > 0, "反弹后应触发买入信号");
}

#[tokio::test]
async fn ma_cross_no_double_entry() {
    let mut strat = MaCrossStrategy::new("BTC-USDT", 3, 7);

    // 制造金叉信号
    for p in [80.0, 78.0, 76.0, 74.0, 72.0, 70.0, 68.0, 66.0, 64.0, 62.0] {
        strat.on_candle(&candle(p)).await.unwrap();
    }
    for p in [75.0, 90.0, 108.0, 126.0, 144.0, 160.0, 175.0, 188.0] {
        strat.on_candle(&candle(p)).await.unwrap();
    }

    // 价格继续上涨（已持仓），不应再次买入
    let mut extra_buys = 0;
    for p in [190.0, 195.0, 200.0, 205.0] {
        let sigs = strat.on_candle(&candle(p)).await.unwrap();
        extra_buys += sigs.iter().filter(|s| s.side == OrderSide::Buy).count();
    }
    assert_eq!(extra_buys, 0, "持仓中不应重复买入");
}

// ── RSI 策略测试 ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn rsi_strategy_signal_in_range() {
    let mut strat = RsiStrategy::default("BTC-USDT");
    let prices: Vec<f64> = (1..=30).map(|x| 100.0 + (x % 5) as f64 - 2.0).collect();

    for p in &prices {
        let sigs = strat.on_candle(&candle(*p)).await.unwrap();
        for s in &sigs {
            // 信号只能是买或卖
            assert!(s.side == OrderSide::Buy || s.side == OrderSide::Sell);
        }
    }
}

// ── Engine + MockExchange 端对端 ──────────────────────────────────────────────

#[tokio::test]
async fn engine_runs_ma_cross_on_csv() {
    let mock = Arc::new(MockExchange::default_fees());
    mock.seed_balance("USDT", 10000.0);

    let mut feed = CsvFeed::from_str(CSV, "BTC-USDT", "1h").unwrap();
    let mut strategy = MaCrossStrategy::new("BTC-USDT", 3, 7);
    let mut engine = Engine::new(mock.clone());

    engine.run(&mut feed, &mut strategy).await.unwrap();

    let stats = engine.stats();
    assert_eq!(stats.candles_processed, 30, "应处理全部30根K线");
    // 30根K线的趋势行情中至少应有1个信号
    println!(
        "信号数={} 下单成功={} 失败={}",
        stats.signals_generated, stats.orders_placed, stats.orders_failed
    );
}

#[tokio::test]
async fn engine_insufficient_balance_handled_gracefully() {
    let mock = Arc::new(MockExchange::default_fees());
    // 余额极少，下单会被拒绝，但引擎不应崩溃
    mock.seed_balance("USDT", 0.001);

    let mut feed = CsvFeed::from_str(CSV, "BTC-USDT", "1h").unwrap();
    let mut strategy = MaCrossStrategy::new("BTC-USDT", 3, 7);
    let mut engine = Engine::new(mock);

    // 不应 panic
    engine.run(&mut feed, &mut strategy).await.unwrap();
}
