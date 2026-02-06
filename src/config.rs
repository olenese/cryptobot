use anyhow::{Context, Result};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub exchange: ExchangeConfig,
    pub trading: TradingConfig,
    pub risk: RiskConfig,
    pub strategy: StrategyConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExchangeConfig {
    pub name: String,
    pub symbols: Vec<String>,
    pub update_interval_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TradingConfig {
    pub paper_trading: bool,
    pub default_order_type: String,
    pub slippage_tolerance: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RiskConfig {
    pub max_position_pct: Decimal,
    pub max_daily_loss_pct: Decimal,
    pub max_open_positions: u32,
    pub default_stop_loss_pct: Decimal,
    pub default_take_profit_pct: Decimal,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrategyConfig {
    pub default: String,
    pub sma_crossover: SmaCrossoverConfig,
    pub rsi: RsiConfig,
    pub grid: GridConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SmaCrossoverConfig {
    pub short_period: usize,
    pub long_period: usize,
    pub min_signal_strength: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RsiConfig {
    pub period: usize,
    pub oversold_threshold: f64,
    pub overbought_threshold: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GridConfig {
    pub grid_levels: u32,
    pub grid_spacing_pct: f64,
    pub order_size_pct: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub file_enabled: bool,
    pub file_path: String,
}

#[derive(Debug, Clone)]
pub struct ExchangeCredentials {
    pub api_key: String,
    pub secret_key: String,
    pub environment: Environment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Testnet,
    Mainnet,
}

impl Environment {
    pub fn base_url(&self) -> &'static str {
        match self {
            Environment::Testnet => "https://testnet.binance.vision",
            Environment::Mainnet => "https://api.binance.com",
        }
    }

    pub fn ws_url(&self) -> &'static str {
        match self {
            Environment::Testnet => "wss://testnet.binance.vision/ws",
            Environment::Mainnet => "wss://stream.binance.com:9443/ws",
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        Self::load_from_path("config/default.toml")
    }

    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let settings = config::Config::builder()
            .add_source(config::File::from(path.as_ref()))
            .build()
            .context("Failed to build configuration")?;

        settings
            .try_deserialize()
            .context("Failed to deserialize configuration")
    }
}

impl ExchangeCredentials {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let api_key = std::env::var("BINANCE_API_KEY")
            .context("BINANCE_API_KEY environment variable not set")?;

        let secret_key = std::env::var("BINANCE_SECRET_KEY")
            .context("BINANCE_SECRET_KEY environment variable not set")?;

        let env_str = std::env::var("BINANCE_ENVIRONMENT").unwrap_or_else(|_| "testnet".to_string());

        let environment = match env_str.to_lowercase().as_str() {
            "mainnet" | "production" | "prod" => Environment::Mainnet,
            _ => Environment::Testnet,
        };

        if environment == Environment::Mainnet {
            tracing::warn!("Running in MAINNET mode - real funds at risk!");
        }

        Ok(Self {
            api_key,
            secret_key,
            environment,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_urls() {
        assert_eq!(
            Environment::Testnet.base_url(),
            "https://testnet.binance.vision"
        );
        assert_eq!(Environment::Mainnet.base_url(), "https://api.binance.com");
    }
}
