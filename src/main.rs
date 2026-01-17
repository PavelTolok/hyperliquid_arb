use crate::share_state::SharedState;
use bybit::Bybit;
use hyperliquid::HyperLiquidStruct;
use std::sync::Arc;

mod bybit;
mod compare_price;
mod hyperliquid;
mod share_state;
mod telegram;
mod utils;

fn get_common_tickers(bybit_tickers: Vec<String>, hyperliquid_tickers: Vec<String>) -> Vec<String> {
    let common_tickers: Vec<String> = bybit_tickers
        .iter()
        .filter(|ticker| hyperliquid_tickers.contains(&ticker))
        .cloned()
        .collect();
    common_tickers
}

#[tokio::main]
async fn main() {
    // Загружаем переменные окружения из .env
    dotenv::dotenv().ok();
    
    // Инициализируем логирование
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    log::info!("Starting hyperliquid arbitrage bot...");

    // Инициализируем Telegram notifier (если доступен)
    let telegram_notifier = match crate::telegram::TelegramNotifier::new() {
        Ok(notifier) => {
            log::info!("Telegram notifier initialized successfully");
            Some(notifier)
        }
        Err(e) => {
            log::warn!("Failed to initialize Telegram notifier: {}. Continuing without Telegram notifications.", e);
            None
        }
    };

    let hyper_liquid = HyperLiquidStruct::new().await;
    let bybit = Bybit::new();
    let shared_state = Arc::new(
        if let Some(telegram) = telegram_notifier {
            SharedState::with_telegram(telegram)
        } else {
            SharedState::new()
        }
    );

    let hyperliquid_tickers = hyper_liquid.get_tickers().await;

    let bybit_tickers = match bybit.get_tickers().await {
        Ok(tickers) => tickers,
        Err(e) => {
            log::error!("Error calling bybit get tickers: {}", e);
            std::process::exit(1);
        }
    };

    let common_tickers = get_common_tickers(bybit_tickers, hyperliquid_tickers);

    {
        let mut bybit_prices = shared_state.bybit_prices.write().await;
        let mut hyperliquid_price = shared_state.hyperliquid_prices.write().await;
        for ticker in &common_tickers {
            bybit_prices.insert(ticker.clone(), 0.0);
            hyperliquid_price.insert(ticker.clone(), 0.0);
        }
    }

    tokio::join!(
        hyper_liquid.hyperliquid_ws(&shared_state),
        bybit.bybit_ws(&common_tickers, &shared_state)
    );
}
