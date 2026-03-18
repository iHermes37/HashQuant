use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use chrono::Utc;
use hq_core::{traits::Result, types::*, error::CoreError};

#[derive(Debug, Default)]
struct State {
    tickers:   HashMap<String, Ticker>,
    orders:    HashMap<String, Order>,
    trades:    Vec<Trade>,
    balances:  HashMap<String, Balance>,
    fee_rate:  f64,   // 百分比，如 0.1 表示 0.1%
    trade_seq: u64,
}

impl State {
    fn new(fee_rate: f64) -> Self { Self { fee_rate, ..Default::default() } }

    fn next_trade_id(&mut self) -> String {
        self.trade_seq += 1;
        format!("MOCK-T-{:06}", self.trade_seq)
    }

    fn base_quote(symbol: &str) -> (&str, &str) {
        // 支持 BTC-USDT / BTCUSDT 两种格式
        if let Some(pos) = symbol.find('-') {
            (&symbol[..pos], &symbol[pos + 1..])
        } else if symbol.len() > 4 {
            (&symbol[..symbol.len() - 4], &symbol[symbol.len() - 4..])
        } else {
            (symbol, "USDT")
        }
    }

    fn lock_funds(&mut self, order: &Order) -> Result<()> {
        let (base, quote) = Self::base_quote(&order.symbol);
        let (asset, amount) = match order.side {
            // 锁仓包含预估手续费（quote * fee_rate/100），避免成交后余额不足
            OrderSide::Buy  => {
                let notional = order.qty * order.price.unwrap_or(0.0);
                let est_fee  = notional * self.fee_rate / 100.0;
                (quote.to_string(), notional + est_fee)
            }
            OrderSide::Sell => (base.to_string(), order.qty),
        };
        let bal = self.balances.entry(asset.clone())
            .or_insert(Balance { asset: asset.clone(), free: 0.0, locked: 0.0 });
        if bal.free < amount {
            return Err(CoreError::InsufficientBalance {
                required:  format!("{:.8} {}", amount, asset),
                available: format!("{:.8} {}", bal.free, asset),
            });
        }
        bal.free   -= amount;
        bal.locked += amount;
        Ok(())
    }

    fn try_fill(&mut self, order_id: &str) -> bool {
        let order = match self.orders.get(order_id) {
            Some(o) if o.is_active() => o.clone(),
            _ => return false,
        };
        let ticker = match self.tickers.get(&order.symbol) {
            Some(t) => t.clone(),
            None    => return false,
        };

        let fill_price = match order.order_type {
            OrderType::Market => match order.side {
                OrderSide::Buy  => ticker.ask,
                OrderSide::Sell => ticker.bid,
            },
            OrderType::Limit => {
                let lp = match order.price { Some(p) => p, None => return false };
                match order.side {
                    OrderSide::Buy  if lp >= ticker.ask => ticker.ask,
                    OrderSide::Sell if lp <= ticker.bid => ticker.bid,
                    _ => return false,
                }
            }
            _ => return false,
        };

        let fill_qty = order.remaining_qty();
        let fee = fill_qty * fill_price * self.fee_rate / 100.0;

        // 更新订单
        let o = self.orders.get_mut(order_id).unwrap();
        o.filled_qty     = o.qty;
        o.avg_fill_price = Some(fill_price);
        o.status         = OrderStatus::Filled;
        o.updated_at     = Some(Utc::now());

        let (base, quote) = Self::base_quote(&order.symbol);
        // 释放锁仓并结算
        match order.side {
            OrderSide::Buy => {
                // 成交：清空 quote（USDT）的全部锁仓（含预估手续费），避免浮点残留
                if let Some(b) = self.balances.get_mut(quote) {
                    b.locked = 0.0;
                }
                // 收到完整的 base（BTC），实际手续费已从锁仓中扣除
                self.balances.entry(base.to_string())
                    .or_insert(Balance { asset: base.to_string(), free: 0.0, locked: 0.0 })
                    .free += fill_qty;
            }
            OrderSide::Sell => {
                if let Some(b) = self.balances.get_mut(base) {
                    b.locked = (b.locked - fill_qty).max(0.0);
                }
                // 收到 quote（USDT），扣除 fee
                let recv = fill_qty * fill_price - fee;
                self.balances.entry(quote.to_string())
                    .or_insert(Balance { asset: quote.to_string(), free: 0.0, locked: 0.0 })
                    .free += recv;
            }
        }

        let tid = self.next_trade_id();
        self.trades.push(Trade {
            trade_id:  tid,
            order_id:  order_id.into(),
            symbol:    order.symbol.clone(),
            side:      order.side.clone(),
            price:     fill_price,
            qty:       fill_qty,
            fee,
            fee_asset: quote.to_string(),
            timestamp: Utc::now(),
        });
        true
    }

