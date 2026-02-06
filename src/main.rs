use anyhow::Result;
use clap::Parser;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use cryptobot::{
    config::{AppConfig, ExchangeCredentials},
    exchange::BinanceClient,
    risk::RiskManager,
    strategy::{SmaCrossoverStrategy, Strategy},
    trading::TradingEngine,
};

#[derive(Parser, Debug)]
#[command(name = "cryptobot")]
#[command(about = "A secure, high-performance crypto trading bot")]
struct Args {
    /// Use testnet environment (overrides .env setting)
    #[arg(long)]
    testnet: bool,

    /// Enable paper trading mode (no real orders)
    #[arg(long)]
    paper: bool,

    /// Path to configuration file
    #[arg(short, long, default_value = "config/default.toml")]
    config: String,

    /// Run once and exit (useful for testing)
    #[arg(long)]
    once: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cryptobot=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    info!("Starting Cryptobot...");

    // Load configuration
    let config = AppConfig::load_from_path(&args.config)?;
    info!("Configuration loaded from {}", args.config);

    // Load credentials from environment
    let mut credentials = ExchangeCredentials::from_env()?;

    // Override environment if --testnet flag is set
    if args.testnet {
        credentials.environment = cryptobot::Environment::Testnet;
        info!("Testnet mode enabled via CLI flag");
    }

    info!(
        "Environment: {:?}, Base URL: {}",
        credentials.environment,
        credentials.environment.base_url()
    );

    // Check if paper trading
    let paper_trading = args.paper || config.trading.paper_trading;
    if paper_trading {
        warn!("Paper trading mode enabled - no real orders will be placed");
    }

    // Initialize exchange client
    let client = BinanceClient::new(credentials.clone())?;

    // Test connection by fetching account info
    info!("Testing connection to Binance...");
    match client.get_account_info().await {
        Ok(account) => {
            info!("Connected successfully!");
            info!(
                "Account balances with value: {:?}",
                account
                    .balances
                    .iter()
                    .filter(|b| b.free.parse::<f64>().unwrap_or(0.0) > 0.0
                        || b.locked.parse::<f64>().unwrap_or(0.0) > 0.0)
                    .collect::<Vec<_>>()
            );
        }
        Err(e) => {
            tracing::error!("Failed to connect to Binance: {}", e);
            return Err(e.into());
        }
    }

    // Initialize risk manager
    let risk_manager = RiskManager::new(
        config.risk.max_position_pct,
        config.risk.max_daily_loss_pct,
        config.risk.max_open_positions,
    );

    // Initialize strategy
    let strategy: Box<dyn Strategy> = Box::new(SmaCrossoverStrategy::new(
        config.strategy.sma_crossover.short_period,
        config.strategy.sma_crossover.long_period,
        config.strategy.sma_crossover.min_signal_strength,
    ));

    info!("Using strategy: {}", strategy.name());

    // Initialize trading engine
    let mut engine = TradingEngine::new(
        client,
        risk_manager,
        strategy,
        config.exchange.symbols.clone(),
        paper_trading,
    );

    // Run trading engine
    if args.once {
        info!("Running single iteration (--once mode)");
        engine.run_once().await?;
    } else {
        info!("Starting trading loop...");
        engine.run(config.exchange.update_interval_ms).await?;
    }

    Ok(())
}
