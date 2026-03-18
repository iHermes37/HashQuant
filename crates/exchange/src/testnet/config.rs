use hq_core::types::Environment;
use std::collections::HashMap;

/// 单个交易所的运行时连接配置
#[derive(Debug, Clone)]
pub struct ExchangeConfig {
    pub rest_base:     String,
    pub extra_headers: HashMap<String, String>,
    pub environment:   Environment,
    /// 可选代理，格式如 "http://127.0.0.1:7890"
    pub proxy:         Option<String>,
}

impl ExchangeConfig {
    pub fn binance_live() -> Self {
        Self { proxy: None, rest_base: "https://api.binance.com".into(),
               extra_headers: HashMap::new(), environment: Environment::Live }
    }
    pub fn binance_testnet() -> Self {
        Self { proxy: None, rest_base: "https://testnet.binance.vision".into(),
               extra_headers: HashMap::new(), environment: Environment::Testnet }
    }
    pub fn okx_live() -> Self {
        Self { proxy: None, rest_base: "https://www.okx.com".into(),
               extra_headers: HashMap::new(), environment: Environment::Live }
    }
    /// OKX 模拟盘：同域名，加 x-simulated-trading: 1
    pub fn okx_testnet() -> Self {
        let mut h = HashMap::new();
        h.insert("x-simulated-trading".into(), "1".into());
        Self { proxy: None, rest_base: "https://www.okx.com".into(),
               extra_headers: h, environment: Environment::Testnet }
    }
    pub fn coinbase_live() -> Self {
        Self { proxy: None, rest_base: "https://api.coinbase.com".into(),
               extra_headers: HashMap::new(), environment: Environment::Live }
    }
    pub fn coinbase_testnet() -> Self {
        Self { proxy: None, rest_base: "https://api-public.sandbox.exchange.coinbase.com".into(),
               extra_headers: HashMap::new(), environment: Environment::Testnet }
    }
    pub fn polymarket_live() -> Self {
        Self { proxy: None, rest_base: "https://clob.polymarket.com".into(),
               extra_headers: HashMap::new(), environment: Environment::Live }
    }
    pub fn polymarket_testnet() -> Self {
        // Polymarket 无官方测试网，标记 Testnet 但连接相同地址
        Self { proxy: None, rest_base: "https://clob.polymarket.com".into(),
               extra_headers: HashMap::new(), environment: Environment::Testnet }
    }
}
