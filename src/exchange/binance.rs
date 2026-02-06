use anyhow::{Context, Result};
use hmac::{Hmac, Mac};
use reqwest::Client;
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, instrument};

use crate::config::ExchangeCredentials;

use super::models::*;

type HmacSha256 = Hmac<Sha256>;

pub struct BinanceClient {
    client: Client,
    credentials: ExchangeCredentials,
    base_url: String,
}

impl BinanceClient {
    pub fn new(credentials: ExchangeCredentials) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        let base_url = credentials.environment.base_url().to_string();

        Ok(Self {
            client,
            credentials,
            base_url,
        })
    }

    fn timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    fn sign(&self, query: &str) -> String {
        let mut mac =
            HmacSha256::new_from_slice(self.credentials.secret_key.as_bytes()).unwrap();
        mac.update(query.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    fn build_signed_query(&self, params: &[(&str, String)]) -> String {
        let timestamp = Self::timestamp().to_string();
        let mut all_params: Vec<(&str, String)> = params.to_vec();
        all_params.push(("timestamp", timestamp));

        let query: String = all_params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");

        let signature = self.sign(&query);
        format!("{}&signature={}", query, signature)
    }

    #[instrument(skip(self))]
    pub async fn get_account_info(&self) -> Result<AccountInfo> {
        let query = self.build_signed_query(&[]);
        let url = format!("{}/api/v3/account?{}", self.base_url, query);

        debug!("Fetching account info");

        let response = self
            .client
            .get(&url)
            .header("X-MBX-APIKEY", &self.credentials.api_key)
            .send()
            .await
            .context("Failed to send account info request")?;

        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            anyhow::bail!("Account info request failed: {} - {}", status, text);
        }

        serde_json::from_str(&text).context("Failed to parse account info response")
    }

    #[instrument(skip(self))]
    pub async fn get_ticker_price(&self, symbol: &str) -> Result<TickerPrice> {
        let url = format!("{}/api/v3/ticker/price?symbol={}", self.base_url, symbol);

        debug!("Fetching ticker price for {}", symbol);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send ticker price request")?;

        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            anyhow::bail!("Ticker price request failed: {} - {}", status, text);
        }

        serde_json::from_str(&text).context("Failed to parse ticker price response")
    }

    #[instrument(skip(self))]
    pub async fn get_all_ticker_prices(&self) -> Result<Vec<TickerPrice>> {
        let url = format!("{}/api/v3/ticker/price", self.base_url);

        debug!("Fetching all ticker prices");

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send all ticker prices request")?;

        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            anyhow::bail!("All ticker prices request failed: {} - {}", status, text);
        }

        serde_json::from_str(&text).context("Failed to parse all ticker prices response")
    }

    #[instrument(skip(self))]
    pub async fn get_klines(
        &self,
        symbol: &str,
        interval: &str,
        limit: u32,
    ) -> Result<Vec<Kline>> {
        let url = format!(
            "{}/api/v3/klines?symbol={}&interval={}&limit={}",
            self.base_url, symbol, interval, limit
        );

        debug!("Fetching {} klines for {} at {} interval", limit, symbol, interval);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send klines request")?;

        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            anyhow::bail!("Klines request failed: {} - {}", status, text);
        }

        // Binance returns klines as arrays of arrays
        let raw: Vec<Vec<serde_json::Value>> =
            serde_json::from_str(&text).context("Failed to parse klines response")?;

        let klines = raw
            .into_iter()
            .map(|k| Kline {
                open_time: k[0].as_u64().unwrap_or(0),
                open: k[1].as_str().unwrap_or("0").to_string(),
                high: k[2].as_str().unwrap_or("0").to_string(),
                low: k[3].as_str().unwrap_or("0").to_string(),
                close: k[4].as_str().unwrap_or("0").to_string(),
                volume: k[5].as_str().unwrap_or("0").to_string(),
                close_time: k[6].as_u64().unwrap_or(0),
                quote_asset_volume: k[7].as_str().unwrap_or("0").to_string(),
                number_of_trades: k[8].as_u64().unwrap_or(0),
                taker_buy_base_asset_volume: k[9].as_str().unwrap_or("0").to_string(),
                taker_buy_quote_asset_volume: k[10].as_str().unwrap_or("0").to_string(),
            })
            .collect();

        Ok(klines)
    }

    #[instrument(skip(self))]
    pub async fn place_order(&self, order: &OrderRequest) -> Result<OrderResponse> {
        let mut params = vec![
            ("symbol", order.symbol.clone()),
            ("side", order.side.to_string()),
            ("type", order.order_type.to_string()),
            ("quantity", order.quantity.to_string()),
        ];

        if let Some(price) = &order.price {
            params.push(("price", price.to_string()));
        }

        if let Some(tif) = &order.time_in_force {
            params.push(("timeInForce", tif.to_string()));
        }

        if let Some(stop_price) = &order.stop_price {
            params.push(("stopPrice", stop_price.to_string()));
        }

        let query = self.build_signed_query(&params);
        let url = format!("{}/api/v3/order?{}", self.base_url, query);

        debug!("Placing order: {:?}", order);

        let response = self
            .client
            .post(&url)
            .header("X-MBX-APIKEY", &self.credentials.api_key)
            .send()
            .await
            .context("Failed to send order request")?;

        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            anyhow::bail!("Order request failed: {} - {}", status, text);
        }

        serde_json::from_str(&text).context("Failed to parse order response")
    }

    #[instrument(skip(self))]
    pub async fn get_open_orders(&self, symbol: Option<&str>) -> Result<Vec<OpenOrder>> {
        let params: Vec<(&str, String)> = if let Some(s) = symbol {
            vec![("symbol", s.to_string())]
        } else {
            vec![]
        };

        let query = self.build_signed_query(&params);
        let url = format!("{}/api/v3/openOrders?{}", self.base_url, query);

        debug!("Fetching open orders for {:?}", symbol);

        let response = self
            .client
            .get(&url)
            .header("X-MBX-APIKEY", &self.credentials.api_key)
            .send()
            .await
            .context("Failed to send open orders request")?;

        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            anyhow::bail!("Open orders request failed: {} - {}", status, text);
        }

        serde_json::from_str(&text).context("Failed to parse open orders response")
    }

    #[instrument(skip(self))]
    pub async fn cancel_order(&self, symbol: &str, order_id: u64) -> Result<CancelOrderResponse> {
        let params = vec![
            ("symbol", symbol.to_string()),
            ("orderId", order_id.to_string()),
        ];

        let query = self.build_signed_query(&params);
        let url = format!("{}/api/v3/order?{}", self.base_url, query);

        debug!("Cancelling order {} for {}", order_id, symbol);

        let response = self
            .client
            .delete(&url)
            .header("X-MBX-APIKEY", &self.credentials.api_key)
            .send()
            .await
            .context("Failed to send cancel order request")?;

        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            anyhow::bail!("Cancel order request failed: {} - {}", status, text);
        }

        serde_json::from_str(&text).context("Failed to parse cancel order response")
    }

    #[instrument(skip(self))]
    pub async fn get_exchange_info(&self) -> Result<ExchangeInfo> {
        let url = format!("{}/api/v3/exchangeInfo", self.base_url);

        debug!("Fetching exchange info");

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send exchange info request")?;

        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            anyhow::bail!("Exchange info request failed: {} - {}", status, text);
        }

        serde_json::from_str(&text).context("Failed to parse exchange info response")
    }

    pub async fn get_market_data(&self, symbol: &str, kline_limit: u32) -> Result<MarketData> {
        let ticker = self.get_ticker_price(symbol).await?;
        let klines = self.get_klines(symbol, "1h", kline_limit).await?;

        Ok(MarketData {
            symbol: symbol.to_string(),
            current_price: ticker.price_decimal(),
            klines,
            timestamp: Self::timestamp(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp() {
        let ts = BinanceClient::timestamp();
        assert!(ts > 1700000000000); // Should be after Nov 2023
    }
}