    fn match_pending(&mut self) {
        let ids: Vec<String> = self.orders.values()
            .filter(|o| o.is_active())
            .map(|o| o.order_id.clone())
            .collect();
        for id in ids { self.try_fill(&id); }
    }
}

// ── 公开 API ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct MockExchange {
    state: Arc<Mutex<State>>,
}

impl MockExchange {
    /// `fee_rate` 单位为百分比，如 `0.1` = 0.1%
    pub fn new(fee_rate: f64) -> Self {
        Self { state: Arc::new(Mutex::new(State::new(fee_rate))) }
    }

    /// 默认 0.1% 手续费
    pub fn default_fees() -> Self { Self::new(0.1) }

    /// 初始化余额（测试 setup）
    pub fn seed_balance(&self, asset: &str, amount: f64) {
        let mut s = self.state.lock().unwrap();
        s.balances.insert(asset.into(), Balance { asset: asset.into(), free: amount, locked: 0.0 });
    }

    /// 推送行情并触发挂单撮合
    pub fn set_ticker(&self, ticker: Ticker) {
        let mut s = self.state.lock().unwrap();
        s.tickers.insert(ticker.symbol.clone(), ticker);
        s.match_pending();
    }

    /// 批量推送行情
    pub fn set_tickers(&self, tickers: Vec<Ticker>) {
        let mut s = self.state.lock().unwrap();
        for t in tickers { s.tickers.insert(t.symbol.clone(), t); }
        s.match_pending();
    }

    pub fn all_trades(&self)  -> Vec<Trade> { self.state.lock().unwrap().trades.clone() }
    pub fn all_orders(&self)  -> Vec<Order> { self.state.lock().unwrap().orders.values().cloned().collect() }
}

// ── Exchange impl ─────────────────────────────────────────────────────────────

#[async_trait]
impl hq_core::traits::Exchange for MockExchange {
    fn name(&self) -> &'static str { "Mock" }
    fn environment(&self) -> &Environment { &Environment::Paper }

    async fn get_ticker(&self, symbol: &str) -> Result<Ticker> {
        self.state.lock().unwrap().tickers.get(symbol).cloned()
            .ok_or_else(|| CoreError::Parse(format!("Mock: 未找到行情 {}", symbol)))
    }

    async fn get_order_book(&self, symbol: &str, depth: u32) -> Result<OrderBook> {
        let s = self.state.lock().unwrap();
        let t = s.tickers.get(symbol)
            .ok_or_else(|| CoreError::Parse(format!("Mock: 未找到行情 {}", symbol)))?;
        let spread = t.ask - t.bid;
        let tick   = spread / 4.0;
        let bids = (0..depth.min(5) as i64).map(|i| Level {
            price: t.bid - tick * i as f64, qty: 1.0 + i as f64,
        }).collect();
        let asks = (0..depth.min(5) as i64).map(|i| Level {
            price: t.ask + tick * i as f64, qty: 1.0 + i as f64,
        }).collect();
        Ok(OrderBook { symbol: symbol.into(), bids, asks, timestamp: t.timestamp })
    }

