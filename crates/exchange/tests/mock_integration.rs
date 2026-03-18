//! 集成测试 — 全部基于 MockExchange，零网络依赖
//! 运行：cargo test -p hq-exchange

use hq_exchange::{MockExchange, Exchange};
use hq_core::types::*;
use chrono::Utc;

fn ticker(symbol: &str, bid: f64, ask: f64) -> Ticker {
    Ticker { symbol: symbol.into(), bid, ask, last: (bid + ask) / 2.0,
             volume_24h: 500.0, price_change_pct: 0.0, timestamp: Utc::now() }
}

#[tokio::test]
async fn full_buy_sell_cycle() {
    let ex = MockExchange::default_fees();
    ex.seed_balance("USDT", 50000.0);
    ex.set_ticker(ticker("BTC-USDT", 29900.0, 30000.0));

    let buy = ex.place_order(PlaceOrderRequest::market("BTC-USDT", OrderSide::Buy, 0.5)).await.unwrap();
    assert_eq!(buy.status, OrderStatus::Filled);

    ex.set_ticker(ticker("BTC-USDT", 32000.0, 32100.0));
    let sell = ex.place_order(PlaceOrderRequest::market("BTC-USDT", OrderSide::Sell, 0.5)).await.unwrap();
    assert_eq!(sell.status, OrderStatus::Filled);

    let trades = ex.all_trades();
    assert_eq!(trades.len(), 2);
    // 卖出净收入 > 买入成本 → 盈利
    let buy_cost  = trades[0].price * trades[0].qty + trades[0].fee;
    let sell_recv = trades[1].price * trades[1].qty - trades[1].fee;
    assert!(sell_recv > buy_cost);
}

#[tokio::test]
async fn limit_order_lifecycle() {
    let ex = MockExchange::default_fees();
    ex.seed_balance("USDT", 100000.0);
    ex.set_ticker(ticker("ETH-USDT", 1900.0, 1910.0));

    let o = ex.place_order(PlaceOrderRequest::limit("ETH-USDT", OrderSide::Buy, 10.0, 1850.0)).await.unwrap();
    assert_eq!(o.status, OrderStatus::New);

    let open = ex.get_open_orders(Some("ETH-USDT")).await.unwrap();
    assert_eq!(open.len(), 1);

    ex.set_ticker(ticker("ETH-USDT", 1830.0, 1840.0));
    let filled = ex.get_order("", &o.order_id).await.unwrap();
    assert_eq!(filled.status, OrderStatus::Filled);

    let open_after = ex.get_open_orders(Some("ETH-USDT")).await.unwrap();
    assert!(open_after.is_empty());
}

#[tokio::test]
async fn balance_enforcement() {
    let ex = MockExchange::default_fees();
    ex.seed_balance("USDT", 1000.0);
    ex.set_ticker(ticker("BTC-USDT", 29900.0, 30000.0));

    let result = ex.place_order(
        PlaceOrderRequest::limit("BTC-USDT", OrderSide::Buy, 1.0, 30000.0)
    ).await;
    assert!(matches!(result, Err(hq_core::error::CoreError::InsufficientBalance { .. })));

    // 失败下单不改变余额
    let acc = ex.get_account().await.unwrap();
    let usdt = acc.balances.iter().find(|b| b.asset == "USDT").unwrap();
    assert_eq!(usdt.free, 1000.0);
}

#[tokio::test]
async fn simulated_backtest() {
    let ex = MockExchange::default_fees();
    ex.seed_balance("USDT", 10000.0);

    // 买入时 ask=10200，卖出时 bid=10700，bid > ask → 扣除手续费后仍盈利
    let prices = vec![
        (9900.0,  10000.0),
        (9950.0,  10050.0),
        (10100.0, 10200.0), // i=2 买入，成交价 = ask = 10200
        (10300.0, 10400.0),
        (10600.0, 10700.0),
        (10800.0, 10900.0), // i=5
        (10700.0, 10800.0), // i=6 卖出，成交价 = bid = 10700 > 10200 ✓
        (10500.0, 10600.0),
    ];

    for (i, (bid, ask)) in prices.iter().enumerate() {
        ex.set_ticker(ticker("BTC-USDT", *bid, *ask));
        if i == 2 {
            let o = ex.place_order(PlaceOrderRequest::market("BTC-USDT", OrderSide::Buy, 0.1)).await.unwrap();
            assert_eq!(o.status, OrderStatus::Filled);
        }
        if i == 6 {
            let o = ex.place_order(PlaceOrderRequest::market("BTC-USDT", OrderSide::Sell, 0.1)).await.unwrap();
            assert_eq!(o.status, OrderStatus::Filled);
        }
    }

    let trades = ex.all_trades();
    assert_eq!(trades.len(), 2);
    assert!(trades[1].price > trades[0].price, "策略应盈利: sell={} buy={}", trades[1].price, trades[0].price);
}
#[tokio::test]
async fn environment_flags() {
    use hq_exchange::{BinanceClient, OkxClient, CoinbaseClient};
    use hq_core::types::Environment;

    assert_eq!(*BinanceClient::new("k", "s").environment(),      Environment::Live);
    assert_eq!(*BinanceClient::testnet("k", "s").environment(),  Environment::Testnet);
    assert_eq!(*OkxClient::new("k","s","p").environment(),       Environment::Live);
    assert_eq!(*OkxClient::testnet("k","s","p").environment(),   Environment::Testnet);
    assert_eq!(*CoinbaseClient::new("k","s").environment(),      Environment::Live);
    assert_eq!(*CoinbaseClient::testnet("k","s").environment(),  Environment::Testnet);
    assert_eq!(*MockExchange::default_fees().environment(),      Environment::Paper);
}

#[tokio::test]
async fn concurrent_orders() {
    use std::sync::Arc;
    use futures::future::join_all;

    let ex = Arc::new(MockExchange::default_fees());
    ex.seed_balance("USDT", 1_000_000.0);
    ex.set_ticker(ticker("BTC-USDT", 29900.0, 30000.0));

    let handles: Vec<_> = (0..20).map(|_| {
        let ex = ex.clone();
        tokio::spawn(async move {
            ex.place_order(PlaceOrderRequest::market("BTC-USDT", OrderSide::Buy, 0.001))
              .await.unwrap()
        })
    }).collect();

    let results = join_all(handles).await;
    assert!(results.iter().all(|r| r.as_ref().map(|o| o.status == OrderStatus::Filled).unwrap_or(false)));
    assert_eq!(ex.all_trades().len(), 20);
}