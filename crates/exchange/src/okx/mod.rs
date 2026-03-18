use async_trait::async_trait;
use reqwest::{Client, header};
use serde::Deserialize;
use serde_json::{json, Value};
use hq_core::{traits::Result, types::*, error::CoreError};
use crate::utils::*;
use crate::testnet::ExchangeConfig;

pub struct OkxClient {
    client:     Client,
    api_key:    String,
    secret:     String,
    passphrase: String,
    config:     ExchangeConfig,
}

impl OkxClient {
    pub fn new(api_key: impl Into<String>, secret: impl Into<String>, passphrase: impl Into<String>) -> Self {
        Self::with_config(api_key, secret, passphrase, ExchangeConfig::okx_live())
    }
    pub fn testnet(api_key: impl Into<String>, secret: impl Into<String>, passphrase: impl Into<String>) -> Self {
        Self::with_config(api_key, secret, passphrase, ExchangeConfig::okx_testnet())
    }
    pub fn with_config(api_key: impl Into<String>, secret: impl Into<String>,
                       passphrase: impl Into<String>, config: ExchangeConfig) -> Self {
        let client = {
            let mut b = Client::builder();
            if let Some(proxy_url) = &config.proxy {
                b = b.proxy(reqwest::Proxy::all(proxy_url).expect("invalid proxy url"));
            }
            b.build().unwrap()
        };
        Self { client, api_key: api_key.into(),
               secret: secret.into(), passphrase: passphrase.into(), config }
    }

    fn url(&self, path: &str) -> String { format!("{}{}", self.config.rest_base, path) }

    fn auth_headers(&self, method: &str, path: &str, body: &str) -> header::HeaderMap {
        let ts  = iso8601_now();
        let sig = hmac_b64(&self.secret, &format!("{}{}{}{}", ts, method, path, body));
        let mut m = header::HeaderMap::new();
        m.insert("OK-ACCESS-KEY",        header::HeaderValue::from_str(&self.api_key).unwrap());
        m.insert("OK-ACCESS-SIGN",       header::HeaderValue::from_str(&sig).unwrap());
        m.insert("OK-ACCESS-TIMESTAMP",  header::HeaderValue::from_str(&ts).unwrap());
        m.insert("OK-ACCESS-PASSPHRASE", header::HeaderValue::from_str(&self.passphrase).unwrap());
        m.insert("Content-Type",         header::HeaderValue::from_static("application/json"));
        for (k, v) in &self.config.extra_headers {
            m.insert(header::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                     header::HeaderValue::from_str(v).unwrap());
        }
        m
    }

    async fn get_pub<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let r = self.client.get(self.url(path)).send().await
            .map_err(|e| CoreError::Http(e.to_string()))?;
        self.parse(r).await
    }

    async fn get_auth<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let hdrs = self.auth_headers("GET", path, "");
        let r = self.client.get(self.url(path)).headers(hdrs).send().await
            .map_err(|e| CoreError::Http(e.to_string()))?;
        self.parse(r).await
    }

    async fn post_auth<T: for<'de> Deserialize<'de>>(&self, path: &str, body: Value) -> Result<T> {
        let bs = serde_json::to_string(&body).map_err(|e| CoreError::Json(e.to_string()))?;
        let hdrs = self.auth_headers("POST", path, &bs);
        let r = self.client.post(self.url(path)).headers(hdrs).body(bs).send().await
            .map_err(|e| CoreError::Http(e.to_string()))?;
        self.parse(r).await
    }

    async fn parse<T: for<'de> Deserialize<'de>>(&self, r: reqwest::Response) -> Result<T> {
        let status = r.status();
        let text = r.text().await.map_err(|e| CoreError::Http(e.to_string()))?;
        let v: Value = serde_json::from_str(&text).map_err(|e| CoreError::Parse(e.to_string()))?;
        let code = v["code"].as_str().unwrap_or("0");
        if code != "0" {
            return Err(CoreError::Api {
                exchange: format!("OKX({:?})", self.config.environment),
                code:    code.parse().unwrap_or(status.as_u16() as i64),
                message: v["msg"].as_str().unwrap_or("unknown").into(),
            });
        }
        serde_json::from_value(v["data"].clone()).map_err(|e| CoreError::Parse(e.to_string()))
    }
}

// ── 内部结构 ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct OkxTicker {
    #[serde(rename = "instId")] inst_id: String,
    #[serde(rename = "bidPx")]  bid_px:  String,
    #[serde(rename = "askPx")]  ask_px:  String,
    last: String,
    #[serde(rename = "vol24h")]  vol24h:  String,
    #[serde(rename = "open24h")] open24h: String,
    ts: String,
}