    async fn get_candles(&self, _symbol: &str, _interval: &str, _limit: u32) -> Result<Vec<Candle>> {
        Ok(vec![]) // 回测 K 线由 datafeed crate 提供
    }

    async fn get_account(&self) -> Result<AccountInfo> {
        let s = self.state.lock().unwrap();
        Ok(AccountInfo {
            balances:     s.balances.values().cloned().collect(),
            can_trade:    true,
            can_withdraw: true,
            timestamp:    Utc::now(),
        })
    }

    async fn place_order(&self, req: PlaceOrderRequest) -> Result<Order> {
        let order_id = format!("MOCK-O-{}", Uuid::new_v4().simple());
        let mut s = self.state.lock().unwrap();

        // Market order 用 ask/bid 估算锁仓价格
        let lock_price = match req.order_type {
            OrderType::Market => s.tickers.get(&req.symbol)
                .map(|t| match req.side { OrderSide::Buy => t.ask, OrderSide::Sell => t.bid })
                .unwrap_or(req.price.unwrap_or(0.0)),
            _ => req.price.unwrap_or(0.0),
        };

        let order = Order {
            order_id:        order_id.clone(),
            client_order_id: req.client_order_id.clone(),
            symbol:          req.symbol.clone(),
            side:            req.side.clone(),
            order_type:      req.order_type.clone(),
            price:           req.price,
            qty:             req.qty,
            filled_qty:      0.0,
            avg_fill_price:  None,
            status:          OrderStatus::New,
            created_at:      Utc::now(),
            updated_at:      None,
        };

        // 用实际锁仓价格检查余额
        let lock_order = Order { price: Some(lock_price), ..order.clone() };
        s.lock_funds(&lock_order)?;

        s.orders.insert(order_id.clone(), order);
        s.try_fill(&order_id);
        Ok(s.orders[&order_id].clone())
    }

    async fn cancel_order(&self, _symbol: &str, order_id: &str) -> Result<Order> {
        let mut s = self.state.lock().unwrap();
        let order = s.orders.get(order_id)
            .ok_or_else(|| CoreError::OrderNotFound(order_id.into()))?.clone();
        if !order.is_active() {
            return Err(CoreError::Api {
                exchange: "Mock".into(), code: -1,
                message: format!("订单 {} 已处于终态 {:?}", order_id, order.status),
            });
        }
        // 释放锁仓：直接把该资产全部 locked 退回 free
        // 避免因手续费估算与实际成交费用的浮点差值导致 locked 无法归零
        let (base, quote) = State::base_quote(&order.symbol);
        let asset = match order.side {
            OrderSide::Buy  => quote.to_string(),
            OrderSide::Sell => base.to_string(),
        };
        if let Some(b) = s.balances.get_mut(&asset) {
            b.free  += b.locked;
            b.locked = 0.0;
        }
        let o = s.orders.get_mut(order_id).unwrap();
        o.status     = OrderStatus::Cancelled;
        o.updated_at = Some(Utc::now());
        Ok(o.clone())
    }

    async fn get_order(&self, _symbol: &str, order_id: &str) -> Result<Order> {
        self.state.lock().unwrap().orders.get(order_id).cloned()
            .ok_or_else(|| CoreError::OrderNotFound(order_id.into()))
    }

    async fn get_open_orders(&self, symbol: Option<&str>) -> Result<Vec<Order>> {
        let s = self.state.lock().unwrap();
        Ok(s.orders.values()
            .filter(|o| o.is_active())
            .filter(|o| symbol.map_or(true, |sym| o.symbol == sym))
            .cloned().collect())
    }

    async fn get_my_trades(&self, symbol: &str, limit: u32) -> Result<Vec<Trade>> {
        let s = self.state.lock().unwrap();
        Ok(s.trades.iter().filter(|t| t.symbol == symbol)
            .rev().take(limit as usize).cloned().collect())
    }
}

