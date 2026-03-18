use async_trait::async_trait;
use reqwest::{Client, header};
use serde::Deserialize;
use serde_json::{json, Value};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use hq_core::{traits::Result, types::*, error::CoreError};
use crate::utils::*;
use crate::testnet::ExchangeConfig;

pub struct CoinbaseClient {
    client:  Client,
    api_key: String,
    secret:  String,
    config:  ExchangeConfig,
}

impl CoinbaseClient {
    pub fn new(api_key: impl Into<String>, secret: impl Into<String>) -> Self {
        Self::with_config(api_key, secret, ExchangeConfig::coinbase_live())
    }
    pub fn testnet(api_key: impl Into<String>, secret: impl Into<String>) -> Self {
        Self::with_config(api_key, secret, ExchangeConfig::coinbase_testnet())
    }
    pub fn with_config(api_key: impl Into<String>, secret: impl Into<String>, config: ExchangeConfig) -> Self {
        let client = {
            let mut b = Client::builder();
            if let Some(proxy_url) = &config.proxy {
                b = b.proxy(reqwest::Proxy::all(proxy_url).expect("invalid proxy url"));
            }
            b.build().unwrap()
        };
        Self { client, api_key: api_key.into(), secret: secret.into(), config }
    }

    fn url(&self, path: &str) -> String { format!("{}{}", self.config.rest_base, path) }

    fn auth_headers(&self, method: &str, path: &str, body: &str) -> header::HeaderMap {
        let ts  = timestamp_secs();
        let sig = hmac_hex(&self.secret, &format!("{}{}{}{}", ts, method, path, body));
        let mut m = header::HeaderMap::new();
        m.insert("CB-ACCESS-KEY",        header::HeaderValue::from_str(&self.api_key).unwrap());
        m.insert("CB-ACCESS-SIGN",       header::HeaderValue::from_str(&sig).unwrap());
        m.insert("CB-ACCESS-TIMESTAMP",  header::HeaderValue::from_str(&ts.to_string()).unwrap());
        m.insert("Content-Type",         header::HeaderValue::from_static("application/json"));
        for (k, v) in &self.config.extra_headers {
            m.insert(header::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                     header::HeaderValue::from_str(v).unwrap());
        }
        m
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let r = self.client.get(self.url(path))
            .headers(self.auth_headers("GET", path, ""))
            .send().await.map_err(|e| CoreError::Http(e.to_string()))?;
        self.parse(r).await
    }

    async fn post<T: for<'de> Deserialize<'de>>(&self, path: &str, body: Value) -> Result<T> {
        let bs = serde_json::to_string(&body).map_err(|e| CoreError::Json(e.to_string()))?;
        let r = self.client.post(self.url(path))
            .headers(self.auth_headers("POST", path, &bs))
            .body(bs).send().await.map_err(|e| CoreError::Http(e.to_string()))?;
        self.parse(r).await
    }

    async fn parse<T: for<'de> Deserialize<'de>>(&self, r: reqwest::Response) -> Result<T> {
        let status = r.status();
        let text = r.text().await.map_err(|e| CoreError::Http(e.to_string()))?;
        if !status.is_success() {
            let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            return Err(CoreError::Api {
                exchange: format!("Coinbase({:?})", self.config.environment),
                code:    status.as_u16() as i64,
                message: v["message"].as_str().or(v["error"].as_str()).unwrap_or(&text).into(),
            });
        }
        serde_json::from_str(&text).map_err(|e| CoreError::Parse(e.to_string()))
    }
}

// ── 内部结构 ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CbBook { pricebook: CbPricebook }
#[derive(Deserialize)]
struct CbPricebook { product_id: String, bids: Vec<CbLvl>, asks: Vec<CbLvl>, time: String }
#[derive(Deserialize)]
struct CbLvl { price: String, size: String }

#[derive(Deserialize)]
struct CbOrderResp {
    order_id:             String,
    client_order_id:      Option<String>,
    product_id:           String,
    side:                 String,
    order_type:           Option<String>,
    order_configuration:  Value,
    status:               String,
    filled_size:          Option<String>,
    average_filled_price: Option<String>,
    created_time:         String,
    last_fill_time:       Option<String>,
}

