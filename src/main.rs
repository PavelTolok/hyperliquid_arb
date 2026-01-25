use crate::share_state::SharedState;
use bybit::Bybit;
use hyperliquid::HyperLiquidStruct;
use aster::AsterStruct;
use std::collections::HashSet;
use std::sync::Arc;

mod bybit;
mod compare_price;
mod hyperliquid;
mod share_state;
mod telegram;
mod utils;
mod bingx;
mod aster;

use bingx::BingXClient;

fn get_common_tickers(bybit_tickers: Vec<String>, hyperliquid_tickers: Vec<String>, aster_tickers: Vec<String>) -> HashSet<String> {
    // Используем HashSet для O(1) поиска вместо O(n)
    let hyperliquid_set: HashSet<String> = hyperliquid_tickers.into_iter().collect();
    let aster_set: HashSet<String> = aster_tickers.into_iter().collect();
    let common_tickers: HashSet<String> = bybit_tickers
        .into_iter()
        .filter(|ticker| hyperliquid_set.contains(ticker) && aster_set.contains(ticker))
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

    log::info!("Starting arbitrage bot (Bybit + Hyperliquid + ASTER)...");

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

    // Инициализируем ASTER клиента
    let aster_client = match AsterStruct::new() {
        Ok(client) => {
            log::info!("ASTER client initialized successfully");
            client
        }
        Err(e) => {
            log::error!("Failed to initialize ASTER client: {}. Exiting.", e);
            std::process::exit(1);
        }
    };

    // Инициализируем BingX клиента (если есть ключи в окружении)
    let bingx_client = match BingXClient::from_env() {
        Ok(client) => {
            log::info!("BingX client initialized successfully");
            Some(Arc::new(client))
        }
        Err(e) => {
            log::warn!("BingX client not initialized: {}. BingX trading is disabled.", e);
            None
        }
    };

    let bybit = Bybit::new();
    let shared_state = Arc::new(
        if let Some(telegram) = telegram_notifier {
            SharedState::with_telegram(telegram, bingx_client.clone())
        } else {
            SharedState::new(bingx_client.clone())
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

    let aster_tickers = aster_client.get_tickers().await;

    let common_tickers = get_common_tickers(bybit_tickers, hyperliquid_tickers, aster_tickers);
    
    if common_tickers.is_empty() {
        log::error!("No common tickers found between Bybit, Hyperliquid and ASTER");
        std::process::exit(1);
    }
    
    log::info!("Found {} common tickers", common_tickers.len());

    {
        let mut bybit_prices = shared_state.bybit_prices.write().await;
        let mut hyperliquid_price = shared_state.hyperliquid_prices.write().await;
        let mut aster_prices = shared_state.aster_prices.write().await;
        for ticker in &common_tickers {
            bybit_prices.insert(ticker.clone(), 0.0);
            hyperliquid_price.insert(ticker.clone(), 0.0);
            aster_prices.insert(ticker.clone(), 0.0);
        }
    }

    // Конвертируем HashSet в Vec для передачи в bybit_ws (для совместимости)
    let common_tickers_vec: Vec<String> = common_tickers.iter().cloned().collect();
    let common_tickers_set = common_tickers;

    tokio::join!(
        hyper_liquid.hyperliquid_ws(&shared_state),
        bybit.bybit_ws(&common_tickers_vec, &common_tickers_set, &shared_state),
        aster_client.aster_ws(&shared_state)
    );
}
