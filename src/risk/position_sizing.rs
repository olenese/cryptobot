use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::RwLock;
use thiserror::Error;
use tracing::{debug, warn};

use crate::exchange::{Balance, OrderRequest, OrderSide};

#[derive(Error, Debug)]
pub enum RiskError {
    #[error("Position size {requested} exceeds maximum allowed {max_allowed} ({max_pct}% of balance)")]
    PositionTooLarge {
        requested: Decimal,
        max_allowed: Decimal,
        max_pct: Decimal,
    },

    #[error("Insufficient balance: have {available}, need {required}")]
    InsufficientBalance { available: Decimal, required: Decimal },

    #[error("Daily loss limit exceeded: current loss {current_loss}% exceeds max {max_loss}%")]
    DailyLossExceeded {
        current_loss: Decimal,
        max_loss: Decimal,
    },

    #[error("Maximum open positions ({max}) reached")]
    MaxPositionsReached { max: u32 },

    #[error("Invalid order: {reason}")]
    InvalidOrder { reason: String },
}

pub struct RiskManager {
    max_position_pct: Decimal,
    max_daily_loss_pct: Decimal,
    max_open_positions: u32,
    current_daily_loss_pct: RwLock<Decimal>,
    current_open_positions: AtomicU32,
}

impl RiskManager {
    pub fn new(max_position_pct: Decimal, max_daily_loss_pct: Decimal, max_open_positions: u32) -> Self {
        Self {
            max_position_pct,
            max_daily_loss_pct,
            max_open_positions,
            current_daily_loss_pct: RwLock::new(dec!(0)),
            current_open_positions: AtomicU32::new(0),
        }
    }

    pub fn validate_order(
        &self,
        order: &OrderRequest,
        quote_balance: &Balance,
        current_price: Decimal,
    ) -> Result<(), RiskError> {
        // Check daily loss limit
        {
            let daily_loss = self.current_daily_loss_pct.read().unwrap();
            if *daily_loss >= self.max_daily_loss_pct {
                return Err(RiskError::DailyLossExceeded {
                    current_loss: *daily_loss,
                    max_loss: self.max_daily_loss_pct,
                });
            }
        }

        // Check open positions limit
        let open_positions = self.current_open_positions.load(Ordering::SeqCst);
        if open_positions >= self.max_open_positions {
            return Err(RiskError::MaxPositionsReached {
                max: self.max_open_positions,
            });
        }

        // For buy orders, check if we have sufficient quote balance
        if matches!(order.side, OrderSide::Buy) {
            let order_value = order.quantity * current_price;
            let available = quote_balance.free_decimal();
            let max_position_value = available * self.max_position_pct / dec!(100);

            debug!(
                "Order validation: value={}, available={}, max_allowed={}",
                order_value, available, max_position_value
            );

            if order_value > max_position_value {
                return Err(RiskError::PositionTooLarge {
                    requested: order_value,
                    max_allowed: max_position_value,
                    max_pct: self.max_position_pct,
                });
            }

            if order_value > available {
                return Err(RiskError::InsufficientBalance {
                    available,
                    required: order_value,
                });
            }
        }

        // Validate order has positive quantity
        if order.quantity <= dec!(0) {
            return Err(RiskError::InvalidOrder {
                reason: "Quantity must be positive".to_string(),
            });
        }

        // Validate limit orders have a price
        if matches!(order.order_type, crate::exchange::OrderType::Limit) && order.price.is_none() {
            return Err(RiskError::InvalidOrder {
                reason: "Limit orders must have a price".to_string(),
            });
        }

        Ok(())
    }

    pub fn calculate_position_size(
        &self,
        balance: Decimal,
        risk_pct: Decimal,
        price: Decimal,
    ) -> Decimal {
        let effective_risk_pct = risk_pct.min(self.max_position_pct);
        let position_value = balance * effective_risk_pct / dec!(100);
        let quantity = position_value / price;

        debug!(
            "Position size calculation: balance={}, risk_pct={}, price={}, quantity={}",
            balance, effective_risk_pct, price, quantity
        );

        quantity
    }

