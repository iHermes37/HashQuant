use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

// ─── 枚举 ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderSide { Buy, Sell }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType { Limit, Market, StopLimit, StopMarket }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeInForce { GoodTillCancel, ImmediateOrCancel, FillOrKill }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    New, PartiallyFilled, Filled,
    Cancelled, Rejected, Expired, PendingCancel,
}

/// 运行环境
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Environment {
    /// 真实生产网
    Live,
    /// 交易所官方测试网 / 模拟盘
    Testnet,
    /// 本地内存模拟（单测 / 回测）
    Paper,
}

impl Default for Environment {
    fn default() -> Self { Self::Live }
}

// ─── 行情 ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticker {
    pub symbol:           String,
    pub bid:              f64,
    pub ask:              f64,
    pub last:             f64,
    pub volume_24h:       f64,
    pub price_change_pct: f64,
    pub timestamp:        DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Level {
    pub price: f64,
    pub qty:   f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub symbol:    String,
    pub bids:      Vec<Level>,
    pub asks:      Vec<Level>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    pub open_time: DateTime<Utc>,
    pub open:      f64,
    pub high:      f64,
    pub low:       f64,
    pub close:     f64,
    pub volume:    f64,
}

// ─── 账户 ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub asset:  String,
    pub free:   f64,
    pub locked: f64,
}

impl Balance {
    pub fn total(&self) -> f64 { self.free + self.locked }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub balances:     Vec<Balance>,
    pub can_trade:    bool,
    pub can_withdraw: bool,
    pub timestamp:    DateTime<Utc>,
}

// ─── 订单 ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceOrderRequest {
    pub symbol:          String,
    pub side:            OrderSide,
    pub order_type:      OrderType,
    pub price:           Option<f64>,
    pub qty:             f64,
    pub time_in_force:   Option<TimeInForce>,
    pub client_order_id: Option<String>,
}

impl PlaceOrderRequest {
    pub fn market(symbol: impl Into<String>, side: OrderSide, qty: f64) -> Self {
        Self {
            symbol: symbol.into(), side,
            order_type: OrderType::Market,
            price: None, qty,
            time_in_force: None,
            client_order_id: Some(Uuid::new_v4().to_string()),
        }
    }

    pub fn limit(symbol: impl Into<String>, side: OrderSide, qty: f64, price: f64) -> Self {
        Self {
            symbol: symbol.into(), side,
            order_type: OrderType::Limit,
            price: Some(price), qty,
            time_in_force: Some(TimeInForce::GoodTillCancel),
            client_order_id: Some(Uuid::new_v4().to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub order_id:        String,
    pub client_order_id: Option<String>,
    pub symbol:          String,
    pub side:            OrderSide,
    pub order_type:      OrderType,
    pub price:           Option<f64>,
    pub qty:             f64,
    pub filled_qty:      f64,
    pub avg_fill_price:  Option<f64>,
    pub status:          OrderStatus,
    pub created_at:      DateTime<Utc>,
    pub updated_at:      Option<DateTime<Utc>>,
}

impl Order {
    pub fn remaining_qty(&self) -> f64 { self.qty - self.filled_qty }
    pub fn is_active(&self) -> bool {
        matches!(self.status, OrderStatus::New | OrderStatus::PartiallyFilled)
    }
}

// ─── 成交 ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub trade_id:  String,
    pub order_id:  String,
    pub symbol:    String,
    pub side:      OrderSide,
    pub price:     f64,
    pub qty:       f64,
    pub fee:       f64,
    pub fee_asset: String,
    pub timestamp: DateTime<Utc>,
}

impl Trade {
    pub fn notional(&self) -> f64 { self.price * self.qty }
}

// ─── Polymarket 专用 ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolymarketMarket {
    pub condition_id: String,
    pub question:     String,
    pub description:  Option<String>,
    pub end_date:     Option<DateTime<Utc>>,
    pub active:       bool,
    pub tokens:       Vec<PolymarketToken>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolymarketToken {
    pub token_id: String,
    pub outcome:  String,
    pub price:    f64,
}
