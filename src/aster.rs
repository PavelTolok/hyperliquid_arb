use crate::share_state::SharedState;
use std::{sync::Arc, time::Duration};
use tokio::time::sleep;
use log::{error, info, warn};
use serde::Deserialize;
use std::env;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

#[derive(Debug, Deserialize)]
struct ExchangeInfoResponse {
    symbols: Vec<SymbolInfo>,
}

#[derive(Debug, Deserialize)]
struct SymbolInfo {
    symbol: String,
    status: String,
}


pub struct AsterStruct {
    #[allow(dead_code)]
    api_key: String,
    #[allow(dead_code)]
    api_secret: String,
    base_url: String,
    ws_url: String,
}

impl AsterStruct {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let api_key = env::var("ASTER_API_KEY")
            .map_err(|_| "ASTER_API_KEY not found in environment")?;
        let api_secret = env::var("ASTER_API_SECRET")
            .map_err(|_| "ASTER_API_SECRET not found in environment")?;

        if api_key.is_empty() || api_secret.is_empty() {
            return Err("ASTER_API_KEY or ASTER_API_SECRET is empty".into());
        }

        Ok(Self {
            api_key,
            api_secret,
            base_url: "https://fapi.asterdex.com".to_string(),
            ws_url: "wss://fstream.asterdex.com".to_string(),
        })
    }

    pub async fn get_tickers(&self) -> Vec<String> {
        // Получаем список всех символов из exchangeInfo
        let exchange_info_url = format!("{}/fapi/v1/exchangeInfo", self.base_url);
        
        match reqwest::get(&exchange_info_url).await {
            Ok(response) => {
                match response.json::<ExchangeInfoResponse>().await {
                    Ok(exchange_info) => {
                        let tickers: Vec<String> = exchange_info
                            .symbols
                            .into_iter()
                            .filter(|s| s.status == "TRADING")
                            .map(|s| s.symbol)
                            .collect();
                        info!("Retrieved {} ASTER tickers", tickers.len());
                        tickers
                    }
                    Err(e) => {
                        error!("Failed to parse ASTER exchangeInfo: {}", e);
                        Vec::new()
                    }
                }
            }
            Err(e) => {
                error!("Failed to get ASTER exchangeInfo: {}", e);
                Vec::new()
            }
        }
    }

    pub async fn aster_ws(self, shared_state: &Arc<SharedState>) {
        const MAX_RECONNECT_ATTEMPTS: u32 = 0; // 0 = бесконечные попытки
        const RECONNECT_DELAY: Duration = Duration::from_secs(5);
        const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(30);
        
        let mut reconnect_count = 0u32;
        
        // Внешний цикл для переподключений
        loop {
            // Подключаемся к WebSocket
            let ws_url = format!("{}/stream?streams=!ticker@arr", self.ws_url);
            let (mut ws_stream, _) = match connect_async(&ws_url).await {
                Ok(stream) => {
                    if reconnect_count == 0 {
                        info!("ASTER WebSocket connected successfully");
                    } else {
                        info!("ASTER WebSocket reconnected (attempt {})", reconnect_count + 1);
                    }
                    reconnect_count = 0; // Сбрасываем счетчик при успешном подключении
                    stream
                }
                Err(e) => {
                    error!("Failed to connect to ASTER WebSocket: {}", e);
                    reconnect_count += 1;
                    if MAX_RECONNECT_ATTEMPTS > 0 && reconnect_count >= MAX_RECONNECT_ATTEMPTS {
                        error!("Max reconnection attempts ({}) reached. Exiting.", MAX_RECONNECT_ATTEMPTS);
                        return;
                    }
                    warn!("Retrying in {:?}...", RECONNECT_DELAY);
                    sleep(RECONNECT_DELAY).await;
                    continue;
                }
            };

            // Внутренний цикл для обработки сообщений
            let mut last_message_time = std::time::Instant::now();
            let mut connection_alive = true;
            
            while connection_alive {
                // Используем timeout для обнаружения "тихих" разрывов соединения
                match tokio::time::timeout(HEARTBEAT_TIMEOUT, ws_stream.next()).await {
                    Ok(Some(Ok(Message::Text(text)))) => {
                        last_message_time = std::time::Instant::now();
                        
                        // Парсим сообщение
                        match serde_json::from_str::<serde_json::Value>(&text) {
                            Ok(json) => {
                                // Проверяем, что это сообщение с данными тикера
                                if let Some(data) = json.get("data") {
                                    if let Some(data_array) = data.as_array() {
                                        // Обрабатываем массив тикеров
                                        for ticker_data in data_array {
                                            if let Some(symbol) = ticker_data.get("s").and_then(|s| s.as_str()) {
                                                if let Some(price_str) = ticker_data.get("c").and_then(|p| p.as_str()) {
                                                    let price: f64 = match price_str.parse::<f64>() {
                                                        Ok(p) => {
                                                            if p <= 0.0 || !p.is_finite() {
                                                                warn!("Invalid price for {}: {}", symbol, p);
                                                                continue;
                                                            }
                                                            p
                                                        }
                                                        Err(e) => {
                                                            warn!("Failed to parse price for {}: {} (value: {})", symbol, e, price_str);
                                                            continue;
                                                        }
                                                    };
                                                    
                                                    {
                                                        let mut aster_prices = shared_state.aster_prices.write().await;
                                                        aster_prices.insert(symbol.to_string(), price);
                                                    }
                                                }
                                            }
                                        }
                                    } else if let Some(symbol) = data.get("s").and_then(|s| s.as_str()) {
                                        // Обрабатываем одиночный тикер
                                        if let Some(price_str) = data.get("c").and_then(|p| p.as_str()) {
                                            let price: f64 = match price_str.parse::<f64>() {
                                                Ok(p) => {
                                                    if p <= 0.0 || !p.is_finite() {
                                                        warn!("Invalid price for {}: {}", symbol, p);
                                                        continue;
                                                    }
                                                    p
                                                }
                                                Err(e) => {
                                                    warn!("Failed to parse price for {}: {} (value: {})", symbol, e, price_str);
                                                    continue;
                                                }
                                            };
                                            
                                            {
                                                let mut aster_prices = shared_state.aster_prices.write().await;
                                                aster_prices.insert(symbol.to_string(), price);
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse ASTER WebSocket message: {} (text: {})", e, text);
                            }
                        }
                    }
                    Ok(Some(Ok(Message::Ping(_)))) => {
                        // Отвечаем на ping
                        if let Err(e) = ws_stream.send(Message::Pong(vec![])).await {
                            warn!("Failed to send pong: {}", e);
                            connection_alive = false;
                        }
                    }
                    Ok(Some(Ok(Message::Pong(_)))) => {
                        // Игнорируем pong сообщения
                    }
                    Ok(Some(Ok(Message::Binary(_)))) => {
                        // Игнорируем binary сообщения (если они появятся)
                    }
                    Ok(Some(Ok(Message::Close(_)))) => {
                        warn!("ASTER WebSocket connection closed by server");
                        connection_alive = false;
                    }
                    Ok(Some(Err(e))) => {
                        error!("ASTER WebSocket error: {}", e);
                        connection_alive = false;
                    }
                    Ok(None) => {
                        warn!("ASTER WebSocket stream ended");
                        connection_alive = false;
                    }
                    Err(_) => {
                        // Timeout - возможно соединение тихо разорвано
                        let elapsed = last_message_time.elapsed();
                        warn!("No messages received from ASTER for {:?}. Connection may be lost.", elapsed);
                        connection_alive = false;
                    }
                }
            }

            // Соединение потеряно, пытаемся переподключиться
            error!("ASTER WebSocket connection lost. Attempting to reconnect...");
            reconnect_count += 1;
            
            if MAX_RECONNECT_ATTEMPTS > 0 && reconnect_count >= MAX_RECONNECT_ATTEMPTS {
                error!("Max reconnection attempts ({}) reached. Exiting.", MAX_RECONNECT_ATTEMPTS);
                return;
            }
            
            warn!("Reconnecting in {:?}... (attempt {}{})", 
                  RECONNECT_DELAY, 
                  reconnect_count,
                  if MAX_RECONNECT_ATTEMPTS > 0 {
                      format!("/{}", MAX_RECONNECT_ATTEMPTS)
                  } else {
                      "".to_string()
                  });
            sleep(RECONNECT_DELAY).await;
        }
    }
}
