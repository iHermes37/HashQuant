use async_trait::async_trait;
use reqwest::{Client, header};
use serde::Deserialize;
use serde_json::Value;
use hq_core::{traits::Result, types::*, error::CoreError};
use crate::utils::*;
use crate::testnet::ExchangeConfig;

pub struct BinanceClient {
    client: Client,
    secret: String,
    config: ExchangeConfig,
}

impl BinanceClient {
    pub fn new(api_key: impl Into<String>, secret: impl Into<String>) -> Self {
        Self::with_config(api_key, secret, ExchangeConfig::binance_live())
    }
    pub fn testnet(api_key: impl Into<String>, secret: impl Into<String>) -> Self {
        Self::with_config(api_key, secret, ExchangeConfig::binance_testnet())
    }
    pub fn with_config(api_key: impl Into<String>, secret: impl Into<String>, config: ExchangeConfig) -> Self {
        let key = api_key.into();
        let mut dh = header::HeaderMap::new();
        dh.insert("X-MBX-APIKEY", header::HeaderValue::from_str(&key).unwrap());
        for (k, v) in &config.extra_headers {
            dh.insert(header::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                      header::HeaderValue::from_str(v).unwrap());
        }
        let mut builder = Client::builder().default_headers(dh);
        if let Some(proxy_url) = &config.proxy {
            builder = builder.proxy(reqwest::Proxy::all(proxy_url).expect("invalid proxy url"));
        }
        let client = builder.build().unwrap();
        Self { client, secret: secret.into(), config }
    }

    fn url(&self, path: &str) -> String { format!("{}{}", self.config.rest_base, path) }
    fn sign(&self, qs: &str) -> String { hmac_hex(&self.secret, qs) }

    async fn get_pub<T: for<'de> Deserialize<'de>>(&self, path: &str, qs: &str) -> Result<T> {
        let url = if qs.is_empty() { self.url(path) } else { format!("{}?{}", self.url(path), qs) };
        let r = self.client.get(&url).send().await.map_err(|e| CoreError::Http(e.to_string()))?;
        self.parse(r).await
    }

    async fn get_signed<T: for<'de> Deserialize<'de>>(&self, path: &str, mut p: Vec<(&str, String)>) -> Result<T> {
        p.push(("timestamp", timestamp_ms().to_string()));
        let qs = build_qs(&p);
        let url = format!("{}?{}&signature={}", self.url(path), qs, self.sign(&qs));
        let r = self.client.get(&url).send().await.map_err(|e| CoreError::Http(e.to_string()))?;
        self.parse(r).await
    }

    async fn post_signed<T: for<'de> Deserialize<'de>>(&self, path: &str, mut p: Vec<(&str, String)>) -> Result<T> {
        p.push(("timestamp", timestamp_ms().to_string()));
        let qs = build_qs(&p);
        let body = format!("{}&signature={}", qs, self.sign(&qs));
        let r = self.client.post(self.url(path))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body).send().await.map_err(|e| CoreError::Http(e.to_string()))?;
        self.parse(r).await
    }

    async fn delete_signed<T: for<'de> Deserialize<'de>>(&self, path: &str, mut p: Vec<(&str, String)>) -> Result<T> {
        p.push(("timestamp", timestamp_ms().to_string()));
        let qs = build_qs(&p);
        let url = format!("{}?{}&signature={}", self.url(path), qs, self.sign(&qs));
        let r = self.client.delete(&url).send().await.map_err(|e| CoreError::Http(e.to_string()))?;
        self.parse(r).await
    }

    async fn parse<T: for<'de> Deserialize<'de>>(&self, r: reqwest::Response) -> Result<T> {
        let status = r.status();
        let text = r.text().await.map_err(|e| CoreError::Http(e.to_string()))?;
        if !status.is_success() {
            let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            return Err(CoreError::Api {
                exchange: format!("Binance({})", self.config.rest_base),
                code:    v["code"].as_i64().unwrap_or(status.as_u16() as i64),
                message: v["msg"].as_str().unwrap_or(&text).into(),
            });
        }
        serde_json::from_str(&text).map_err(|e| CoreError::Parse(e.to_string()))
    }
}

