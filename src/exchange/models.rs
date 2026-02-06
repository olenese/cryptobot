use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub maker_commission: i64,
    pub taker_commission: i64,
    pub buyer_commission: i64,
    pub seller_commission: i64,
    pub can_trade: bool,
    pub can_withdraw: bool,
    pub can_deposit: bool,
    pub update_time: u64,
    pub account_type: String,
    pub balances: Vec<Balance>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Balance {
    pub asset: String,
    pub free: String,
    pub locked: String,
}

impl Balance {
    pub fn free_decimal(&self) -> Decimal {
        self.free.parse().unwrap_or_default()
    }

    pub fn locked_decimal(&self) -> Decimal {
        self.locked.parse().unwrap_or_default()
    }

    pub fn total(&self) -> Decimal {
        self.free_decimal() + self.locked_decimal()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TickerPrice {
    pub symbol: String,
    pub price: String,
}

impl TickerPrice {
    pub fn price_decimal(&self) -> Decimal {
        self.price.parse().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kline {
    pub open_time: u64,
    pub open: String,
    pub high: String,
    pub low: String,
    pub close: String,
    pub volume: String,
    pub close_time: u64,
    pub quote_asset_volume: String,
    pub number_of_trades: u64,
    pub taker_buy_base_asset_volume: String,
    pub taker_buy_quote_asset_volume: String,
}

impl Kline {
    pub fn close_decimal(&self) -> Decimal {
        self.close.parse().unwrap_or_default()
    }

    pub fn open_decimal(&self) -> Decimal {
        self.open.parse().unwrap_or_default()
    }

    pub fn high_decimal(&self) -> Decimal {
        self.high.parse().unwrap_or_default()
    }

    pub fn low_decimal(&self) -> Decimal {
        self.low.parse().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderSide {
    Buy,
    Sell,
}

impl std::fmt::Display for OrderSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderSide::Buy => write!(f, "BUY"),
            OrderSide::Sell => write!(f, "SELL"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderType {
    Market,
    Limit,
    StopLoss,
    StopLossLimit,
    TakeProfit,
    TakeProfitLimit,
    LimitMaker,
}

impl std::fmt::Display for OrderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderType::Market => write!(f, "MARKET"),
            OrderType::Limit => write!(f, "LIMIT"),
            OrderType::StopLoss => write!(f, "STOP_LOSS"),
            OrderType::StopLossLimit => write!(f, "STOP_LOSS_LIMIT"),
            OrderType::TakeProfit => write!(f, "TAKE_PROFIT"),
            OrderType::TakeProfitLimit => write!(f, "TAKE_PROFIT_LIMIT"),
            OrderType::LimitMaker => write!(f, "LIMIT_MAKER"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TimeInForce {
    Gtc, // Good Till Cancel
    Ioc, // Immediate or Cancel
    Fok, // Fill or Kill
}

impl std::fmt::Display for TimeInForce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeInForce::Gtc => write!(f, "GTC"),
            TimeInForce::Ioc => write!(f, "IOC"),
            TimeInForce::Fok => write!(f, "FOK"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OrderRequest {
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    pub time_in_force: Option<TimeInForce>,
    pub stop_price: Option<Decimal>,
}

impl OrderRequest {
    pub fn market(symbol: &str, side: OrderSide, quantity: Decimal) -> Self {
        Self {
            symbol: symbol.to_string(),
            side,
            order_type: OrderType::Market,
            quantity,
            price: None,
            time_in_force: None,
            stop_price: None,
        }
    }

    pub fn limit(symbol: &str, side: OrderSide, quantity: Decimal, price: Decimal) -> Self {
        Self {
            symbol: symbol.to_string(),
            side,
            order_type: OrderType::Limit,
            quantity,
            price: Some(price),
            time_in_force: Some(TimeInForce::Gtc),
            stop_price: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderResponse {
    pub symbol: String,
    pub order_id: u64,
    pub client_order_id: String,
    pub transact_time: u64,
    pub price: String,
    pub orig_qty: String,
    pub executed_qty: String,
    pub status: String,
    pub time_in_force: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub side: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenOrder {
    pub symbol: String,
    pub order_id: u64,
    pub client_order_id: String,
    pub price: String,
    pub orig_qty: String,
    pub executed_qty: String,
    pub status: String,
    pub time_in_force: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub side: String,
    pub time: u64,
    pub update_time: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelOrderResponse {
    pub symbol: String,
    pub order_id: u64,
    pub client_order_id: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct MarketData {
    pub symbol: String,
    pub current_price: Decimal,
    pub klines: Vec<Kline>,
    pub timestamp: u64,
}

impl MarketData {
    pub fn close_prices(&self) -> Vec<Decimal> {
        self.klines.iter().map(|k| k.close_decimal()).collect()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct WsTickerUpdate {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "c")]
    pub close_price: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeInfo {
    pub timezone: String,
    pub server_time: u64,
    pub symbols: Vec<SymbolInfo>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolInfo {
    pub symbol: String,
    pub status: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub base_asset_precision: u32,
    pub quote_precision: u32,
}
