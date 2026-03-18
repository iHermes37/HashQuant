//! 运行方式：
//!   cargo run --example demo -p hq-exchange
//!
//! 配置方式：
//!   cp .env.example .env    （在项目根目录）
//!   编辑 .env，填入真实 Key

use hq_exchange::{
    AppConfig, BinanceClient, OkxClient, MockExchange, Exchange,
};
use hq_core::types::*;
use chrono::Utc;

async fn show(ex: &dyn Exchange, symbol: &str) {
    print!("  [{:<8}|{:?}] {} => ", ex.name(), ex.environment(), symbol);
    match ex.get_ticker(symbol).await {
        Ok(t)  => println!("bid={:.2}  ask={:.2}  last={:.2}  chg={:+.2}%",
                           t.bid, t.ask, t.last, t.price_change_pct),
        Err(e) => println!("ERROR: {}", e),
    }
}

#[tokio::main]
async fn main() {
    // ── 加载配置（自动读取 .env 文件）────────────────────────────────────────
    let cfg = AppConfig::from_env().expect("配置加载失败");
    cfg.print_summary();
    println!();

    // ── 1. Paper Trading：永远可用，不需要任何 Key ────────────────────────────
    println!("=== Paper Trading (MockExchange) ===");
    let mock = MockExchange::default_fees();
    mock.seed_balance("USDT", 50_000.0);
    mock.seed_balance("BTC",  1.0);
    mock.set_ticker(Ticker {
        symbol: "BTC-USDT".into(),
        bid: 29950.0, ask: 30050.0, last: 30000.0,
        volume_24h: 12345.0, price_change_pct: 1.23,
        timestamp: Utc::now(),
    });
    show(&mock, "BTC-USDT").await;

    // 限价挂单演示
    let req = PlaceOrderRequest::limit("BTC-USDT", OrderSide::Buy, 0.1, 29500.0);
    let order = mock.place_order(req).await.unwrap();
    println!("  挂单 {} => {:?}", order.order_id, order.status);

    // 触发成交
    mock.set_ticker(Ticker {
        symbol: "BTC-USDT".into(),
        bid: 29300.0, ask: 29400.0, last: 29350.0,
        volume_24h: 12345.0, price_change_pct: -2.0,
        timestamp: Utc::now(),
    });
    let filled = mock.get_order("", &order.order_id).await.unwrap();
    println!("  价格下跌后状态 => {:?}  avg_price={:?}", filled.status, filled.avg_fill_price);

    let acc = mock.get_account().await.unwrap();
    println!("  当前余额：");
    for b in &acc.balances {
        println!("    {} free={:.6}  locked={:.6}", b.asset, b.free, b.locked);
    }

    // ── 2. Binance 测试网（需要配置 BINANCE_TESTNET_KEY）─────────────────────
    println!("\n=== Binance Testnet ===");
    match cfg.require_binance_testnet() {
        Ok(c) => {
            let mut exchange_cfg = hq_exchange::ExchangeConfig::binance_testnet();
            exchange_cfg.proxy = cfg.proxy.clone(); // 注入代理
            show(&BinanceClient::with_config(&c.api_key, &c.secret, exchange_cfg), "BTCUSDT").await;
        }
        Err(e) => println!("  跳过 — {}", e),
    }

    // ── 3. OKX 模拟盘（需要配置 OKX_* ，与生产 Key 相同）───────────────────
    println!("\n=== OKX 模拟盘 ===");
    match cfg.require_okx() {
        Ok(c) => {
            let mut exchange_cfg = hq_exchange::ExchangeConfig::okx_testnet();
            exchange_cfg.proxy = cfg.proxy.clone();
            show(&OkxClient::with_config(&c.api_key, &c.secret, &c.passphrase, exchange_cfg), "BTC-USDT").await;
        }
        Err(e) => println!("  跳过 — {}", e),
    }

    // ── 4. Binance 生产网公开行情（无需 Key，走代理确保可达）────────────────
    println!("\n=== Binance 生产网（公开行情）===");
    let mut live_cfg = hq_exchange::ExchangeConfig::binance_live();
    live_cfg.proxy = cfg.proxy.clone();
    show(&BinanceClient::with_config("", "", live_cfg), "BTCUSDT").await;
}
