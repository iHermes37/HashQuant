use std::env;
use hq_core::error::CoreError;

// ─── 各交易所配置结构 ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BinanceConfig {
    pub api_key: String,
    pub secret:  String,
}

#[derive(Debug, Clone)]
pub struct OkxConfig {
    pub api_key:    String,
    pub secret:     String,
    pub passphrase: String,
}

#[derive(Debug, Clone)]
pub struct CoinbaseConfig {
    pub api_key: String,
    pub secret:  String,
}

#[derive(Debug, Clone)]
pub struct PolymarketConfig {
    pub api_key: String,
    pub secret:  String,
}

// ─── 整体配置 ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Binance 生产网
    pub binance:          Option<BinanceConfig>,
    /// Binance 测试网（testnet.binance.vision 单独申请的 Key）
    pub binance_testnet:  Option<BinanceConfig>,
    /// OKX 生产网
    pub okx:              Option<OkxConfig>,
    /// Coinbase 生产网
    pub coinbase:         Option<CoinbaseConfig>,
    /// Coinbase 沙箱
    pub coinbase_testnet: Option<CoinbaseConfig>,
    /// Polymarket
    pub polymarket:       Option<PolymarketConfig>,
    /// 全局代理，读取自 HTTPS_PROXY 环境变量，格式：http://127.0.0.1:7890
    pub proxy:            Option<String>,
}

// ─── 加载逻辑 ─────────────────────────────────────────────────────────────────

fn optional_pair(k: &str, s: &str) -> Option<(String, String)> {
    match (env::var(k), env::var(s)) {
        (Ok(kv), Ok(sv)) if !kv.is_empty() && !sv.is_empty() => Some((kv, sv)),
        _ => None,
    }
}

fn optional_triple(k: &str, s: &str, p: &str) -> Option<(String, String, String)> {
    match (env::var(k), env::var(s), env::var(p)) {
        (Ok(kv), Ok(sv), Ok(pv)) if !kv.is_empty() && !sv.is_empty() && !pv.is_empty()
            => Some((kv, sv, pv)),
        _ => None,
    }
}

impl AppConfig {
    /// 从 .env 文件或系统环境变量加载配置。
    ///
    /// 调用时会自动执行 `dotenv::dotenv()`；
    /// 未填写的交易所配置为 `None`，不会报错。
    pub fn from_env() -> Result<Self, CoreError> {
        // 加载 .env 文件（不存在则跳过，继续读系统环境变量）
        let _ = dotenv::dotenv();

        let proxy = env::var("HTTPS_PROXY")
            .or_else(|_| env::var("https_proxy"))
            .ok()
            .filter(|s| !s.is_empty());

        Ok(Self {
            proxy,
            binance: optional_pair("BINANCE_API_KEY", "BINANCE_SECRET")
                .map(|(k, s)| BinanceConfig { api_key: k, secret: s }),

            binance_testnet: optional_pair("BINANCE_TESTNET_KEY", "BINANCE_TESTNET_SECRET")
                .map(|(k, s)| BinanceConfig { api_key: k, secret: s }),

            okx: optional_triple("OKX_API_KEY", "OKX_SECRET", "OKX_PASSPHRASE")
                .map(|(k, s, p)| OkxConfig { api_key: k, secret: s, passphrase: p }),

            coinbase: optional_pair("COINBASE_API_KEY", "COINBASE_SECRET")
                .map(|(k, s)| CoinbaseConfig { api_key: k, secret: s }),

            coinbase_testnet: optional_pair("COINBASE_SANDBOX_KEY", "COINBASE_SANDBOX_SECRET")
                .map(|(k, s)| CoinbaseConfig { api_key: k, secret: s }),

            polymarket: optional_pair("POLY_API_KEY", "POLY_SECRET")
                .map(|(k, s)| PolymarketConfig { api_key: k, secret: s }),
        })
    }

    // ── 严格 require（缺少时返回 Err）────────────────────────────────────────

    pub fn require_binance(&self) -> Result<&BinanceConfig, CoreError> {
        self.binance.as_ref().ok_or_else(|| CoreError::Auth(
            "缺少 Binance 配置，请在 .env 中设置 BINANCE_API_KEY / BINANCE_SECRET".into()))
    }

    pub fn require_binance_testnet(&self) -> Result<&BinanceConfig, CoreError> {
        self.binance_testnet.as_ref().ok_or_else(|| CoreError::Auth(
            "缺少 Binance 测试网配置，请在 .env 中设置 BINANCE_TESTNET_KEY / BINANCE_TESTNET_SECRET\n  获取地址：https://testnet.binance.vision".into()))
    }

    pub fn require_okx(&self) -> Result<&OkxConfig, CoreError> {
        self.okx.as_ref().ok_or_else(|| CoreError::Auth(
            "缺少 OKX 配置，请在 .env 中设置 OKX_API_KEY / OKX_SECRET / OKX_PASSPHRASE".into()))
    }

    pub fn require_coinbase(&self) -> Result<&CoinbaseConfig, CoreError> {
        self.coinbase.as_ref().ok_or_else(|| CoreError::Auth(
            "缺少 Coinbase 配置，请在 .env 中设置 COINBASE_API_KEY / COINBASE_SECRET".into()))
    }

    pub fn require_polymarket(&self) -> Result<&PolymarketConfig, CoreError> {
        self.polymarket.as_ref().ok_or_else(|| CoreError::Auth(
            "缺少 Polymarket 配置，请在 .env 中设置 POLY_API_KEY / POLY_SECRET".into()))
    }

    /// 打印脱敏摘要（只显示 Key 前 4 位）
    pub fn print_summary(&self) {
        println!("=== 配置加载摘要 ===");
        show("Binance  生产网", self.binance.as_ref().map(|c| &c.api_key));
        show("Binance  测试网", self.binance_testnet.as_ref().map(|c| &c.api_key));
        show("OKX      生产网", self.okx.as_ref().map(|c| &c.api_key));
        show("Coinbase 生产网", self.coinbase.as_ref().map(|c| &c.api_key));
        show("Coinbase 沙箱  ", self.coinbase_testnet.as_ref().map(|c| &c.api_key));
        show("Polymarket     ", self.polymarket.as_ref().map(|c| &c.api_key));
    }
}

fn show(name: &str, key: Option<&String>) {
    match key {
        Some(k) => {
            let preview = if k.len() >= 4 { &k[..4] } else { k.as_str() };
            println!("  {:<20} ✅  {}****", name, preview);
        }
        None => println!("  {:<20} ⬜  未配置", name),
    }
}
