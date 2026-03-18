use async_trait::async_trait;
use reqwest::{Client, header};
use serde::Deserialize;
use serde_json::{json, Value};
use hq_core::{traits::Result, types::*, error::CoreError};
use crate::utils::*;
use crate::testnet::ExchangeConfig;

const GAMMA_BASE: &str = "https://gamma-api.polymarket.com";

pub struct PolymarketClient {
    client:     Client,
    api_key:    String,
    secret:     String,
    config:     ExchangeConfig,
}

impl PolymarketClient {
    pub fn new(api_key: impl Into<String>, secret: impl Into<String>) -> Self {
        Self::with_config(api_key, secret, ExchangeConfig::polymarket_live())
    }
    pub fn testnet(api_key: impl Into<String>, secret: impl Into<String>) -> Self {
        Self::with_config(api_key, secret, ExchangeConfig::polymarket_testnet())
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

    fn url(&self, path: &str)       -> String { format!("{}{}", self.config.rest_base, path) }
    fn gamma_url(&self, path: &str) -> String { format!("{}{}", GAMMA_BASE, path) }

    fn auth_headers(&self, method: &str, path: &str, body: &str) -> header::HeaderMap {
        let ts  = timestamp_ms();
        let sig = hmac_hex(&self.secret, &format!("{}{}{}{}", ts, method, path, body));
        let mut m = header::HeaderMap::new();
        m.insert("POLY-API-KEY",   header::HeaderValue::from_str(&self.api_key).unwrap());
        m.insert("POLY-SIGNATURE", header::HeaderValue::from_str(&sig).unwrap());
        m.insert("POLY-TIMESTAMP", header::HeaderValue::from_str(&ts.to_string()).unwrap());
        m.insert("Content-Type",   header::HeaderValue::from_static("application/json"));
        m
    }

    async fn clob_get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let hdrs = self.auth_headers("GET", path, "");
        let r = self.client.get(self.url(path)).headers(hdrs).send().await
            .map_err(|e| CoreError::Http(e.to_string()))?;
        self.parse(r, "Polymarket-CLOB").await
    }

    async fn clob_post<T: for<'de> Deserialize<'de>>(&self, path: &str, body: Value) -> Result<T> {
        let bs = serde_json::to_string(&body).map_err(|e| CoreError::Json(e.to_string()))?;
        let hdrs = self.auth_headers("POST", path, &bs);
        let r = self.client.post(self.url(path)).headers(hdrs).body(bs).send().await
            .map_err(|e| CoreError::Http(e.to_string()))?;
        self.parse(r, "Polymarket-CLOB").await
    }

    async fn gamma_get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let r = self.client.get(self.gamma_url(path)).send().await
            .map_err(|e| CoreError::Http(e.to_string()))?;
        self.parse(r, "Polymarket-Gamma").await
    }

    async fn parse<T: for<'de> Deserialize<'de>>(&self, r: reqwest::Response, name: &str) -> Result<T> {
        let status = r.status();
        let text = r.text().await.map_err(|e| CoreError::Http(e.to_string()))?;
        if !status.is_success() {
            let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            return Err(CoreError::Api {
                exchange: name.into(),
                code:    status.as_u16() as i64,
                message: v["error"].as_str().unwrap_or(&text).into(),
            });
        }
        serde_json::from_str(&text).map_err(|e| CoreError::Parse(e.to_string()))
    }

    /// 获取预测市场列表（公开，无需认证）
    pub async fn get_markets(&self, limit: u32, offset: u32) -> Result<Vec<PolymarketMarket>> {
        let raw: Vec<Value> = self.gamma_get(
            &format!("/markets?limit={}&offset={}&active=true&closed=false", limit, offset)).await?;
        Ok(raw.into_iter().filter_map(|m| {
            let condition_id = m["conditionId"].as_str()?.to_string();
            let tokens = m["tokens"].as_array().unwrap_or(&vec![]).iter().filter_map(|t| {
                Some(PolymarketToken {
                    token_id: t["token_id"].as_str()?.to_string(),
                    outcome:  t["outcome"].as_str().unwrap_or("").into(),
                    price:    t["price"].as_f64().unwrap_or(0.0),
                })
            }).collect();
            Some(PolymarketMarket {
                condition_id,
                question:    m["question"].as_str().unwrap_or("").into(),
                description: m["description"].as_str().map(Into::into),
                end_date:    m["endDate"].as_str()
                    .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok()),
                active: m["active"].as_bool().unwrap_or(false),
                tokens,
            })
        }).collect())
    }
}

#[derive(Deserialize)]
struct PolyOrder {
    id:            String,
    asset_id:      String,
    side:          String,
    price:         String,
    original_size: String,
    size_matched:  String,
    status:        String,
    created_at:    u64,
}

fn map_poly_order(o: PolyOrder) -> Order {
    let filled = parse_f64(&o.size_matched);
    let price  = parse_f64(&o.price);
    Order {
        order_id:        o.id,
        client_order_id: None,
        symbol:          o.asset_id,
        side:            if o.side.to_lowercase() == "buy" { OrderSide::Buy } else { OrderSide::Sell },
        order_type:      OrderType::Limit,
        price:           Some(price),
        qty:             parse_f64(&o.original_size),
        filled_qty:      filled,
        avg_fill_price:  if filled > 0.0 { Some(price) } else { None },
        status: match o.status.as_str() {
            "LIVE"             => OrderStatus::New,
            "MATCHED"          => OrderStatus::Filled,
            "PARTIALLY_FILLED" => OrderStatus::PartiallyFilled,
            "CANCELLED"        => OrderStatus::Cancelled,
            _                  => OrderStatus::New,
        },
        created_at: ms_to_dt(o.created_at),
        updated_at: None,
    }
}