#[derive(Deserialize)] struct CbOrderList { orders: Vec<CbOrderResp> }
#[derive(Deserialize)] struct CbFills { fills: Vec<CbFill> }
#[derive(Deserialize)]
struct CbFill {
    entry_id: String, order_id: String, product_id: String,
    side: String, price: String, size: String,
    commission: String, trade_time: String,
}

fn map_cb_order(o: CbOrderResp) -> Order {
    let (order_type, price) = match o.order_type.as_deref().unwrap_or("") {
        "MARKET" => (OrderType::Market, None),
        _ => {
            let p = ["limit_limit_gtc", "limit_limit_ioc"].iter()
                .find_map(|k| o.order_configuration.get(k))
                .and_then(|c| c.get("limit_price"))
                .and_then(|v| v.as_str())
                .map(parse_f64);
            (OrderType::Limit, p)
        }
    };
    Order {
        order_id:        o.order_id,
        client_order_id: o.client_order_id,
        symbol:          o.product_id,
        side:            if o.side == "BUY" { OrderSide::Buy } else { OrderSide::Sell },
        order_type, price,
        qty:             0.0,
        filled_qty:      o.filled_size.as_deref().map(parse_f64).unwrap_or(0.0),
        avg_fill_price:  o.average_filled_price.as_deref().map(parse_f64),
        status: match o.status.as_str() {
            "OPEN"      => OrderStatus::New,
            "FILLED"    => OrderStatus::Filled,
            "CANCELLED" => OrderStatus::Cancelled,
            "EXPIRED"   => OrderStatus::Expired,
            _           => OrderStatus::New,
        },
        created_at: o.created_time.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now()),
        updated_at: o.last_fill_time.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
    }
}

#[async_trait]
impl hq_core::traits::Exchange for CoinbaseClient {
    fn name(&self) -> &'static str { "Coinbase" }
    fn environment(&self) -> &Environment { &self.config.environment }