#[derive(Deserialize)]
struct OkxBook { bids: Vec<Vec<String>>, asks: Vec<Vec<String>>, ts: String }

#[derive(Deserialize)]
struct OkxAccBal { details: Vec<OkxBal> }

#[derive(Deserialize)]
struct OkxBal {
    #[serde(rename = "ccy")]       ccy:    String,
    #[serde(rename = "availBal")]  avail:  String,
    #[serde(rename = "frozenBal")] frozen: String,
}

#[derive(Deserialize)]
struct OkxOrder {
    #[serde(rename = "ordId")]   ord_id:    String,
    #[serde(rename = "clOrdId")] cl_ord_id: String,
    #[serde(rename = "instId")] inst_id:   String,
    side:    String,
    #[serde(rename = "ordType")] ord_type:  String,
    px:      Option<String>,
    sz:      String,
    #[serde(rename = "fillSz")] fill_sz:   String,
    #[serde(rename = "fillPx")] fill_px:   Option<String>,
    state:   String,
    #[serde(rename = "cTime")]  c_time:    String,
    #[serde(rename = "uTime")]  u_time:    String,
}

#[derive(Deserialize)]
struct OkxFill {
    #[serde(rename = "tradeId")] trade_id: String,
    #[serde(rename = "ordId")]   ord_id:   String,
    #[serde(rename = "instId")]  inst_id:  String,
    side: String, px: String, sz: String,
    fee: String,
    #[serde(rename = "feeCcy")] fee_ccy: String,
    ts: String,
}

fn map_okx_order(o: OkxOrder) -> Order {
    let filled = parse_f64(&o.fill_sz);
    let avg    = o.fill_px.as_deref().map(parse_f64).filter(|&p| p > 0.0);
    Order {
        order_id:        o.ord_id,
        client_order_id: if o.cl_ord_id.is_empty() { None } else { Some(o.cl_ord_id) },
        symbol:          o.inst_id,
        side:            if o.side == "buy" { OrderSide::Buy } else { OrderSide::Sell },
        order_type:      if o.ord_type == "market" { OrderType::Market } else { OrderType::Limit },
        price:           o.px.as_deref().map(parse_f64).filter(|&p| p > 0.0),
        qty:             parse_f64(&o.sz),
        filled_qty:      filled,
        avg_fill_price:  avg,
        status: match o.state.as_str() {
            "live"             => OrderStatus::New,
            "partially_filled" => OrderStatus::PartiallyFilled,
            "filled"           => OrderStatus::Filled,
            "canceled"         => OrderStatus::Cancelled,
            _                  => OrderStatus::New,
        },
        created_at: ms_to_dt(o.c_time.parse().unwrap_or(0)),
        updated_at: Some(ms_to_dt(o.u_time.parse().unwrap_or(0))),
    }
}

#[async_trait]
impl hq_core::traits::Exchange for OkxClient {
    fn name(&self) -> &'static str { "OKX" }
    fn environment(&self) -> &Environment { &self.config.environment }

    async fn get_ticker(&self, symbol: &str) -> Result<Ticker> {
        let mut v: Vec<OkxTicker> = self
            .get_pub(&format!("/api/v5/market/ticker?instId={}", symbol)).await?;
        let t = v.pop().ok_or_else(|| CoreError::Parse("empty ticker".into()))?;
        let last = parse_f64(&t.last);
        let open = parse_f64(&t.open24h);
        let chg  = if open > 0.0 { (last - open) / open * 100.0 } else { 0.0 };
        Ok(Ticker {
            symbol: t.inst_id, bid: parse_f64(&t.bid_px), ask: parse_f64(&t.ask_px),
            last, volume_24h: parse_f64(&t.vol24h), price_change_pct: chg,
            timestamp: ms_to_dt(t.ts.parse().unwrap_or(0)),
        })
    }

    async fn get_order_book(&self, symbol: &str, depth: u32) -> Result<OrderBook> {
        let mut v: Vec<OkxBook> = self
            .get_pub(&format!("/api/v5/market/books?instId={}&sz={}", symbol, depth)).await?;
        let raw = v.pop().ok_or_else(|| CoreError::Parse("empty book".into()))?;
        let lv = |r: Vec<String>| Level {
            price: r.first().map(|s| parse_f64(s)).unwrap_or(0.0),
            qty:   r.get(1).map(|s| parse_f64(s)).unwrap_or(0.0),
        };
        Ok(OrderBook {
            symbol: symbol.into(),
            bids: raw.bids.into_iter().map(lv).collect(),
            asks: raw.asks.into_iter().map(lv).collect(),
            timestamp: ms_to_dt(raw.ts.parse().unwrap_or(0)),
        })
    }

