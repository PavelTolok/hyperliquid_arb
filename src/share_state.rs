use std::collections::HashMap;

use tokio::sync::RwLock;

use crate::{bingx::BingXClient, telegram::TelegramNotifier};

#[derive(Debug)]
pub struct SharedState {
    pub bybit_prices: RwLock<HashMap<String, f64>>,
    pub hyperliquid_prices: RwLock<HashMap<String, f64>>,
    pub telegram: Option<TelegramNotifier>,
    /// Опциональный клиент BingX. Если не инициализирован – торги на BingX отключены.
    pub bingx: Option<std::sync::Arc<BingXClient>>,
}

impl SharedState {
    pub fn new(bingx: Option<std::sync::Arc<BingXClient>>) -> Self {
        SharedState {
            bybit_prices: RwLock::new(HashMap::new()),
            hyperliquid_prices: RwLock::new(HashMap::new()),
            telegram: None,
            bingx,
        }
    }

    pub fn with_telegram(telegram: TelegramNotifier, bingx: Option<std::sync::Arc<BingXClient>>) -> Self {
        SharedState {
            bybit_prices: RwLock::new(HashMap::new()),
            hyperliquid_prices: RwLock::new(HashMap::new()),
            telegram: Some(telegram),
            bingx,
        }
    }
}