// ── 内部响应结构 ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct Bn24h {
    symbol: String,
    #[serde(rename = "bidPrice")]           bid_price:  String,
    #[serde(rename = "askPrice")]           ask_price:  String,
    #[serde(rename = "lastPrice")]          last_price: String,
    #[serde(rename = "volume")]             volume:     String,
    #[serde(rename = "priceChangePercent")] change_pct: String,
    #[serde(rename = "closeTime")]          close_time: u64,
}

#[derive(Deserialize)]
struct BnDepth { bids: Vec<[String; 2]>, asks: Vec<[String; 2]> }

#[derive(Deserialize)]
struct BnAccount {
    #[serde(rename = "canTrade")]    can_trade:    bool,
    #[serde(rename = "canWithdraw")] can_withdraw: bool,
    balances: Vec<BnBalance>,
    #[serde(rename = "updateTime")] update_time: u64,
}
#[derive(Deserialize)]
struct BnBalance { asset: String, free: String, locked: String }

#[derive(Deserialize)]
struct BnOrder {
    #[serde(rename = "orderId")]            order_id:        u64,
    #[serde(rename = "clientOrderId")]      client_order_id: String,
    symbol: String,
    side:   String,
    #[serde(rename = "type")]               order_type:      String,
    price:  String,
    #[serde(rename = "origQty")]            orig_qty:        String,
    #[serde(rename = "executedQty")]        executed_qty:    String,
    #[serde(rename = "cummulativeQuoteQty")] cumulative_quote: String,
    status: String,
    #[serde(rename = "transactTime", default)] transact_time: u64,
    #[serde(rename = "time",         default)] time:          u64,
    #[serde(rename = "updateTime",   default)] update_time:   u64,
}

#[derive(Deserialize)]
struct BnTrade {
    id: u64,
    #[serde(rename = "orderId")]          order_id:         u64,
    symbol: String,
    #[serde(rename = "isBuyer")]          is_buyer:         bool,
    price: String, qty: String,
    commission: String,
    #[serde(rename = "commissionAsset")] commission_asset: String,
    time: u64,
}

fn map_bn_order(o: BnOrder) -> Order {
    let ts = if o.transact_time > 0 { o.transact_time } else { o.time };
    let exec  = parse_f64(&o.executed_qty);
    let quote = parse_f64(&o.cumulative_quote);
    let avg   = if exec > 0.0 { Some(quote / exec) } else { None };
    Order {
        order_id:        o.order_id.to_string(),
        client_order_id: Some(o.client_order_id),
        symbol:          o.symbol,
        side:            if o.side == "BUY" { OrderSide::Buy } else { OrderSide::Sell },
        order_type:      if o.order_type == "MARKET" { OrderType::Market } else { OrderType::Limit },
        price:           { let p = parse_f64(&o.price); if p > 0.0 { Some(p) } else { None } },
        qty:             parse_f64(&o.orig_qty),
        filled_qty:      exec,
        avg_fill_price:  avg,
        status: match o.status.as_str() {
            "NEW"              => OrderStatus::New,
            "PARTIALLY_FILLED" => OrderStatus::PartiallyFilled,
            "FILLED"           => OrderStatus::Filled,
            "CANCELED"         => OrderStatus::Cancelled,
            "REJECTED"         => OrderStatus::Rejected,
            "EXPIRED"          => OrderStatus::Expired,
            _                  => OrderStatus::New,
        },
        created_at: ms_to_dt(ts),
        updated_at: if o.update_time > 0 { Some(ms_to_dt(o.update_time)) } else { None },
    }
}

// ── Exchange impl ─────────────────────────────────────────────────────────────

#[async_trait]
impl hq_core::traits::Exchange for BinanceClient {
    fn name(&self) -> &'static str { "Binance" }
    fn environment(&self) -> &Environment { &self.config.environment }

    async fn get_ticker(&self, symbol: &str) -> Result<Ticker> {
        let t: Bn24h = self.get_pub("/api/v3/ticker/24hr", &format!("symbol={}", symbol)).await?;
        Ok(Ticker {
            symbol:           t.symbol,
            bid:              parse_f64(&t.bid_price),
            ask:              parse_f64(&t.ask_price),
            last:             parse_f64(&t.last_price),
            volume_24h:       parse_f64(&t.volume),
            price_change_pct: parse_f64(&t.change_pct),
            timestamp:        ms_to_dt(t.close_time),
        })
    }