// ── 单元测试 ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use hq_core::traits::Exchange; // trait 方法必须引入作用域才能调用

    fn ticker(symbol: &str, bid: f64, ask: f64) -> Ticker {
        Ticker { symbol: symbol.into(), bid, ask, last: (bid + ask) / 2.0,
                 volume_24h: 1000.0, price_change_pct: 0.0, timestamp: Utc::now() }
    }

    #[tokio::test]
    async fn market_buy_fills_immediately() {
        let ex = MockExchange::default_fees();
        ex.seed_balance("USDT", 10000.0);
        ex.set_ticker(ticker("BTC-USDT", 29900.0, 30000.0));
        let o = ex.place_order(PlaceOrderRequest::market("BTC-USDT", OrderSide::Buy, 0.1)).await.unwrap();
        assert_eq!(o.status, OrderStatus::Filled);
        assert_eq!(o.filled_qty, 0.1);
        assert_eq!(o.avg_fill_price, Some(30000.0));
    }

    #[tokio::test]
    async fn limit_buy_pending_then_fills_on_drop() {
        let ex = MockExchange::default_fees();
        ex.seed_balance("USDT", 10000.0);
        ex.set_ticker(ticker("BTC-USDT", 30100.0, 30200.0));
        let o = ex.place_order(PlaceOrderRequest::limit("BTC-USDT", OrderSide::Buy, 0.1, 29500.0)).await.unwrap();
        assert_eq!(o.status, OrderStatus::New);
        ex.set_ticker(ticker("BTC-USDT", 29300.0, 29400.0));
        let filled = ex.get_order("", &o.order_id).await.unwrap();
        assert_eq!(filled.status, OrderStatus::Filled);
    }

    #[tokio::test]
    async fn cancel_releases_funds() {
        let ex = MockExchange::default_fees();
        ex.seed_balance("USDT", 10000.0);
        ex.set_ticker(ticker("BTC-USDT", 30100.0, 30200.0));
        let o = ex.place_order(PlaceOrderRequest::limit("BTC-USDT", OrderSide::Buy, 0.1, 28000.0)).await.unwrap();
        let acc_before = ex.get_account().await.unwrap();
        let locked = acc_before.balances.iter().find(|b| b.asset == "USDT").unwrap().locked;
        assert!(locked > 0.0, "下单后应有锁仓");
        ex.cancel_order("", &o.order_id).await.unwrap();
        let acc_after = ex.get_account().await.unwrap();
        let usdt = acc_after.balances.iter().find(|b| b.asset == "USDT").unwrap();
        assert_eq!(usdt.locked, 0.0);
        assert_eq!(usdt.free, 10000.0);
    }

    #[tokio::test]
    async fn insufficient_balance_rejected() {
        let ex = MockExchange::default_fees();
        ex.seed_balance("USDT", 100.0);
        ex.set_ticker(ticker("BTC-USDT", 30100.0, 30200.0));
        let result = ex.place_order(PlaceOrderRequest::limit("BTC-USDT", OrderSide::Buy, 0.1, 30000.0)).await;
        assert!(matches!(result, Err(CoreError::InsufficientBalance { .. })));
    }

    #[tokio::test]
    async fn limit_sell_fills_on_price_rise() {
        let ex = MockExchange::default_fees();
        ex.seed_balance("BTC", 1.0);
        ex.set_ticker(ticker("BTC-USDT", 29900.0, 30000.0));
        let o = ex.place_order(PlaceOrderRequest::limit("BTC-USDT", OrderSide::Sell, 1.0, 31000.0)).await.unwrap();
        assert_eq!(o.status, OrderStatus::New);
        ex.set_ticker(ticker("BTC-USDT", 31100.0, 31200.0));
        let filled = ex.get_order("", &o.order_id).await.unwrap();
        assert_eq!(filled.status, OrderStatus::Filled);
    }
}
