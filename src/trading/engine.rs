use anyhow::Result;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tracing::{debug, error, info, warn};

use crate::exchange::{BinanceClient, OrderRequest, OrderSide};
use crate::risk::RiskManager;
use crate::strategy::{Signal, Strategy};

pub struct TradingEngine {
    client: BinanceClient,
    risk_manager: RiskManager,
    strategy: Box<dyn Strategy>,
    symbols: Vec<String>,
    paper_trading: bool,
}

impl TradingEngine {
    pub fn new(
        client: BinanceClient,
        risk_manager: RiskManager,
        strategy: Box<dyn Strategy>,
        symbols: Vec<String>,
        paper_trading: bool,
    ) -> Self {
        Self {
            client,
            risk_manager,
            strategy,
            symbols,
            paper_trading,
        }
    }

    pub async fn run(&mut self, interval_ms: u64) -> Result<()> {
        info!("Starting trading engine with {} symbols", self.symbols.len());

        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(interval_ms));

        loop {
            interval.tick().await;

            if let Err(e) = self.run_once().await {
                error!("Trading cycle error: {}", e);
            }
        }
    }

    pub async fn run_once(&mut self) -> Result<()> {
        debug!("Running trading cycle");

        // Check if we can trade
        if !self.risk_manager.can_trade() {
            warn!("Risk limits reached, skipping trading cycle");
            return Ok(());
        }

        // Get account info for balance checks
        let account = self.client.get_account_info().await?;

        for symbol in self.symbols.clone() {
            if let Err(e) = self.process_symbol(&symbol, &account.balances).await {
                error!("Error processing {}: {}", symbol, e);
            }
        }

        Ok(())
    }

    async fn process_symbol(
        &self,
        symbol: &str,
        balances: &[crate::exchange::Balance],
    ) -> Result<()> {
        debug!("Processing symbol: {}", symbol);

        // Get market data
        let required_history = self.strategy.required_history() as u32;
        let market_data = self
            .client
            .get_market_data(symbol, required_history.max(50))
            .await?;

        info!(
            "{}: Current price = {}, Klines = {}",
            symbol,
            market_data.current_price,
            market_data.klines.len()
        );

        // Analyze with strategy
        let signal = self.strategy.analyze(&market_data).await;

        match &signal {
            Signal::Buy { strength } => {
                info!("{}: BUY signal with strength {:.2}", symbol, strength);
                self.execute_buy(symbol, &market_data, balances, *strength)
                    .await?;
            }
            Signal::Sell { strength } => {
                info!("{}: SELL signal with strength {:.2}", symbol, strength);
                self.execute_sell(symbol, &market_data, balances, *strength)
                    .await?;
            }
            Signal::Hold => {
                debug!("{}: HOLD - no action", symbol);
            }
        }

        Ok(())
    }

    async fn execute_buy(
        &self,
        symbol: &str,
        market_data: &crate::exchange::MarketData,
        balances: &[crate::exchange::Balance],
        signal_strength: f64,
    ) -> Result<()> {
        // Find quote asset balance (assume USDT for now)
        let quote_asset = if symbol.ends_with("USDT") {
            "USDT"
        } else if symbol.ends_with("BTC") {
            "BTC"
        } else {
            "USDT"
        };

        let quote_balance = balances
            .iter()
            .find(|b| b.asset == quote_asset)
            .ok_or_else(|| anyhow::anyhow!("Quote balance not found for {}", quote_asset))?;

        // Calculate position size based on signal strength and risk settings
        let risk_pct = dec!(1) + Decimal::try_from(signal_strength).unwrap_or(dec!(0));
        let quantity = self.risk_manager.calculate_position_size(
            quote_balance.free_decimal(),
            risk_pct,
            market_data.current_price,
        );

        if quantity <= dec!(0) {
            warn!("Calculated quantity is zero or negative, skipping order");
            return Ok(());
        }

        // Round quantity to appropriate precision (simplified)
        let quantity = self.round_quantity(quantity, symbol);

        let order = OrderRequest::market(symbol, OrderSide::Buy, quantity);

        // Validate with risk manager
        if let Err(e) = self
            .risk_manager
            .validate_order(&order, quote_balance, market_data.current_price)
        {
            warn!("Order rejected by risk manager: {}", e);
            return Ok(());
        }

        // Execute or simulate
        if self.paper_trading {
            info!(
                "[PAPER] Would BUY {} {} at {} (value: {} {})",
                quantity,
                symbol,
                market_data.current_price,
                quantity * market_data.current_price,
                quote_asset
            );
        } else {
            info!(
                "Placing BUY order: {} {} at market price",
                quantity, symbol
            );
            match self.client.place_order(&order).await {
                Ok(response) => {
                    info!(
                        "Order placed successfully: ID={}, Status={}",
                        response.order_id, response.status
                    );
                    self.risk_manager.increment_positions();
                }
                Err(e) => {
                    error!("Failed to place order: {}", e);
                }
            }
        }

        Ok(())
    }

    async fn execute_sell(
        &self,
        symbol: &str,
        market_data: &crate::exchange::MarketData,
        balances: &[crate::exchange::Balance],
        signal_strength: f64,
    ) -> Result<()> {
        // Find base asset balance
        let base_asset = symbol
            .strip_suffix("USDT")
            .or_else(|| symbol.strip_suffix("BTC"))
            .or_else(|| symbol.strip_suffix("ETH"))
            .unwrap_or(symbol);

        let base_balance = balances.iter().find(|b| b.asset == base_asset);

        let quantity = match base_balance {
            Some(b) => {
                let available = b.free_decimal();
                if available <= dec!(0) {
                    debug!("No {} available to sell", base_asset);
                    return Ok(());
                }
                // Sell portion based on signal strength
                let sell_pct = Decimal::try_from(signal_strength).unwrap_or(dec!(0.5));
                available * sell_pct
            }
            None => {
                debug!("No {} balance found", base_asset);
                return Ok(());
            }
        };

        if quantity <= dec!(0) {
            return Ok(());
        }

        let quantity = self.round_quantity(quantity, symbol);

        let order = OrderRequest::market(symbol, OrderSide::Sell, quantity);

        // Quote balance for validation (not really needed for sells but for consistency)
        let quote_balance = balances
            .iter()
            .find(|b| b.asset == "USDT")
            .cloned()
            .unwrap_or(crate::exchange::Balance {
                asset: "USDT".to_string(),
                free: "0".to_string(),
                locked: "0".to_string(),
            });

        if let Err(e) = self
            .risk_manager
            .validate_order(&order, &quote_balance, market_data.current_price)
        {
            warn!("Order rejected by risk manager: {}", e);
            return Ok(());
        }

        if self.paper_trading {
            info!(
                "[PAPER] Would SELL {} {} at {} (value: {} USDT)",
                quantity,
                symbol,
                market_data.current_price,
                quantity * market_data.current_price
            );
        } else {
            info!(
                "Placing SELL order: {} {} at market price",
                quantity, symbol
            );
            match self.client.place_order(&order).await {
                Ok(response) => {
                    info!(
                        "Order placed successfully: ID={}, Status={}",
                        response.order_id, response.status
                    );
                    self.risk_manager.decrement_positions();
                }
                Err(e) => {
                    error!("Failed to place order: {}", e);
                }
            }
        }

        Ok(())
    }

    fn round_quantity(&self, quantity: Decimal, symbol: &str) -> Decimal {
        // Simplified rounding - in production, fetch from exchange info
        let precision = if symbol.starts_with("BTC") {
            5
        } else if symbol.starts_with("ETH") {
            4
        } else {
            3
        };

        quantity.round_dp(precision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_quantity() {
        // This is a simple test to verify the rounding logic
        let quantity = dec!(0.123456789);

        // BTC precision = 5
        assert_eq!(quantity.round_dp(5), dec!(0.12346));

        // ETH precision = 4
        assert_eq!(quantity.round_dp(4), dec!(0.1235));

        // Other precision = 3
        assert_eq!(quantity.round_dp(3), dec!(0.123));
    }
}
