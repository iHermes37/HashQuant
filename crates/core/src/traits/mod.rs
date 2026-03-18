use async_trait::async_trait;
use crate::types::*;
use crate::error::CoreError;

pub type Result<T> = std::result::Result<T, CoreError>;

/// 所有交易所适配器必须实现的统一接口
#[async_trait]
pub trait Exchange: Send + Sync {
    fn name(&self) -> &'static str;
    fn environment(&self) -> &Environment;

    async fn get_ticker(&self, symbol: &str) -> Result<Ticker>;
    async fn get_order_book(&self, symbol: &str, depth: u32) -> Result<OrderBook>;
    async fn get_candles(&self, symbol: &str, interval: &str, limit: u32) -> Result<Vec<Candle>>;
    async fn get_account(&self) -> Result<AccountInfo>;
    async fn place_order(&self, req: PlaceOrderRequest) -> Result<Order>;
    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<Order>;
    async fn get_order(&self, symbol: &str, order_id: &str) -> Result<Order>;
    async fn get_open_orders(&self, symbol: Option<&str>) -> Result<Vec<Order>>;
    async fn get_my_trades(&self, symbol: &str, limit: u32) -> Result<Vec<Trade>>;
}
