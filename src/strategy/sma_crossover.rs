use async_trait::async_trait;
use tracing::debug;

use crate::exchange::MarketData;

use super::r#trait::{calculate_sma, Signal, Strategy};

pub struct SmaCrossoverStrategy {
    short_period: usize,
    long_period: usize,
    min_signal_strength: f64,
}

impl SmaCrossoverStrategy {
    pub fn new(short_period: usize, long_period: usize, min_signal_strength: f64) -> Self {
        assert!(
            short_period < long_period,
            "Short period must be less than long period"
        );

        Self {
            short_period,
            long_period,
            min_signal_strength,
        }
    }
}

#[async_trait]
impl Strategy for SmaCrossoverStrategy {
    fn name(&self) -> &str {
        "SMA Crossover"
    }

    async fn analyze(&self, market_data: &MarketData) -> Signal {
        let prices = market_data.close_prices();

        if prices.len() < self.long_period + 1 {
            debug!(
                "Insufficient data for SMA analysis: have {}, need {}",
                prices.len(),
                self.long_period + 1
            );
            return Signal::Hold;
        }

        // Calculate current SMAs
        let short_sma = match calculate_sma(&prices, self.short_period) {
            Some(v) => v,
            None => return Signal::Hold,
        };

        let long_sma = match calculate_sma(&prices, self.long_period) {
            Some(v) => v,
            None => return Signal::Hold,
        };

        // Calculate previous SMAs (one candle back)
        let prev_prices = &prices[..prices.len() - 1];
        let prev_short_sma = match calculate_sma(prev_prices, self.short_period) {
            Some(v) => v,
            None => return Signal::Hold,
        };

        let prev_long_sma = match calculate_sma(prev_prices, self.long_period) {
            Some(v) => v,
            None => return Signal::Hold,
        };

        debug!(
            "SMA Analysis - Short: {} -> {}, Long: {} -> {}",
            prev_short_sma, short_sma, prev_long_sma, long_sma
        );

        // Detect crossover
        let was_below = prev_short_sma < prev_long_sma;
        let is_above = short_sma > long_sma;
        let was_above = prev_short_sma > prev_long_sma;
        let is_below = short_sma < long_sma;

        // Calculate signal strength based on the separation between SMAs
        let separation = if long_sma != rust_decimal::Decimal::ZERO {
            let sep: f64 = ((short_sma - long_sma).abs() / long_sma)
                .try_into()
                .unwrap_or(0.0);
            (sep * 100.0).min(1.0) // Normalize to 0-1 range
        } else {
            0.0
        };

        // Golden cross: short SMA crosses above long SMA (bullish)
        if was_below && is_above {
            let strength = (0.5 + separation).min(1.0);
            debug!("Golden cross detected! Strength: {}", strength);

            if strength >= self.min_signal_strength {
                return Signal::Buy { strength };
            }
        }

        // Death cross: short SMA crosses below long SMA (bearish)
        if was_above && is_below {
            let strength = (0.5 + separation).min(1.0);
            debug!("Death cross detected! Strength: {}", strength);

            if strength >= self.min_signal_strength {
                return Signal::Sell { strength };
            }
        }

        Signal::Hold
    }

    fn required_history(&self) -> usize {
        self.long_period + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exchange::Kline;
    use rust_decimal_macros::dec;

    fn create_market_data(close_prices: Vec<&str>) -> MarketData {
        let klines = close_prices
            .into_iter()
            .enumerate()
            .map(|(i, price)| Kline {
                open_time: i as u64 * 3600000,
                open: price.to_string(),
                high: price.to_string(),
                low: price.to_string(),
                close: price.to_string(),
                volume: "100".to_string(),
                close_time: (i as u64 + 1) * 3600000,
                quote_asset_volume: "10000".to_string(),
                number_of_trades: 100,
                taker_buy_base_asset_volume: "50".to_string(),
                taker_buy_quote_asset_volume: "5000".to_string(),
            })
            .collect();

        MarketData {
            symbol: "BTCUSDT".to_string(),
            current_price: dec!(100),
            klines,
            timestamp: 0,
        }
    }

    #[tokio::test]
    async fn test_golden_cross() {
        let strategy = SmaCrossoverStrategy::new(2, 4, 0.0);

        // Create data where short MA crosses above long MA
        // Prices: 10, 10, 10, 10, 12, 14 (short MA rising fast)
        let market_data = create_market_data(vec!["10", "10", "10", "10", "12", "14"]);

        let signal = strategy.analyze(&market_data).await;
        assert!(matches!(signal, Signal::Buy { .. }));
    }

    #[tokio::test]
    async fn test_death_cross() {
        let strategy = SmaCrossoverStrategy::new(2, 4, 0.0);

        // Create data where short MA crosses below long MA
        // Prices: 14, 14, 14, 14, 12, 10 (short MA falling fast)
        let market_data = create_market_data(vec!["14", "14", "14", "14", "12", "10"]);

        let signal = strategy.analyze(&market_data).await;
        assert!(matches!(signal, Signal::Sell { .. }));
    }

    #[tokio::test]
    async fn test_hold_signal() {
        let strategy = SmaCrossoverStrategy::new(2, 4, 0.0);

        // Create flat data - no crossover
        let market_data = create_market_data(vec!["10", "10", "10", "10", "10", "10"]);

        let signal = strategy.analyze(&market_data).await;
        assert!(matches!(signal, Signal::Hold));
    }

    #[tokio::test]
    async fn test_insufficient_data() {
        let strategy = SmaCrossoverStrategy::new(2, 4, 0.0);

        // Not enough data points
        let market_data = create_market_data(vec!["10", "11", "12"]);

        let signal = strategy.analyze(&market_data).await;
        assert!(matches!(signal, Signal::Hold));
    }
}
