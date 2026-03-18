//! hq-datafeed — HashQuant 数据接入层
//!
//! # 三种数据源
//!
//! | 数据源 | 用途 | 类型 |
//! |--------|------|------|
//! | `ExchangeFeed` | 实时轮询交易所行情 | 无限流 |
//! | `CsvFeed` | 读取 CSV 历史 K 线 | 有限流（回测） |
//! | `DatabaseFeed` | 读取 SQLite 历史 K 线 | 有限流（回测） |
//!
//! # 快速开始
//!
//! ## 实时行情（接 MockExchange 测试）
//! ```rust,no_run
//! use std::sync::Arc;
//! use hq_datafeed::sources::{ExchangeFeed};
//! use hq_datafeed::stream::{DataFeed, FeedEvent, Subscription};
//! use hq_exchange::MockExchange;
//!
//! #[tokio::main]
//! async fn main() {
//!     let mock = Arc::new(MockExchange::default_fees());
//!     let mut feed = ExchangeFeed::new(mock, 1000)
//!         .subscribe(Subscription::ticker("BTC-USDT"));
//!
//!     while let Some(event) = feed.next().await {
//!         match event {
//!             FeedEvent::Tick(t) => println!("last={}", t.last),
//!             _ => {}
//!         }
//!     }
//! }
//! ```
//!
//! ## CSV 历史回测
//! ```rust,no_run
//! use hq_datafeed::sources::CsvFeed;
//! use hq_datafeed::stream::{DataFeed, FeedEvent};
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut feed = CsvFeed::from_file("data/btc_1h.csv", "BTC-USDT", "1h").unwrap();
//!     while let Some(event) = feed.next().await {
//!         match event {
//!             FeedEvent::Candle { candle, .. } => println!("close={}", candle.close),
//!             FeedEvent::End => break,
//!             _ => {}
//!         }
//!     }
//! }
//! ```

pub mod error;
pub mod stream;
pub mod sources;
pub mod storage;

pub use error::{FeedError, Result};
pub use stream::{DataFeed, FeedEvent, Subscription};
pub use sources::{ExchangeFeed, CandleFeed, CsvFeed, DatabaseFeed};
