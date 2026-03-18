//! hq-strategy — HashQuant 策略引擎
//!
//! # 模块结构
//!
//! | 模块 | 内容 |
//! |------|------|
//! | `indicators` | SMA / EMA / RSI / MACD / 布林带 |
//! | `strategy` | `Strategy` trait + `Signal` 信号类型 |
//! | `strategies` | 内置策略：MA交叉、RSI均值回归 |
//! | `engine` | 策略引擎，连接 DataFeed → Strategy → Exchange |
//!
//! # 快速开始
//!
//! ```rust,no_run
//! use hq_strategy::strategies::MaCrossStrategy;
//! use hq_strategy::engine::Engine;
//! use hq_strategy::strategy::Strategy;
//! ```

pub mod error;
pub mod strategy;
pub mod indicators;
pub mod strategies;
pub mod engine;

pub use error::{StrategyError, Result};
pub use strategy::{Strategy, Signal, SignalKind};
pub use strategies::{MaCrossStrategy, RsiStrategy};
pub use engine::Engine;
