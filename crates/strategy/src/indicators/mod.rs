//! 技术指标库
//!
//! 所有指标都是**纯函数**，输入 `&[f64]`（收盘价序列），输出计算结果。
//! 不持有状态，方便在回测和实盘中复用。

pub mod ma;
pub mod rsi;
pub mod macd;
pub mod boll;

pub use ma::{sma, ema, sma_series, ema_series};
pub use rsi::{rsi, rsi_series};
pub use macd::{macd, MacdResult};
pub use boll::{bollinger_bands, BollResult};
