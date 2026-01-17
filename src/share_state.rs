use std::collections::HashMap;

use tokio::sync::RwLock;

use crate::telegram::TelegramNotifier;

#[derive(Debug)]
pub struct SharedState {
    pub bybit_prices: RwLock<HashMap<String, f64>>,
    pub hyperliquid_prices: RwLock<HashMap<String, f64>>,
    pub telegram: Option<TelegramNotifier>,
}

impl SharedState {
    pub fn new() -> Self {
        SharedState {
            bybit_prices: RwLock::new(HashMap::new()),
            hyperliquid_prices: RwLock::new(HashMap::new()),
            telegram: None,
        }
    }

    pub fn with_telegram(telegram: TelegramNotifier) -> Self {
        SharedState {
            bybit_prices: RwLock::new(HashMap::new()),
            hyperliquid_prices: RwLock::new(HashMap::new()),
            telegram: Some(telegram),
        }
    }
}