#[async_trait]
impl hq_core::traits::Exchange for PolymarketClient {
    fn name(&self) -> &'static str { "Polymarket" }
    fn environment(&self) -> &Environment { &self.config.environment }

    async fn get_ticker(&self, token_id: &str) -> Result<Ticker> {
        let raw: Value = self.clob_get(&format!("/book?token_id={}", token_id)).await?;
        let lv = |v: &Value| Level {
            price: v["price"].as_str().map(parse_f64).unwrap_or(0.0),
            qty:   v["size"].as_str().map(parse_f64).unwrap_or(0.0),
        };
        let bids: Vec<Level> = raw["bids"].as_array().unwrap_or(&vec![]).iter().map(lv).collect();
        let asks: Vec<Level> = raw["asks"].as_array().unwrap_or(&vec![]).iter().map(lv).collect();
        let last_raw: Value = self.clob_get(&format!("/last-trade-price?token_id={}", token_id)).await
            .unwrap_or(Value::Null);
        let last = last_raw["price"].as_str().map(parse_f64).unwrap_or(0.0);
        Ok(Ticker {
            symbol:           token_id.into(),
            bid:              bids.first().map(|l| l.price).unwrap_or(0.0),
            ask:              asks.first().map(|l| l.price).unwrap_or(0.0),
            last,
            volume_24h:       0.0,
            price_change_pct: 0.0,
            timestamp:        chrono::Utc::now(),
        })
    }

    async fn get_order_book(&self, token_id: &str, _depth: u32) -> Result<OrderBook> {
        let raw: Value = self.clob_get(&format!("/book?token_id={}", token_id)).await?;
        let lv = |v: &Value| Level {
            price: v["price"].as_str().map(parse_f64).unwrap_or(0.0),
            qty:   v["size"].as_str().map(parse_f64).unwrap_or(0.0),
        };
        Ok(OrderBook {
            symbol:    token_id.into(),
            bids:      raw["bids"].as_array().unwrap_or(&vec![]).iter().map(lv).collect(),
            asks:      raw["asks"].as_array().unwrap_or(&vec![]).iter().map(lv).collect(),
            timestamp: chrono::Utc::now(),
        })
    }

    async fn get_candles(&self, _symbol: &str, _interval: &str, _limit: u32) -> Result<Vec<Candle>> {
        Err(CoreError::Unsupported("Polymarket 不支持 K 线数据".into()))
    }

    async fn get_account(&self) -> Result<AccountInfo> {
        Ok(AccountInfo { balances: vec![], can_trade: true, can_withdraw: false, timestamp: chrono::Utc::now() })
    }

    async fn place_order(&self, req: PlaceOrderRequest) -> Result<Order> {
        let price = req.price.ok_or_else(|| CoreError::InvalidParam("Polymarket 必须指定限价".into()))?;
        let body = json!({
            "token_id": req.symbol, "price": price.to_string(),
            "size": req.qty.to_string(),
            "side": if req.side == OrderSide::Buy { "BUY" } else { "SELL" },
            "type": "GTC",
        });
        let resp: Value = self.clob_post("/order", body).await?;
        let oid = resp["orderID"].as_str().unwrap_or("").to_string();
        self.get_order(&req.symbol, &oid).await
    }

    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<Order> {
        let _: Value = self.clob_post("/cancel", json!({ "orderID": order_id })).await?;
        self.get_order(symbol, order_id).await
    }

    async fn get_order(&self, _symbol: &str, order_id: &str) -> Result<Order> {
        let raw: PolyOrder = self.clob_get(&format!("/order/{}", order_id)).await?;
        Ok(map_poly_order(raw))
    }

    async fn get_open_orders(&self, symbol: Option<&str>) -> Result<Vec<Order>> {
        let path = match symbol {
            Some(s) => format!("/orders?asset_id={}&status=live", s),
            None    => "/orders?status=live".into(),
        };
        let raw: Vec<PolyOrder> = self.clob_get(&path).await?;
        Ok(raw.into_iter().map(map_poly_order).collect())
    }

    async fn get_my_trades(&self, symbol: &str, limit: u32) -> Result<Vec<Trade>> {
        let raw: Vec<Value> = self
            .clob_get(&format!("/trades?asset_id={}&limit={}", symbol, limit)).await?;
        Ok(raw.into_iter().map(|t| Trade {
            trade_id:  t["id"].as_str().unwrap_or("").into(),
            order_id:  t["maker_order_id"].as_str().unwrap_or("").into(),
            symbol:    t["asset_id"].as_str().unwrap_or("").into(),
            side:      if t["side"].as_str().unwrap_or("") == "BUY" { OrderSide::Buy } else { OrderSide::Sell },
            price:     t["price"].as_str().map(parse_f64).unwrap_or(0.0),
            qty:       t["size"].as_str().map(parse_f64).unwrap_or(0.0),
            fee:       t["fee"].as_str().map(parse_f64).unwrap_or(0.0),
            fee_asset: "USDC".into(),
            timestamp: ms_to_dt(t["created_at"].as_u64().unwrap_or(0)),
        }).collect())
    }
}
