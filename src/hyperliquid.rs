use crate::share_state::SharedState;
use hyperliquid_rust_sdk::{BaseUrl, InfoClient, Message, Subscription};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc::unbounded_channel;
use log::{error, info, warn};

pub struct HyperLiquidStruct {
    info_client: InfoClient,
}

impl HyperLiquidStruct {
    pub async fn new() -> Self {
        let info_client = match InfoClient::new(None, Some(BaseUrl::Mainnet)).await {
            Ok(client) => {
                info!("HyperLiquid InfoClient initialized successfully");
                client
            }
            Err(e) => {
                error!("Failed to initialize HyperLiquid InfoClient: {}", e);
                panic!("Failed to initialize HyperLiquid client");
            }
        };
        Self { info_client }
    }

    fn format_hyperliquid_tickers(tickers: &HashMap<String, String>) -> Vec<String> {
        let symbols: Vec<String> = tickers
            .keys()
            .map(|key| Self::format_ticker_name(key))
            .collect();
        symbols
    }

    fn format_ticker_name(ticker: &String) -> String {
        let formatted_ticker = if ticker.starts_with("k") {
            ticker.replacen("k", "1000", 1)
        } else {
            ticker.to_string()
        };
        format!("{}USDT", formatted_ticker)
    }

    pub async fn get_tickers(&self) -> Vec<String> {
        let tickers = match self.info_client.all_mids().await {
            Ok(tickers) => tickers,
            Err(e) => {
                error!("Failed to get HyperLiquid tickers: {}", e);
                return Vec::new();
            }
        };
        let format_tickers = Self::format_hyperliquid_tickers(&tickers);
        info!("Retrieved {} HyperLiquid tickers", format_tickers.len());
        format_tickers
    }

    pub async fn hyperliquid_ws(mut self, shared_state: &Arc<SharedState>) {
        let (sender, mut receiver) = unbounded_channel();
        if let Err(e) = self.info_client
            .subscribe(Subscription::AllMids, sender)
            .await
        {
            error!("Failed to subscribe to HyperLiquid WebSocket: {}", e);
            return;
        }
        info!("Subscribed to HyperLiquid WebSocket");

        while let Some(message) = receiver.recv().await {
            match message {
                Message::AllMids(all_mids) => {
                    for (ticker, price_str) in all_mids.data.mids.iter() {
                        let formatted_ticker = Self::format_ticker_name(&ticker);
                        let price: f64 = match price_str.parse() {
                            Ok(p) => p,
                            Err(e) => {
                                warn!("Failed to parse price for {}: {} (value: {})", formatted_ticker, e, price_str);
                                0.0
                            }
                        };
                        {
                            let mut hyperliquid_prices = shared_state.hyperliquid_prices.write().await;
                            hyperliquid_prices.insert(formatted_ticker.clone(), price);
                        }
                    }
                }
                _ => {
                    warn!("Received unexpected message type from HyperLiquid");
                }
            }
        }
        error!("HyperLiquid WebSocket receiver closed");
    }
}
