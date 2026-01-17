use crate::share_state::SharedState;
use hyperliquid_rust_sdk::{BaseUrl, InfoClient, Message, Subscription};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::mpsc::unbounded_channel;
use tokio::time::sleep;
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
        tickers
            .keys()
            .map(Self::format_ticker_name)
            .collect()
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

    pub async fn hyperliquid_ws(self, shared_state: &Arc<SharedState>) {
        const MAX_RECONNECT_ATTEMPTS: u32 = 0; // 0 = бесконечные попытки
        const RECONNECT_DELAY: Duration = Duration::from_secs(5);
        const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(30);
        
        let mut reconnect_count = 0u32;
        
        // Внешний цикл для переподключений
        loop {
            // Создаем новый клиент при каждом переподключении
            // Это критически важно для избежания проблем с внутренним состоянием WebSocket manager
            // когда возникает ошибка "Reader data not found"
            let mut info_client = match InfoClient::new(None, Some(BaseUrl::Mainnet)).await {
                Ok(client) => {
                    if reconnect_count == 0 {
                        info!("HyperLiquid InfoClient created successfully");
                    } else {
                        info!("HyperLiquid InfoClient recreated for reconnection (attempt {})", reconnect_count + 1);
                    }
                    client
                }
                Err(e) => {
                    error!("Failed to create HyperLiquid InfoClient: {}", e);
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

            // Создаем канал для подписки
            let (sender, mut receiver) = unbounded_channel();
            match info_client.subscribe(Subscription::AllMids, sender).await {
                Ok(_) => {
                    if reconnect_count == 0 {
                        info!("Subscribed to HyperLiquid WebSocket");
                    } else {
                        info!("Reconnected to HyperLiquid WebSocket (attempt {})", reconnect_count + 1);
                    }
                    reconnect_count = 0; // Сбрасываем счетчик при успешном подключении
                }
                Err(e) => {
                    error!("Failed to subscribe to HyperLiquid WebSocket: {}", e);
                    reconnect_count += 1;
                    if MAX_RECONNECT_ATTEMPTS > 0 && reconnect_count >= MAX_RECONNECT_ATTEMPTS {
                        error!("Max reconnection attempts ({}) reached. Exiting.", MAX_RECONNECT_ATTEMPTS);
                        return;
                    }
                    warn!("Retrying subscription in {:?}...", RECONNECT_DELAY);
                    sleep(RECONNECT_DELAY).await;
                    continue;
                }
            }

            // Внутренний цикл для обработки сообщений
            let mut last_message_time = std::time::Instant::now();
            let mut connection_alive = true;
            
            while connection_alive {
                // Используем timeout для обнаружения "тихих" разрывов соединения
                // Если сообщения не приходят долго, возможно соединение разорвано
                match tokio::time::timeout(HEARTBEAT_TIMEOUT, receiver.recv()).await {
                    Ok(Some(message)) => {
                        last_message_time = std::time::Instant::now();
                        match message {
                            Message::AllMids(all_mids) => {
                                for (ticker, price_str) in all_mids.data.mids.iter() {
                                    let formatted_ticker = Self::format_ticker_name(ticker);
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
                    Ok(None) => {
                        // Канал закрыт - соединение разорвано
                        warn!("HyperLiquid WebSocket receiver channel closed (possibly due to 'Reader data not found' error)");
                        connection_alive = false;
                    }
                    Err(_) => {
                        // Timeout - возможно соединение тихо разорвано
                        let elapsed = last_message_time.elapsed();
                        warn!("No messages received from HyperLiquid for {:?}. Connection may be lost.", elapsed);
                        connection_alive = false;
                    }
                }
            }

            // Соединение потеряно, пытаемся переподключиться
            error!("HyperLiquid WebSocket connection lost. Attempting to reconnect...");
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
