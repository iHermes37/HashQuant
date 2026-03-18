//! 数据落库模块
//!
//! 把从交易所拉取的实时 K 线持久化到 SQLite，
//! 后续回测可直接从数据库读取，不需要重复请求 API。

use hq_core::types::Candle;
use crate::error::{FeedError, Result};

/// K 线存储接口
pub trait CandleStorage: Send + Sync {
    fn save(&self, symbol: &str, interval: &str, candles: &[Candle]) -> Result<()>;
    fn load(&self, symbol: &str, interval: &str) -> Result<Vec<Candle>>;
    fn count(&self, symbol: &str, interval: &str) -> Result<usize>;
}

// ── SQLite 实现 ───────────────────────────────────────────────────────────────

#[cfg(feature = "sqlite")]
pub mod sqlite {
    use super::*;
    use rusqlite::{Connection, params};
    use std::sync::Mutex;
    use chrono::DateTime;

    pub struct SqliteStorage {
        conn: Mutex<Connection>,
    }

    impl SqliteStorage {
        /// 打开（或创建）SQLite 数据库，自动建表
        pub fn open(path: &str) -> Result<Self> {
            let conn = Connection::open(path)
                .map_err(|e| FeedError::Database(e.to_string()))?;

            conn.execute_batch("
                CREATE TABLE IF NOT EXISTS candles (
                    symbol    TEXT    NOT NULL,
                    interval  TEXT    NOT NULL,
                    open_time INTEGER NOT NULL,
                    open      REAL    NOT NULL,
                    high      REAL    NOT NULL,
                    low       REAL    NOT NULL,
                    close     REAL    NOT NULL,
                    volume    REAL    NOT NULL,
                    PRIMARY KEY (symbol, interval, open_time)
                );
                CREATE INDEX IF NOT EXISTS idx_candles_symbol
                    ON candles(symbol, interval, open_time);
            ").map_err(|e| FeedError::Database(e.to_string()))?;

            Ok(Self { conn: Mutex::new(conn) })
        }

        /// 内存数据库（测试用）
        pub fn in_memory() -> Result<Self> {
            Self::open(":memory:")
        }
    }

    impl CandleStorage for SqliteStorage {
        fn save(&self, symbol: &str, interval: &str, candles: &[Candle]) -> Result<()> {
            let conn = self.conn.lock().unwrap();
            let tx = conn.unchecked_transaction()
                .map_err(|e| FeedError::Database(e.to_string()))?;

            for c in candles {
                tx.execute(
                    "INSERT OR REPLACE INTO candles
                     (symbol, interval, open_time, open, high, low, close, volume)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        symbol, interval,
                        c.open_time.timestamp_millis(),
                        c.open, c.high, c.low, c.close, c.volume,
                    ],
                ).map_err(|e| FeedError::Database(e.to_string()))?;
            }

            tx.commit().map_err(|e| FeedError::Database(e.to_string()))?;
            Ok(())
        }

        fn load(&self, symbol: &str, interval: &str) -> Result<Vec<Candle>> {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT open_time, open, high, low, close, volume
                 FROM candles
                 WHERE symbol = ?1 AND interval = ?2
                 ORDER BY open_time ASC"
            ).map_err(|e| FeedError::Database(e.to_string()))?;

            let rows: std::result::Result<Vec<Candle>, _> = stmt.query_map(
                params![symbol, interval],
                |row| {
                    let ms: i64 = row.get(0)?;
                    Ok(Candle {
                        open_time: DateTime::from_timestamp_millis(ms).unwrap_or_default(),
                        open:   row.get(1)?,
                        high:   row.get(2)?,
                        low:    row.get(3)?,
                        close:  row.get(4)?,
                        volume: row.get(5)?,
                    })
                }
            ).map_err(|e| FeedError::Database(e.to_string()))?
             .collect();

            rows.map_err(|e| FeedError::Database(e.to_string()))
        }

        fn count(&self, symbol: &str, interval: &str) -> Result<usize> {
            let conn = self.conn.lock().unwrap();
            let n: i64 = conn.query_row(
                "SELECT COUNT(*) FROM candles WHERE symbol = ?1 AND interval = ?2",
                params![symbol, interval],
                |row| row.get(0),
            ).map_err(|e| FeedError::Database(e.to_string()))?;
            Ok(n as usize)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use chrono::Utc;

        fn make_candle(close: f64) -> Candle {
            Candle {
                open_time: Utc::now(),
                open: close - 10.0, high: close + 20.0,
                low: close - 20.0,  close, volume: 100.0,
            }
        }

        #[test]
        fn save_and_load() {
            let db = SqliteStorage::in_memory().unwrap();
            let candles = vec![make_candle(100.0), make_candle(110.0)];
            db.save("BTC-USDT", "1h", &candles).unwrap();
            let loaded = db.load("BTC-USDT", "1h").unwrap();
            assert_eq!(loaded.len(), 2);
            assert_eq!(loaded[0].close, 100.0);
        }

        #[test]
        fn upsert_deduplicates() {
            let db = SqliteStorage::in_memory().unwrap();
            let c = make_candle(200.0);
            db.save("BTC-USDT", "1h", &[c.clone()]).unwrap();
            db.save("BTC-USDT", "1h", &[c]).unwrap(); // 重复插入
            assert_eq!(db.count("BTC-USDT", "1h").unwrap(), 1);
        }

        #[test]
        fn count_empty() {
            let db = SqliteStorage::in_memory().unwrap();
            assert_eq!(db.count("ETH-USDT", "1h").unwrap(), 0);
        }
    }
}