    async fn get_order_book(&self, symbol: &str, depth: u32) -> Result<OrderBook> {
        let raw: BnDepth = self.get_pub("/api/v3/depth",
            &format!("symbol={}&limit={}", symbol, depth)).await?;
        let lv = |r: [String; 2]| Level { price: parse_f64(&r[0]), qty: parse_f64(&r[1]) };
        Ok(OrderBook {
            symbol: symbol.into(),
            bids: raw.bids.into_iter().map(lv).collect(),
            asks: raw.asks.into_iter().map(lv).collect(),
            timestamp: chrono::Utc::now(),
        })
    }

    async fn get_candles(&self, symbol: &str, interval: &str, limit: u32) -> Result<Vec<Candle>> {
        let raw: Vec<Vec<Value>> = self.get_pub("/api/v3/klines",
            &format!("symbol={}&interval={}&limit={}", symbol, interval, limit)).await?;
        Ok(raw.into_iter().map(|r| Candle {
            open_time: ms_to_dt(r[0].as_u64().unwrap_or(0)),
            open:   parse_f64(r[1].as_str().unwrap_or("0")),
            high:   parse_f64(r[2].as_str().unwrap_or("0")),
            low:    parse_f64(r[3].as_str().unwrap_or("0")),
            close:  parse_f64(r[4].as_str().unwrap_or("0")),
            volume: parse_f64(r[5].as_str().unwrap_or("0")),
        }).collect())
    }

    async fn get_account(&self) -> Result<AccountInfo> {
        let acc: BnAccount = self.get_signed("/api/v3/account", vec![]).await?;
        let balances = acc.balances.into_iter()
            .filter(|b| b.free != "0.00000000" || b.locked != "0.00000000")
            .map(|b| Balance {
                asset: b.asset, free: parse_f64(&b.free), locked: parse_f64(&b.locked),
            }).collect();
        Ok(AccountInfo { balances, can_trade: acc.can_trade,
                         can_withdraw: acc.can_withdraw, timestamp: ms_to_dt(acc.update_time) })
    }

    async fn place_order(&self, req: PlaceOrderRequest) -> Result<Order> {
        let mut p = vec![
            ("symbol",   req.symbol.clone()),
            ("side",     if req.side == OrderSide::Buy { "BUY".into() } else { "SELL".into() }),
            ("type",     if req.order_type == OrderType::Market { "MARKET".into() } else { "LIMIT".into() }),
            ("quantity", req.qty.to_string()),
        ];
        if let Some(price) = req.price {
            p.push(("price", price.to_string()));
            p.push(("timeInForce", "GTC".into()));
        }
        if let Some(cid) = req.client_order_id { p.push(("newClientOrderId", cid)); }
        let raw: BnOrder = self.post_signed("/api/v3/order", p).await?;
        Ok(map_bn_order(raw))
    }

    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<Order> {
        let raw: BnOrder = self.delete_signed("/api/v3/order", vec![
            ("symbol", symbol.into()), ("orderId", order_id.into()),
        ]).await?;
        Ok(map_bn_order(raw))
    }

    async fn get_order(&self, symbol: &str, order_id: &str) -> Result<Order> {
        let raw: BnOrder = self.get_signed("/api/v3/order", vec![
            ("symbol", symbol.into()), ("orderId", order_id.into()),
        ]).await?;
        Ok(map_bn_order(raw))
    }

    async fn get_open_orders(&self, symbol: Option<&str>) -> Result<Vec<Order>> {
        let mut p = vec![];
        if let Some(s) = symbol { p.push(("symbol", s.into())); }
        let raw: Vec<BnOrder> = self.get_signed("/api/v3/openOrders", p).await?;
        Ok(raw.into_iter().map(map_bn_order).collect())
    }

    async fn get_my_trades(&self, symbol: &str, limit: u32) -> Result<Vec<Trade>> {
        let raw: Vec<BnTrade> = self.get_signed("/api/v3/myTrades", vec![
            ("symbol", symbol.into()), ("limit", limit.to_string()),
        ]).await?;
        Ok(raw.into_iter().map(|t| Trade {
            trade_id:  t.id.to_string(),
            order_id:  t.order_id.to_string(),
            symbol:    t.symbol,
            side:      if t.is_buyer { OrderSide::Buy } else { OrderSide::Sell },
            price:     parse_f64(&t.price),
            qty:       parse_f64(&t.qty),
            fee:       parse_f64(&t.commission),
            fee_asset: t.commission_asset,
            timestamp: ms_to_dt(t.time),
        }).collect())
    }
}
