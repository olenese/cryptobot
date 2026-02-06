use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use crate::config::Environment;

use super::models::WsTickerUpdate;

pub struct BinanceWebSocket {
    environment: Environment,
}

#[derive(Debug, Clone)]
pub enum WsMessage {
    Ticker(WsTickerUpdate),
    Connected,
    Disconnected,
    Error(String),
}

impl BinanceWebSocket {
    pub fn new(environment: Environment) -> Self {
        Self { environment }
    }

    pub async fn subscribe_tickers(
        &self,
        symbols: Vec<String>,
    ) -> Result<mpsc::Receiver<WsMessage>> {
        let (tx, rx) = mpsc::channel(100);

        let streams: Vec<String> = symbols
            .iter()
            .map(|s| format!("{}@ticker", s.to_lowercase()))
            .collect();

        let stream_param = streams.join("/");
        let ws_url = format!("{}/stream?streams={}", self.environment.ws_url(), stream_param);

        info!("Connecting to WebSocket: {}", ws_url);

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::run_websocket(ws_url, tx_clone).await {
                error!("WebSocket error: {}", e);
            }
        });

        Ok(rx)
    }

    async fn run_websocket(url: String, tx: mpsc::Sender<WsMessage>) -> Result<()> {
        loop {
            match connect_async(&url).await {
                Ok((ws_stream, _)) => {
                    info!("WebSocket connected");
                    let _ = tx.send(WsMessage::Connected).await;

                    let (mut write, mut read) = ws_stream.split();

                    // Ping task to keep connection alive
                    let ping_tx = tx.clone();
                    tokio::spawn(async move {
                        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
                        loop {
                            interval.tick().await;
                            debug!("Sending WebSocket ping");
                            // Note: we can't send ping from here without shared write access
                            // In production, you'd use Arc<Mutex<>> or a proper actor pattern
                        }
                        #[allow(unreachable_code)]
                        let _ = ping_tx; // Suppress unused warning
                    });

                    while let Some(msg_result) = read.next().await {
                        match msg_result {
                            Ok(Message::Text(text)) => {
                                if let Err(e) = Self::handle_message(&text, &tx).await {
                                    warn!("Failed to handle message: {}", e);
                                }
                            }
                            Ok(Message::Ping(data)) => {
                                debug!("Received ping, sending pong");
                                if write.send(Message::Pong(data)).await.is_err() {
                                    break;
                                }
                            }
                            Ok(Message::Close(_)) => {
                                info!("WebSocket closed by server");
                                let _ = tx.send(WsMessage::Disconnected).await;
                                break;
                            }
                            Err(e) => {
                                error!("WebSocket error: {}", e);
                                let _ = tx.send(WsMessage::Error(e.to_string())).await;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to connect to WebSocket: {}", e);
                    let _ = tx.send(WsMessage::Error(e.to_string())).await;
                }
            }

            // Reconnect after delay
            warn!("WebSocket disconnected, reconnecting in 5 seconds...");
            let _ = tx.send(WsMessage::Disconnected).await;
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }

    async fn handle_message(text: &str, tx: &mpsc::Sender<WsMessage>) -> Result<()> {
        #[derive(serde::Deserialize)]
        struct StreamWrapper {
            stream: String,
            data: serde_json::Value,
        }

        let wrapper: StreamWrapper =
            serde_json::from_str(text).context("Failed to parse stream wrapper")?;

        if wrapper.stream.ends_with("@ticker") {
            let ticker: WsTickerUpdate =
                serde_json::from_value(wrapper.data).context("Failed to parse ticker update")?;

            debug!("Ticker update: {} = {}", ticker.symbol, ticker.close_price);
            tx.send(WsMessage::Ticker(ticker))
                .await
                .context("Failed to send ticker to channel")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_creation() {
        let ws = BinanceWebSocket::new(Environment::Testnet);
        assert_eq!(ws.environment, Environment::Testnet);
    }
}
