//! CSV 历史数据源
//!
//! 读取标准 OHLCV CSV 文件，按时间顺序回放，用于回测。
//!
//! 支持两种列格式：
//!
//! **格式 A（Unix 毫秒时间戳）**
//! ```csv
//! timestamp,open,high,low,close,volume
//! 1609459200000,29000.0,29500.0,28800.0,29300.0,1234.5
//! ```
//!
//! **格式 B（ISO 8601 时间字符串）**
//! ```csv
//! datetime,open,high,low,close,volume
//! 2021-01-01T00:00:00Z,29000.0,29500.0,28800.0,29300.0,1234.5
//! ```

use async_trait::async_trait;
use std::path::Path;
use chrono::{DateTime, Utc, TimeZone};
use hq_core::types::Candle;
use crate::stream::{DataFeed, FeedEvent};
use crate::error::{FeedError, Result};

pub struct CsvFeed {
    symbol:   String,
    interval: String,
    candles:  Vec<Candle>,
    pos:      usize,
}

impl CsvFeed {
    /// 从文件路径加载
    pub fn from_file(
        path:     impl AsRef<Path>,
        symbol:   impl Into<String>,
        interval: impl Into<String>,
    ) -> Result<Self> {
        let candles = load_csv(path.as_ref())?;
        Ok(Self {
            symbol:   symbol.into(),
            interval: interval.into(),
            candles,
            pos: 0,
        })
    }

    /// 从内存字符串加载（测试用）
    pub fn from_str(
        data:     &str,
        symbol:   impl Into<String>,
        interval: impl Into<String>,
    ) -> Result<Self> {
        let candles = parse_csv(data.as_bytes())?;
        Ok(Self {
            symbol:   symbol.into(),
            interval: interval.into(),
            candles,
            pos: 0,
        })
    }

    pub fn len(&self) -> usize { self.candles.len() }
    pub fn is_empty(&self) -> bool { self.candles.is_empty() }

    /// 重置到开头（允许多次回测同一份数据）
    pub fn reset(&mut self) { self.pos = 0; }
}

#[async_trait]
impl DataFeed for CsvFeed {
    fn name(&self) -> &str { "CsvFeed" }

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

// ── CSV 解析 ──────────────────────────────────────────────────────────────────

fn load_csv(path: &Path) -> Result<Vec<Candle>> {
    let content = std::fs::read_to_string(path)?;
    parse_csv(content.as_bytes())
}

fn parse_csv(data: &[u8]) -> Result<Vec<Candle>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(data);

    let headers = rdr.headers()
        .map_err(|e| FeedError::Parse(e.to_string()))?
        .clone();

    // 检测时间列名
    let time_col = headers.iter().position(|h| {
        matches!(h.to_lowercase().as_str(), "timestamp" | "datetime" | "time" | "date" | "open_time")
    }).ok_or_else(|| FeedError::Parse("找不到时间列（timestamp/datetime/time）".into()))?;

    let col = |name: &str| -> Option<usize> {
        headers.iter().position(|h| h.to_lowercase() == name)
    };
    let open_col   = col("open").unwrap_or(1);
    let high_col   = col("high").unwrap_or(2);
    let low_col    = col("low").unwrap_or(3);
    let close_col  = col("close").unwrap_or(4);
    let volume_col = col("volume").unwrap_or(5);

    let mut candles = Vec::new();
    for result in rdr.records() {
        let rec = result.map_err(|e| FeedError::Csv(e))?;
        let get = |i: usize| -> f64 {
            rec.get(i).and_then(|s| s.trim().parse().ok()).unwrap_or(0.0)
        };

        let open_time = parse_time(rec.get(time_col).unwrap_or("0"))
            .ok_or_else(|| FeedError::Parse(format!(
                "无法解析时间: {}", rec.get(time_col).unwrap_or("")
            )))?;

        candles.push(Candle {
            open_time,
            open:   get(open_col),
            high:   get(high_col),
            low:    get(low_col),
            close:  get(close_col),
            volume: get(volume_col),
        });
    }

    // 按时间升序排列
    candles.sort_by_key(|c| c.open_time);
    Ok(candles)
}

fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    // 尝试 Unix 毫秒
    if let Ok(ms) = s.parse::<i64>() {
        if ms > 1_000_000_000_000 {
            return DateTime::from_timestamp_millis(ms);
        }
        // Unix 秒
        return DateTime::from_timestamp(ms, 0);
    }
    // 尝试 ISO 8601
    s.parse::<DateTime<Utc>>().ok()
}

// ── 测试 ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MS: &str = "\
timestamp,open,high,low,close,volume
1609459200000,29000.0,29500.0,28800.0,29300.0,100.0
1609462800000,29300.0,30000.0,29200.0,29900.0,120.0
1609466400000,29900.0,30500.0,29800.0,30200.0,90.0
";

    const SAMPLE_ISO: &str = "\
datetime,open,high,low,close,volume
2021-01-01T00:00:00Z,29000.0,29500.0,28800.0,29300.0,100.0
2021-01-01T01:00:00Z,29300.0,30000.0,29200.0,29900.0,120.0
2021-01-01T02:00:00Z,29900.0,30500.0,29800.0,30200.0,90.0
";

    #[tokio::test]
    async fn csv_ms_timestamp() {
        let mut feed = CsvFeed::from_str(SAMPLE_MS, "BTC-USDT", "1h").unwrap();
        assert_eq!(feed.len(), 3);

        let ev = feed.next().await.unwrap();
        if let FeedEvent::Candle { candle, symbol, .. } = ev {
            assert_eq!(symbol, "BTC-USDT");
            assert_eq!(candle.open, 29000.0);
            assert_eq!(candle.close, 29300.0);
        } else { panic!("expected Candle event"); }
    }

    #[tokio::test]
    async fn csv_iso_timestamp() {
        let mut feed = CsvFeed::from_str(SAMPLE_ISO, "ETH-USDT", "1h").unwrap();
        assert_eq!(feed.len(), 3);

        // 消费全部
        let mut count = 0;
        while let Some(ev) = feed.next().await {
            match ev {
                FeedEvent::Candle { .. } => count += 1,
                FeedEvent::End => break,
                _ => {}
            }
        }
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn csv_ends_with_end_event() {
        let mut feed = CsvFeed::from_str(SAMPLE_MS, "BTC-USDT", "1h").unwrap();
        for _ in 0..3 { feed.next().await; }
        let ev = feed.next().await.unwrap();
        assert!(matches!(ev, FeedEvent::End));
    }

    #[tokio::test]
    async fn csv_reset() {
        let mut feed = CsvFeed::from_str(SAMPLE_MS, "BTC-USDT", "1h").unwrap();
        // 读完
        for _ in 0..4 { feed.next().await; }
        // 重置后再读
        feed.reset();
        let ev = feed.next().await.unwrap();
        assert!(matches!(ev, FeedEvent::Candle { .. }));
    }

    #[test]
    fn sorted_by_time() {
        // 故意乱序输入
        let data = "\
timestamp,open,high,low,close,volume
1609466400000,29900.0,30500.0,29800.0,30200.0,90.0
1609459200000,29000.0,29500.0,28800.0,29300.0,100.0
1609462800000,29300.0,30000.0,29200.0,29900.0,120.0
";
        let feed = CsvFeed::from_str(data, "BTC-USDT", "1h").unwrap();
        let opens: Vec<f64> = feed.candles.iter().map(|c| c.open).collect();
        assert_eq!(opens, vec![29000.0, 29300.0, 29900.0]);
    }
}
