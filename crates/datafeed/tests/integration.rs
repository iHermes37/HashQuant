//! datafeed 集成测试
//! 运行：cargo test -p hq-datafeed

use std::sync::Arc;
use chrono::Utc;
use hq_core::types::{Ticker, OrderSide};
use hq_datafeed::{
    DataFeed, FeedEvent, Subscription,
    ExchangeFeed, CsvFeed, DatabaseFeed,
};
use hq_exchange::MockExchange;

// ── 工具函数 ──────────────────────────────────────────────────────────────────

fn mock_ticker(symbol: &str, price: f64) -> Ticker {
    Ticker {
        symbol: symbol.into(), bid: price - 5.0, ask: price + 5.0, last: price,
        volume_24h: 1000.0, price_change_pct: 0.0, timestamp: Utc::now(),
    }
}

const CSV_DATA: &str = "\
timestamp,open,high,low,close,volume
1609459200000,29000.0,29500.0,28800.0,29300.0,100.0
1609462800000,29300.0,30000.0,29200.0,29900.0,120.0
1609466400000,29900.0,30500.0,29800.0,30200.0,90.0
1609470000000,30200.0,30800.0,30100.0,30600.0,110.0
1609473600000,30600.0,31000.0,30400.0,30800.0,95.0
";

// ── ExchangeFeed 测试 ─────────────────────────────────────────────────────────

#[tokio::test]
async fn exchange_feed_returns_tick() {
    let mock = Arc::new(MockExchange::default_fees());
    mock.set_ticker(mock_ticker("BTC-USDT", 30000.0));

    let mut feed = ExchangeFeed::new(mock.clone(), 50)
        .subscribe(Subscription::ticker("BTC-USDT"));

    let event = feed.next().await.unwrap();
    assert!(matches!(event, FeedEvent::Tick(_)), "应返回 Tick 事件");

    if let FeedEvent::Tick(t) = event {
        assert_eq!(t.symbol, "BTC-USDT");
        assert_eq!(t.last, 30000.0);
    }
}

#[tokio::test]
async fn exchange_feed_multiple_symbols() {
    let mock = Arc::new(MockExchange::default_fees());
    mock.set_ticker(mock_ticker("BTC-USDT", 30000.0));
    mock.set_ticker(mock_ticker("ETH-USDT",  2000.0));

    let mut feed = ExchangeFeed::new(mock.clone(), 50)
        .subscribe(Subscription::ticker("BTC-USDT"))
        .subscribe(Subscription::ticker("ETH-USDT"));

    // 第一次 poll 应返回两个 symbol 的 tick
    let ev1 = feed.next().await.unwrap();
    let ev2 = feed.next().await.unwrap();

    let symbols: Vec<String> = [ev1, ev2].iter().filter_map(|e| {
        if let FeedEvent::Tick(t) = e { Some(t.symbol.clone()) } else { None }
    }).collect();

    assert!(symbols.contains(&"BTC-USDT".to_string()));
    assert!(symbols.contains(&"ETH-USDT".to_string()));
}

// ── CsvFeed 测试 ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn csv_feed_reads_all_candles() {
    let mut feed = CsvFeed::from_str(CSV_DATA, "BTC-USDT", "1h").unwrap();
    assert_eq!(feed.len(), 5);

    let mut count = 0;
    loop {
        match feed.next().await.unwrap() {
            FeedEvent::Candle { symbol, candle, .. } => {
                assert_eq!(symbol, "BTC-USDT");
                assert!(candle.high >= candle.low);
                assert!(candle.open > 0.0);
                count += 1;
            }
            FeedEvent::End => break,
            _ => {}
        }
    }
    assert_eq!(count, 5);
}