    async fn get_ticker(&self, symbol: &str) -> Result<Ticker> {
        let book: CbBook = self.get(
            &format!("/api/v3/brokerage/market/product_book?product_id={}&limit=1", symbol)).await?;
        let product: Value = self.get(
            &format!("/api/v3/brokerage/market/products/{}", symbol)).await?;
        let pb = book.pricebook;
        Ok(Ticker {
            symbol:           pb.product_id,
            bid:              pb.bids.first().map(|l| parse_f64(&l.price)).unwrap_or(0.0),
            ask:              pb.asks.first().map(|l| parse_f64(&l.price)).unwrap_or(0.0),
            last:             product["price"].as_str().map(parse_f64).unwrap_or(0.0),
            volume_24h:       product["volume_24h"].as_str().map(parse_f64).unwrap_or(0.0),
            price_change_pct: product["price_percentage_change_24h"].as_str().map(parse_f64).unwrap_or(0.0),
            timestamp:        pb.time.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now()),
        })
    }

    async fn get_order_book(&self, symbol: &str, depth: u32) -> Result<OrderBook> {
        let book: CbBook = self.get(
            &format!("/api/v3/brokerage/market/product_book?product_id={}&limit={}", symbol, depth)).await?;
        let pb = book.pricebook;
        let lv = |l: CbLvl| Level { price: parse_f64(&l.price), qty: parse_f64(&l.size) };
        Ok(OrderBook {
            symbol:    pb.product_id,
            bids:      pb.bids.into_iter().map(lv).collect(),
            asks:      pb.asks.into_iter().map(lv).collect(),
            timestamp: pb.time.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now()),
        })
    }

    async fn get_candles(&self, symbol: &str, interval: &str, limit: u32) -> Result<Vec<Candle>> {
        let now   = Utc::now().timestamp();
        let gran: i64 = match interval {
            "1m" => 60, "5m" => 300, "15m" => 900,
            "1h" | "ONE_HOUR" => 3600, "6h" => 21600, "1d" => 86400, _ => 3600,
        };
        let start = now - gran * limit as i64;
        let raw: Value = self.get(&format!(
            "/api/v3/brokerage/market/products/{}/candles?start={}&end={}&granularity={}",
            symbol, start, now, interval.to_uppercase())).await?;
        Ok(raw["candles"].as_array().unwrap_or(&vec![]).iter().map(|c| Candle {
            open_time: secs_to_dt(c["start"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0)),
            open:   c["open"].as_str().map(parse_f64).unwrap_or(0.0),
            high:   c["high"].as_str().map(parse_f64).unwrap_or(0.0),
            low:    c["low"].as_str().map(parse_f64).unwrap_or(0.0),
            close:  c["close"].as_str().map(parse_f64).unwrap_or(0.0),
            volume: c["volume"].as_str().map(parse_f64).unwrap_or(0.0),
        }).collect())
    }

    async fn get_account(&self) -> Result<AccountInfo> {
        let v: Value = self.get("/api/v3/brokerage/portfolios").await?;
        let uuid = v["portfolios"].as_array()
            .and_then(|a| a.first())
            .and_then(|p| p["uuid"].as_str())
            .unwrap_or("");
        let detail: Value = self.get(&format!("/api/v3/brokerage/portfolios/{}", uuid)).await?;
        let balances = detail["breakdown"]["spot_positions"].as_array()
            .unwrap_or(&vec![]).iter().map(|p| Balance {
                asset:  p["asset"].as_str().unwrap_or("").into(),
                free:   p["available_to_trade_crypto"].as_str().map(parse_f64).unwrap_or(0.0),
                locked: p["hold"].as_str().map(parse_f64).unwrap_or(0.0),
            }).collect();
        Ok(AccountInfo { balances, can_trade: true, can_withdraw: true, timestamp: Utc::now() })
    }

    async fn place_order(&self, req: PlaceOrderRequest) -> Result<Order> {
        let cid = req.client_order_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let cfg = match req.order_type {
            OrderType::Market => json!({ "market_market_ioc": { "base_size": req.qty.to_string() } }),
            _ => json!({ "limit_limit_gtc": {
                "base_size":   req.qty.to_string(),
                "limit_price": req.price.unwrap_or(0.0).to_string(),
                "post_only":   false,
            }}),
        };
        let body = json!({
            "client_order_id": cid,
            "product_id": req.symbol,
            "side": if req.side == OrderSide::Buy { "BUY" } else { "SELL" },
            "order_configuration": cfg,
        });
        let resp: Value = self.post("/api/v3/brokerage/orders", body).await?;
        if resp["success"].as_bool() == Some(false) {
            return Err(CoreError::Api {
                exchange: "Coinbase".into(), code: -1,
                message: resp["error_response"]["message"].as_str().unwrap_or("unknown").into(),
            });
        }
        let oid = resp["success_response"]["order_id"].as_str().unwrap_or("").to_string();
        self.get_order(&req.symbol, &oid).await
    }

    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<Order> {
        let _: Value = self.post("/api/v3/brokerage/orders/batch_cancel",
            json!({ "order_ids": [order_id] })).await?;
        self.get_order(symbol, order_id).await
    }

    async fn get_order(&self, _symbol: &str, order_id: &str) -> Result<Order> {
        let resp: Value = self.get(
            &format!("/api/v3/brokerage/orders/historical/{}", order_id)).await?;
        let o: CbOrderResp = serde_json::from_value(resp["order"].clone())
            .map_err(|e| CoreError::Parse(e.to_string()))?;
        Ok(map_cb_order(o))
    }

    async fn get_open_orders(&self, symbol: Option<&str>) -> Result<Vec<Order>> {
        let path = match symbol {
            Some(s) => format!("/api/v3/brokerage/orders/historical/batch?product_id={}&order_status=OPEN", s),
            None    => "/api/v3/brokerage/orders/historical/batch?order_status=OPEN".into(),
        };
        let resp: CbOrderList = self.get(&path).await?;
        Ok(resp.orders.into_iter().map(map_cb_order).collect())
    }

    async fn get_my_trades(&self, symbol: &str, limit: u32) -> Result<Vec<Trade>> {
        let resp: CbFills = self.get(&format!(
            "/api/v3/brokerage/orders/historical/fills?product_id={}&limit={}", symbol, limit)).await?;
        Ok(resp.fills.into_iter().map(|f| Trade {
            trade_id:  f.entry_id,
            order_id:  f.order_id,
            symbol:    f.product_id,
            side:      if f.side == "BUY" { OrderSide::Buy } else { OrderSide::Sell },
            price:     parse_f64(&f.price),
            qty:       parse_f64(&f.size),
            fee:       parse_f64(&f.commission),
            fee_asset: "USD".into(),
            timestamp: f.trade_time.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now()),
        }).collect())
    }
}