    async fn get_candles(&self, symbol: &str, interval: &str, limit: u32) -> Result<Vec<Candle>> {
        let raw: Vec<Vec<String>> = self
            .get_pub(&format!("/api/v5/market/candles?instId={}&bar={}&limit={}", symbol, interval, limit))
            .await?;
        Ok(raw.into_iter().map(|r| Candle {
            open_time: ms_to_dt(r.first().and_then(|s| s.parse().ok()).unwrap_or(0)),
            open:   r.get(1).map(|s| parse_f64(s)).unwrap_or(0.0),
            high:   r.get(2).map(|s| parse_f64(s)).unwrap_or(0.0),
            low:    r.get(3).map(|s| parse_f64(s)).unwrap_or(0.0),
            close:  r.get(4).map(|s| parse_f64(s)).unwrap_or(0.0),
            volume: r.get(5).map(|s| parse_f64(s)).unwrap_or(0.0),
        }).collect())
    }

    async fn get_account(&self) -> Result<AccountInfo> {
        let mut v: Vec<OkxAccBal> = self.get_auth("/api/v5/account/balance").await?;
        let acc = v.pop().ok_or_else(|| CoreError::Parse("empty account".into()))?;
        Ok(AccountInfo {
            balances: acc.details.into_iter().map(|b| Balance {
                asset: b.ccy, free: parse_f64(&b.avail), locked: parse_f64(&b.frozen),
            }).collect(),
            can_trade: true, can_withdraw: true, timestamp: chrono::Utc::now(),
        })
    }

    async fn place_order(&self, req: PlaceOrderRequest) -> Result<Order> {
        let mut body = json!({
            "instId":  req.symbol,
            "tdMode":  "cash",
            "side":    if req.side == OrderSide::Buy { "buy" } else { "sell" },
            "ordType": if req.order_type == OrderType::Market { "market" } else { "limit" },
            "sz":      req.qty.to_string(),
        });
        if let Some(p) = req.price { body["px"] = json!(p.to_string()); }
        if let Some(cid) = req.client_order_id { body["clOrdId"] = json!(cid); }
        let mut v: Vec<Value> = self.post_auth("/api/v5/trade/order", body).await?;
        let obj = v.pop().ok_or_else(|| CoreError::Parse("empty resp".into()))?;
        let oid = obj["ordId"].as_str().unwrap_or("").to_string();
        self.get_order(&req.symbol, &oid).await
    }

    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<Order> {
        let _: Vec<Value> = self.post_auth("/api/v5/trade/cancel-order",
            json!({ "instId": symbol, "ordId": order_id })).await?;
        self.get_order(symbol, order_id).await
    }

    async fn get_order(&self, symbol: &str, order_id: &str) -> Result<Order> {
        let mut v: Vec<OkxOrder> = self
            .get_auth(&format!("/api/v5/trade/order?instId={}&ordId={}", symbol, order_id)).await?;
        Ok(map_okx_order(v.pop().ok_or_else(|| CoreError::OrderNotFound(order_id.into()))?))
    }

    async fn get_open_orders(&self, symbol: Option<&str>) -> Result<Vec<Order>> {
        let path = match symbol {
            Some(s) => format!("/api/v5/trade/orders-pending?instId={}", s),
            None    => "/api/v5/trade/orders-pending".into(),
        };
        let v: Vec<OkxOrder> = self.get_auth(&path).await?;
        Ok(v.into_iter().map(map_okx_order).collect())
    }

    async fn get_my_trades(&self, symbol: &str, limit: u32) -> Result<Vec<Trade>> {
        let v: Vec<OkxFill> = self
            .get_auth(&format!("/api/v5/trade/fills?instId={}&limit={}", symbol, limit)).await?;
        Ok(v.into_iter().map(|f| Trade {
            trade_id:  f.trade_id,
            order_id:  f.ord_id,
            symbol:    f.inst_id,
            side:      if f.side == "buy" { OrderSide::Buy } else { OrderSide::Sell },
            price:     parse_f64(&f.px),
            qty:       parse_f64(&f.sz),
            fee:       parse_f64(f.fee.trim_start_matches('-')),
            fee_asset: f.fee_ccy,
            timestamp: ms_to_dt(f.ts.parse().unwrap_or(0)),
        }).collect())
    }
}
