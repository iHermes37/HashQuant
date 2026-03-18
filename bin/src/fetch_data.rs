//! 历史数据下载工具
//!
//! 从 Binance 公开 API 拉取历史 K 线并保存为 CSV 文件。
//! 公开行情接口无需 API Key，无需实名认证。
//!
//! 运行方式：
//!   cargo run --bin fetch_data
//!
//! 默认下载：ETHUSDT 1小时线，最近 1000 根，保存到 data/eth_1h.csv

use std::fs;
use std::path::Path;
use chrono::{Utc, TimeZone};

// ── 配置（按需修改）──────────────────────────────────────────────────────────

const SYMBOL:   &str = "ETHUSDT";
const INTERVAL: &str = "1h";
const LIMIT:    u32  = 1000;       // 每次最多 1000 根（Binance 上限）
const OUT_FILE: &str = "data/eth_1h.csv";

// 如需下载更多数据，调大 PAGES，每页 1000 根
// 总根数 = LIMIT * PAGES
const PAGES: u32 = 3;              // 下载 3000 根

// 代理（中国大陆需要，留空则不用代理）
const PROXY: &str = "";  // 例如 "http://127.0.0.1:7890"

// ─────────────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  HashQuant 历史数据下载");
    println!("  交易对: {}  周期: {}  共 {} 根", SYMBOL, INTERVAL, LIMIT * PAGES);
    println!("  输出文件: {}", OUT_FILE);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // 创建输出目录
    if let Some(parent) = Path::new(OUT_FILE).parent() {
        fs::create_dir_all(parent).expect("无法创建 data/ 目录");
    }

    // 构建 HTTP 客户端（可选代理）
    let mut builder = reqwest::Client::builder();
    if !PROXY.is_empty() {
        builder = builder.proxy(reqwest::Proxy::all(PROXY).expect("代理地址无效"));
        println!("使用代理: {}", PROXY);
    }
    let client = builder.build().unwrap();

    // 收集所有 K 线
    let mut all_rows: Vec<String> = vec!["timestamp,open,high,low,close,volume".into()];
    let mut end_time: Option<u64> = None;

    for page in 0..PAGES {
        print!("  下载第 {}/{} 页...", page + 1, PAGES);

        let mut url = format!(
            "https://api.binance.com/api/v3/klines?symbol={}&interval={}&limit={}",
            SYMBOL, INTERVAL, LIMIT
        );
        if let Some(et) = end_time {
            url.push_str(&format!("&endTime={}", et));
        }

        let resp = match client.get(&url).send().await {
            Ok(r)  => r,
            Err(e) => {
                eprintln!("\n❌ 请求失败: {}", e);
                if e.to_string().contains("connect") {
                    eprintln!("   提示：网络连接失败，中国大陆请设置 PROXY 常量");
                    eprintln!("   例如：const PROXY: &str = \"http://127.0.0.1:7890\";");
                }
                std::process::exit(1);
            }
        };

        let text = resp.text().await.unwrap();
        let data: Vec<Vec<serde_json::Value>> = serde_json::from_str(&text)
            .expect("解析响应失败");

        if data.is_empty() {
            println!(" 无更多数据");
            break;
        }

        // 分页：下一次请求的 endTime = 本次第一根的 open_time - 1
        let first_open_time = data[0][0].as_u64().unwrap_or(0);
        end_time = if first_open_time > 0 { Some(first_open_time - 1) } else { None };

        let count = data.len();
        // 倒序插入（保证最终 CSV 按时间升序）
        let mut page_rows: Vec<String> = data.into_iter().map(|row| {
            let ts     = row[0].as_u64().unwrap_or(0);
            let open   = row[1].as_str().unwrap_or("0");
            let high   = row[2].as_str().unwrap_or("0");
            let low    = row[3].as_str().unwrap_or("0");
            let close  = row[4].as_str().unwrap_or("0");
            let volume = row[5].as_str().unwrap_or("0");
            format!("{},{},{},{},{},{}", ts, open, high, low, close, volume)
        }).collect();

        // 把本页数据插到已有数据前面（因为是倒序翻页）
        page_rows.extend(all_rows.drain(1..)); // 保留 header
        all_rows.extend(page_rows);

        let dt = Utc.timestamp_millis_opt(first_open_time as i64).unwrap();
        println!(" ✅ {} 根（最早: {}）", count, dt.format("%Y-%m-%d %H:%M"));

        // 避免触发限流
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }

    // 写入文件
    let total = all_rows.len() - 1; // 减去 header
    let content = all_rows.join("\n");
    fs::write(OUT_FILE, &content).expect("写入文件失败");

    println!("\n✅ 完成！共 {} 根 K 线 → {}", total, OUT_FILE);
    println!("   现在可以运行回测：cargo run --bin backtest");
}