    pub fn record_trade_result(&self, pnl_pct: Decimal) {
        let mut daily_loss = self.current_daily_loss_pct.write().unwrap();

        if pnl_pct < dec!(0) {
            *daily_loss += pnl_pct.abs();
            warn!("Trade loss recorded: {}%. Total daily loss: {}%", pnl_pct, *daily_loss);
        }
    }

    pub fn increment_positions(&self) {
        self.current_open_positions.fetch_add(1, Ordering::SeqCst);
    }

    pub fn decrement_positions(&self) {
        self.current_open_positions.fetch_sub(1, Ordering::SeqCst);
    }

    pub fn reset_daily_stats(&self) {
        let mut daily_loss = self.current_daily_loss_pct.write().unwrap();
        *daily_loss = dec!(0);
        debug!("Daily stats reset");
    }

    pub fn current_daily_loss(&self) -> Decimal {
        *self.current_daily_loss_pct.read().unwrap()
    }

    pub fn open_positions_count(&self) -> u32 {
        self.current_open_positions.load(Ordering::SeqCst)
    }

    pub fn can_trade(&self) -> bool {
        let daily_loss = self.current_daily_loss_pct.read().unwrap();
        let positions = self.current_open_positions.load(Ordering::SeqCst);

        *daily_loss < self.max_daily_loss_pct && positions < self.max_open_positions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_balance(free: &str) -> Balance {
        Balance {
            asset: "USDT".to_string(),
            free: free.to_string(),
            locked: "0".to_string(),
        }
    }

    #[test]
    fn test_position_size_calculation() {
        let rm = RiskManager::new(dec!(2), dec!(5), 3);

        let size = rm.calculate_position_size(dec!(1000), dec!(2), dec!(50));
        assert_eq!(size, dec!(0.4)); // 2% of 1000 = 20, 20/50 = 0.4
    }

    #[test]
    fn test_position_size_capped_at_max() {
        let rm = RiskManager::new(dec!(2), dec!(5), 3);

        // Request 10% but max is 2%
        let size = rm.calculate_position_size(dec!(1000), dec!(10), dec!(50));
        assert_eq!(size, dec!(0.4)); // Capped at 2%
    }

    #[test]
    fn test_validate_order_too_large() {
        let rm = RiskManager::new(dec!(2), dec!(5), 3);
        let balance = create_test_balance("1000");

        let order = OrderRequest::market("BTCUSDT", OrderSide::Buy, dec!(1));
        let result = rm.validate_order(&order, &balance, dec!(50000));

        assert!(matches!(result, Err(RiskError::PositionTooLarge { .. })));
    }

    #[test]
    fn test_validate_order_valid() {
        let rm = RiskManager::new(dec!(2), dec!(5), 3);
        let balance = create_test_balance("1000");

        // 2% of 1000 = 20 USDT max, order is 0.0004 * 50000 = 20 USDT
        let order = OrderRequest::market("BTCUSDT", OrderSide::Buy, dec!(0.0004));
        let result = rm.validate_order(&order, &balance, dec!(50000));

        assert!(result.is_ok());
    }

    #[test]
    fn test_daily_loss_tracking() {
        let rm = RiskManager::new(dec!(2), dec!(5), 3);

        assert!(rm.can_trade());

        rm.record_trade_result(dec!(-3));
        assert!(rm.can_trade());
        assert_eq!(rm.current_daily_loss(), dec!(3));

        rm.record_trade_result(dec!(-3));
        assert!(!rm.can_trade());
        assert_eq!(rm.current_daily_loss(), dec!(6));

        rm.reset_daily_stats();
        assert!(rm.can_trade());
    }

    #[test]
    fn test_position_count_tracking() {
        let rm = RiskManager::new(dec!(2), dec!(5), 2);

        assert_eq!(rm.open_positions_count(), 0);
        assert!(rm.can_trade());

        rm.increment_positions();
        assert_eq!(rm.open_positions_count(), 1);
        assert!(rm.can_trade());

        rm.increment_positions();
        assert_eq!(rm.open_positions_count(), 2);
        assert!(!rm.can_trade());

        rm.decrement_positions();
        assert!(rm.can_trade());
    }
}