#[tokio::test]
async fn csv_feed_candles_in_order() {
    let mut feed = CsvFeed::from_str(CSV_DATA, "BTC-USDT", "1h").unwrap();
    let mut prev_time = None;

    loop {
        match feed.next().await.unwrap() {
            FeedEvent::Candle { candle, .. } => {
                if let Some(pt) = prev_time {
                    assert!(candle.open_time > pt, "K 线应按时间升序排列");
                }
                prev_time = Some(candle.open_time);
            }
            FeedEvent::End => break,
            _ => {}
        }
    }
}

#[tokio::test]
async fn csv_feed_reset_works() {
    let mut feed = CsvFeed::from_str(CSV_DATA, "BTC-USDT", "1h").unwrap();

    // 第一次读取第一根 K 线
    let first_close = if let FeedEvent::Candle { candle, .. } = feed.next().await.unwrap() {
        candle.close
    } else { panic!(); };

    // 读到结束
    loop {
        if matches!(feed.next().await.unwrap(), FeedEvent::End) { break; }
    }

    // Reset 后再读，应该得到相同的第一根
    feed.reset();
    let again_close = if let FeedEvent::Candle { candle, .. } = feed.next().await.unwrap() {
        candle.close
    } else { panic!(); };

    assert_eq!(first_close, again_close);
}

// ── DatabaseFeed 测试 ─────────────────────────────────────────────────────────

#[tokio::test]
async fn database_feed_from_candles() {
    use hq_core::types::Candle;

    let candles: Vec<Candle> = (0..3).map(|i| Candle {
        open_time: Utc::now() + chrono::Duration::hours(i),
        open:  100.0 + i as f64 * 10.0,
        high:  110.0 + i as f64 * 10.0,
        low:    90.0 + i as f64 * 10.0,
        close: 105.0 + i as f64 * 10.0,
        volume: 50.0,
    }).collect();

    let mut feed = DatabaseFeed::from_candles(candles, "ETH-USDT", "1h").unwrap();
    assert_eq!(feed.len(), 3);

    let mut count = 0;
    loop {
        match feed.next().await.unwrap() {
            FeedEvent::Candle { symbol, .. } => {
                assert_eq!(symbol, "ETH-USDT");
                count += 1;
            }
            FeedEvent::End => break,
            _ => {}
        }
    }
    assert_eq!(count, 3);
}

#[tokio::test]
async fn database_feed_empty_returns_error() {
    use hq_core::types::Candle;
    let result = DatabaseFeed::from_candles(vec![], "BTC-USDT", "1h");
    assert!(result.is_err());
}

// ── 端对端：ExchangeFeed → MockExchange → 验证数据流动 ────────────────────────

#[tokio::test]
async fn e2e_mock_exchange_to_feed() {
    let mock = Arc::new(MockExchange::default_fees());

    // 预先设置两个不同价格
    mock.set_ticker(mock_ticker("SOL-USDT", 100.0));

    let mut feed = ExchangeFeed::new(mock.clone(), 10)
        .subscribe(Subscription::ticker("SOL-USDT"));

    let ev = feed.next().await.unwrap();
    if let FeedEvent::Tick(t) = ev {
        assert_eq!(t.last, 100.0);
        assert_eq!(t.symbol, "SOL-USDT");
        assert!(t.bid < t.ask, "bid 必须小于 ask");
    } else {
        panic!("期望 Tick 事件");
    }
}

// ── 回测场景：CSV → 策略消费验证 ─────────────────────────────────────────────

#[tokio::test]
async fn backtest_scenario_csv_price_sequence() {
    let mut feed = CsvFeed::from_str(CSV_DATA, "BTC-USDT", "1h").unwrap();

    let mut closes = vec![];
    loop {
        match feed.next().await.unwrap() {
            FeedEvent::Candle { candle, .. } => closes.push(candle.close),
            FeedEvent::End => break,
            _ => {}
        }
    }

    // 验证价格序列正确
    assert_eq!(closes.len(), 5);
    assert_eq!(closes[0], 29300.0);
    assert_eq!(closes[4], 30800.0);
    // 整体趋势向上
    assert!(closes.last() > closes.first());
}
