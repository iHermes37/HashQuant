//! Binance 测试网实盘运行入口
//!
//! 运行方式：
//!   cargo run -p hq-bin --bin testnet
//!
//! 配置（.env 文件）：
//!   BINANCE_TESTNET_KEY=...
//!   BINANCE_TESTNET_SECRET=...
//!   HTTPS_PROXY=http://127.0.0.1:7890   （中国大陆需要）
//!
//! 测试网 Key 申请：https://testnet.binance.vision

use std::sync::Arc;
use tracing::{info, warn, error};
use tracing_subscriber::EnvFilter;
use hq_exchange::{BinanceClient, ExchangeConfig, Exchange};
use hq_exchange::config::AppConfig;
use hq_datafeed::CandleFeed;
use hq_datafeed::stream::{DataFeed, FeedEvent};
use hq_strategy::{MaCrossStrategy, strategy::Strategy};
use hq_core::types::{PlaceOrderRequest, OrderSide};

// ── 参数配置 ──────────────────────────────────────────────────────────────────

const SYMBOL:      &str = "ETHUSDT";
const INTERVAL:    &str = "1m";      // K 线周期：1m/3m/5m/15m/1h
const FAST_PERIOD: usize = 9;        // EMA 快线周期
const SLOW_PERIOD: usize = 21;       // EMA 慢线周期
const WARMUP_BARS: u32   = 50;       // 预热历史 K 线数（≥ 慢线周期 * 2）
const ORDER_QTY:   f64   = 0.01;     // 每次下单数量（ETH）

// ─────────────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    // ── 初始化日志 ────────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"))
        )
        .with_target(false)
        .with_thread_ids(false)
        .init();

    // ── 加载配置 ──────────────────────────────────────────────────────────────
    let cfg = AppConfig::from_env().expect("配置加载失败，请检查 .env 文件");
    cfg.print_summary();

    let bn_cfg = match cfg.require_binance_testnet() {
        Ok(c) => c,
        Err(e) => {
            error!("{}", e);
            error!("请在 .env 中配置 BINANCE_TESTNET_KEY 和 BINANCE_TESTNET_SECRET");
            error!("测试网 Key 申请：https://testnet.binance.vision");
            std::process::exit(1);
        }
    };

    // ── 创建交易所客户端（测试网 + 代理）────────────────────────────────────
    let mut ex_cfg = ExchangeConfig::binance_testnet();
    ex_cfg.proxy   = cfg.proxy.clone();

    let exchange = Arc::new(
        BinanceClient::with_config(&bn_cfg.api_key, &bn_cfg.secret, ex_cfg)
    );

    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("  HashQuant 测试网实盘");
    info!("  交易所: Binance Testnet");
    info!("  交易对: {}", SYMBOL);
    info!("  K线周期: {}", INTERVAL);
    info!("  策略: MA交叉 EMA{} × EMA{}", FAST_PERIOD, SLOW_PERIOD);
    info!("  下单数量: {} ETH", ORDER_QTY);
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // ── 查看账户余额 ──────────────────────────────────────────────────────────
    match exchange.get_account().await {
        Ok(acc) => {
            info!("当前账户余额：");
            for b in &acc.balances {
                if b.free > 0.0 || b.locked > 0.0 {
                    info!("  {} free={:.6}  locked={:.6}", b.asset, b.free, b.locked);
                }
            }
        }
        Err(e) => warn!("查询账户失败: {}", e),
    }

    // ── 初始化 K 线数据源（含预热）───────────────────────────────────────────
    info!("正在拉取 {} 根历史K线预热指标...", WARMUP_BARS);
    let mut feed = match CandleFeed::new(
        exchange.clone(),
        SYMBOL,
        INTERVAL,
        WARMUP_BARS,
    ).await {
        Ok(f) => f,
        Err(e) => {
            error!("初始化 K 线数据源失败: {}", e);
            std::process::exit(1);
        }
    };

    // ── 初始化策略 ────────────────────────────────────────────────────────────
    let mut strategy = MaCrossStrategy::new(SYMBOL, FAST_PERIOD, SLOW_PERIOD);
    info!("策略初始化完成，开始运行...");
    info!("按 Ctrl+C 停止");
    info!("─────────────────────────────────────────────────");

    // ── 主循环 ────────────────────────────────────────────────────────────────
    let mut candle_count  = 0u64;
    let mut signal_count  = 0u64;
    let mut order_count   = 0u64;

    loop {
        let event = match feed.next().await {
            Some(e) => e,
            None    => break,
        };

        match event {
            FeedEvent::Candle { candle, .. } => {
                candle_count += 1;
                info!(
                    "[K线 #{}] {} open={:.2} high={:.2} low={:.2} close={:.2} vol={:.2}",
                    candle_count,
                    candle.open_time.format("%m-%d %H:%M"),
                    candle.open, candle.high, candle.low, candle.close, candle.volume
                );

                // 策略计算
                let signals = match strategy.on_candle(&candle).await {
                    Ok(s)  => s,
                    Err(e) => { warn!("策略计算失败: {}", e); continue; }
                };

                // 执行信号
                for sig in &signals {
                    signal_count += 1;
                    info!(
                        "▶ 信号 #{}: {:?} {}  reason={}",
                        signal_count, sig.side, sig.symbol, sig.reason
                    );

                    let req = match sig.side {
                        OrderSide::Buy  => PlaceOrderRequest::market(SYMBOL, OrderSide::Buy,  ORDER_QTY),
                        OrderSide::Sell => PlaceOrderRequest::market(SYMBOL, OrderSide::Sell, ORDER_QTY),
                    };

                    match exchange.place_order(req).await {
                        Ok(order) => {
                            order_count += 1;
                            info!(
                                "✅ 下单成功 order_id={} side={:?} status={:?}",
                                order.order_id, order.side, order.status
                            );
                        }
                        Err(e) => {
                            warn!("❌ 下单失败: {}", e);
                        }
                    }
                }

                if signals.is_empty() {
                    info!("  → 无信号");
                }
            }
            FeedEvent::End => {
                info!("数据流结束");
                break;
            }
            _ => {}
        }
    }

    // ── 运行摘要 ──────────────────────────────────────────────────────────────
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("运行结束 | 处理K线={} 信号={} 下单={}", candle_count, signal_count, order_count);

    if let Ok(acc) = exchange.get_account().await {
        info!("最终账户余额：");
        for b in &acc.balances {
            if b.free > 0.0 || b.locked > 0.0 {
                info!("  {} free={:.6}  locked={:.6}", b.asset, b.free, b.locked);
            }
        }
    }
}
