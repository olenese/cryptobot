use async_trait::async_trait;
use rust_decimal::Decimal;

use crate::exchange::MarketData;

#[derive(Debug, Clone)]
pub enum Signal {
    Buy { strength: f64 },
    Sell { strength: f64 },
    Hold,
}

impl Signal {
    pub fn is_actionable(&self, min_strength: f64) -> bool {
        match self {
            Signal::Buy { strength } | Signal::Sell { strength } => *strength >= min_strength,
            Signal::Hold => false,
        }
    }

    pub fn strength(&self) -> f64 {
        match self {
            Signal::Buy { strength } | Signal::Sell { strength } => *strength,
            Signal::Hold => 0.0,
        }
    }
}

#[async_trait]
pub trait Strategy: Send + Sync {
    fn name(&self) -> &str;

    async fn analyze(&self, market_data: &MarketData) -> Signal;

    fn required_history(&self) -> usize;
}

pub fn calculate_sma(prices: &[Decimal], period: usize) -> Option<Decimal> {
    if prices.len() < period {
        return None;
    }

    let sum: Decimal = prices.iter().rev().take(period).sum();
    Some(sum / Decimal::from(period))
}

pub fn calculate_ema(prices: &[Decimal], period: usize) -> Option<Decimal> {
    if prices.len() < period {
        return None;
    }

    let multiplier = Decimal::from(2) / Decimal::from(period + 1);

    // Start with SMA for initial EMA
    let initial_sma = calculate_sma(&prices[..period], period)?;

    let mut ema = initial_sma;
    for price in prices.iter().skip(period) {
        ema = (*price - ema) * multiplier + ema;
    }

    Some(ema)
}

pub fn calculate_rsi(prices: &[Decimal], period: usize) -> Option<f64> {
    if prices.len() < period + 1 {
        return None;
    }

    let mut gains = Vec::new();
    let mut losses = Vec::new();

    for i in 1..prices.len() {
        let change = prices[i] - prices[i - 1];
        if change > Decimal::ZERO {
            gains.push(change);
            losses.push(Decimal::ZERO);
        } else {
            gains.push(Decimal::ZERO);
            losses.push(change.abs());
        }
    }

    // Calculate average gain and loss over the period
    let recent_gains: Vec<_> = gains.iter().rev().take(period).collect();
    let recent_losses: Vec<_> = losses.iter().rev().take(period).collect();

    let avg_gain: Decimal = recent_gains.iter().copied().sum::<Decimal>() / Decimal::from(period);
    let avg_loss: Decimal = recent_losses.iter().copied().sum::<Decimal>() / Decimal::from(period);

    if avg_loss == Decimal::ZERO {
        return Some(100.0);
    }

    let rs = avg_gain / avg_loss;
    let rs_f64: f64 = rs.try_into().unwrap_or(1.0);

    Some(100.0 - (100.0 / (1.0 + rs_f64)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_calculate_sma() {
        let prices = vec![dec!(10), dec!(11), dec!(12), dec!(13), dec!(14)];
        let sma = calculate_sma(&prices, 3);
        assert_eq!(sma, Some(dec!(13))); // (12 + 13 + 14) / 3 = 13
    }

    #[test]
    fn test_calculate_sma_insufficient_data() {
        let prices = vec![dec!(10), dec!(11)];
        let sma = calculate_sma(&prices, 3);
        assert!(sma.is_none());
    }

    #[test]
    fn test_signal_actionable() {
        assert!(Signal::Buy { strength: 0.8 }.is_actionable(0.6));
        assert!(!Signal::Buy { strength: 0.5 }.is_actionable(0.6));
        assert!(!Signal::Hold.is_actionable(0.0));
    }

    #[test]
    fn test_calculate_rsi() {
        // Create a simple uptrend
        let prices: Vec<Decimal> = (0..20).map(|i| Decimal::from(100 + i)).collect();
        let rsi = calculate_rsi(&prices, 14);
        assert!(rsi.is_some());
        assert!(rsi.unwrap() > 50.0); // Should be bullish
    }
}
