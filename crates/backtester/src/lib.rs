//! hq-backtester — HashQuant 回测系统
//!
//! # 快速开始
//!
//! ```rust,no_run
//! use hq_backtester::{Simulator, BacktestConfig};
//! use hq_backtester::reporter::print_report;
//! use hq_datafeed::CsvFeed;
//! use hq_strategy::MaCrossStrategy;
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut sim   = Simulator::new(BacktestConfig::default());
//!     let mut feed  = CsvFeed::from_file("data/eth_1h.csv", "ETHUSDT", "1h").unwrap();
//!     let mut strat = MaCrossStrategy::new("ETHUSDT", 9, 21);
//!
//!     let result = sim.run(&mut feed, &mut strat, "ETHUSDT").await;
//!     print_report(&result.metrics, strat.name(), "ETHUSDT");
//! }
//! ```

pub mod metrics;
pub mod reporter;
pub mod simulator;

pub use metrics::Metrics;
pub use simulator::{Simulator, BacktestConfig, BacktestResult};
