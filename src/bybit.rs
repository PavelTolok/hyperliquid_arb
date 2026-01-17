use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use log::{error, info, warn};

use crate::{
    compare_price::compare_prices,
    share_state::SharedState,
    utils::{BybitApiResponse, BybitWsResponse},
};

pub struct Bybit {
    instrument_api_url: String,
    ws_url: String,
}

impl Bybit {
    pub fn new() -> Self {
        Self {
            instrument_api_url: "https://api.bybit.com/v5/market/instruments-info?category=linear"
                .into(),
            ws_url: "wss://stream.bybit.com/v5/public/linear".into(),
        }
    }

    pub async fn get_tickers(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let response = reqwest::get(&self.instrument_api_url).await
            .map_err(|e| {
                error!("Failed to fetch Bybit tickers: {}", e);
                e
            })?;
        let response_data: BybitApiResponse = response.json().await
            .map_err(|e| {
                error!("Failed to parse Bybit API response: {}", e);
                e
            })?;

        let tickers: Vec<String> = response_data
            .result
            .list
            .iter()
            .filter_map(|ticker| {
                if !ticker.symbol.contains("-") {
                    Some(ticker.symbol.clone())
                } else {
                    None
                }
            })
            .collect();

        Ok(tickers)
    }
    pub async fn bybit_ws(&self, common_tickers: &Vec<String>, shared_state: &Arc<SharedState>) {
        let (mut ws_stream, _) = match connect_async(&self.ws_url).await {
            Ok(stream) => {
                info!("Bybit WebSocket connected successfully");
                stream
            }
            Err(e) => {
                error!("Failed to connect to Bybit WebSocket: {}", e);
                return;
            }
        };

        let args: Vec<String> = common_tickers
            .iter()
            .map(|ticker| format!("kline.D.{}", ticker))
            .collect();

        let subscribe_message = serde_json::json!({
            "op": "subscribe",
            "args": args
        })
        .to_string();

        if let Err(e) = ws_stream.send(Message::Text(subscribe_message)).await {
            error!("Failed to subscribe to Bybit topics: {}", e);
            return;
        }
        info!("Subscribed to Bybit topics");

        while let Some(message) = ws_stream.next().await {
            match message {
                Ok(Message::Text(text)) => match serde_json::from_str::<BybitWsResponse>(&text) {
                    Ok(parse_msg) => {
                        // Пропускаем сообщения без topic (подтверждения подписки, heartbeat и т.д.)
                        if let Some(topic) = &parse_msg.topic {
                            if let Some(data) = &parse_msg.data {
                                if !data.is_empty() {
                                    let symbol = topic.split(".").last().unwrap().to_string();
                                    if common_tickers.contains(&symbol) {
                                        let price: f64 = data[0].close.parse().unwrap();
                                        {
                                            let mut bybit_prices = shared_state.bybit_prices.write().await;
                                            bybit_prices.insert(symbol.clone(), price);
                                        }
                                        if let Err(e) = compare_prices(shared_state, &symbol).await {
                                            error!("Failed comparing price in bybit for {}: {}", symbol, e);
                                        }
                                    }
                                }
                            }
                        }
                        // Игнорируем сообщения без topic (подтверждения подписки и т.д.)
                    }
                    Err(e) => {
                        warn!("Failed parsing Bybit data: {}", e);
                    }
                },
                Ok(data) => {
                    warn!("Received unparseable data from Bybit: {:?}", data);
                }
                Err(e) => {
                    error!("Bybit WebSocket error: {}", e);
                }
            }
        }
    }
}
