//! 数据库历史数据源（SQLite）
//!
//! 功能：
//! - 从 SQLite 读取历史 K 线，按时间顺序回放
//! - 配合 `storage::sqlite` 模块写入数据
//!
//! 表结构（由 `SqliteStorage::init()` 自动创建）：
//! ```sql
//! CREATE TABLE candles (
//!     symbol    TEXT    NOT NULL,
//!     interval  TEXT    NOT NULL,
//!     open_time INTEGER NOT NULL,   -- Unix 毫秒
//!     open      REAL    NOT NULL,
//!     high      REAL    NOT NULL,
//!     low       REAL    NOT NULL,
//!     close     REAL    NOT NULL,
//!     volume    REAL    NOT NULL,
//!     PRIMARY KEY (symbol, interval, open_time)
//! );
//! ```

use async_trait::async_trait;
use chrono::DateTime;
use hq_core::types::Candle;
use crate::stream::{DataFeed, FeedEvent};
use crate::error::{FeedError, Result};

/// 从数据库加载 K 线并回放的数据源
pub struct DatabaseFeed {
    symbol:   String,
    interval: String,
    candles:  Vec<Candle>,
    pos:      usize,
}

impl DatabaseFeed {
    /// 从 SQLite 文件加载指定 symbol + interval 的全部 K 线
    #[cfg(feature = "sqlite")]
    pub fn from_sqlite(
        db_path:  &str,
        symbol:   impl Into<String>,
        interval: impl Into<String>,
    ) -> Result<Self> {
        use rusqlite::{Connection, params};

        let sym = symbol.into();
        let ivl = interval.into();
        let conn = Connection::open(db_path)
            .map_err(|e| FeedError::Database(e.to_string()))?;

        let mut stmt = conn.prepare(
            "SELECT open_time, open, high, low, close, volume
             FROM candles
             WHERE symbol = ?1 AND interval = ?2
             ORDER BY open_time ASC"
        ).map_err(|e| FeedError::Database(e.to_string()))?;

        let candles: std::result::Result<Vec<Candle>, _> = stmt.query_map(
            params![sym, ivl],
            |row| {
                let ms: i64 = row.get(0)?;
                Ok(Candle {
                    open_time: DateTime::from_timestamp_millis(ms)
                        .unwrap_or_default(),
                    open:   row.get(1)?,
                    high:   row.get(2)?,
                    low:    row.get(3)?,
                    close:  row.get(4)?,
                    volume: row.get(5)?,
                })
            }
        ).map_err(|e| FeedError::Database(e.to_string()))?
         .collect();

        let candles = candles.map_err(|e| FeedError::Database(e.to_string()))?;

        if candles.is_empty() {
            return Err(FeedError::NoData { symbol: sym });
        }

        Ok(Self { symbol: sym, interval: ivl, candles, pos: 0 })
    }

    /// 直接从已有 K 线列表构造（测试 / 其他数据库后端）
    pub fn from_candles(
        candles:  Vec<Candle>,
        symbol:   impl Into<String>,
        interval: impl Into<String>,
    ) -> Result<Self> {
        let sym = symbol.into();
        if candles.is_empty() {
            return Err(FeedError::NoData { symbol: sym });
        }
        Ok(Self { symbol: sym, interval: interval.into(), candles, pos: 0 })
    }

    pub fn len(&self) -> usize { self.candles.len() }
    pub fn reset(&mut self) { self.pos = 0; }
}

#[async_trait]
impl DataFeed for DatabaseFeed {
    fn name(&self) -> &str { "DatabaseFeed" }

    async fn next(&mut self) -> Option<FeedEvent> {
        if self.pos >= self.candles.len() {
            return Some(FeedEvent::End);
        }
        let candle = self.candles[self.pos].clone();
        self.pos += 1;
        Some(FeedEvent::Candle {
            symbol:   self.symbol.clone(),
            interval: self.interval.clone(),
            candle,
        })
    }
}
