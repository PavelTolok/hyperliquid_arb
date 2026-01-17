use crate::share_state::SharedState;
use std::{collections::HashSet, error, sync::Arc};
use log::info;

const EXCLUDED_TOKENS: &[&str] = &[
    "PIXELUSDT",
    "REQUSDT",
    "NTRNUSDT",
    "ORBSUSDT",
    "RDNTUSDT",
    "LISTAUSDT",
    "CYBERUSDT",
    "ILVUSDT",
    "CATIUSDT",
    "OGNUSDT",
    "BNTUSDT",
];

pub async fn compare_prices(
    shared_state: &Arc<SharedState>,
    symbol: &str,
) -> Result<(), Box<dyn error::Error>> {
    // Пропускаем токены из списка исключений
    let excluded: HashSet<&str> = EXCLUDED_TOKENS.iter().copied().collect();
    if excluded.contains(symbol) {
        return Ok(());
    }
    let bybit_price = {
        let bybit_prices = shared_state.bybit_prices.read().await;
        *bybit_prices.get(symbol).unwrap_or(&0.0)
    };

    let hyperliquid_price = {
        let hyperliquid_prices = shared_state.hyperliquid_prices.read().await;
        *hyperliquid_prices.get(symbol).unwrap_or(&0.0)
    };

    if bybit_price == 0.0 || hyperliquid_price == 0.0 {
        return Ok(());
    }

    let difference = ((bybit_price - hyperliquid_price) / bybit_price).abs() * 100.0;

    if difference >= 5.0 {
        let message = format!(
            ">0.8%: {}, bybit price: {}, hyperliquid price: {}, difference: {:.5}%",
            symbol, bybit_price, hyperliquid_price, difference
        );
        
        // Логируем в консоль
        info!("{}", message);
        
        // Отправляем в Telegram, если доступно
        if let Some(telegram) = &shared_state.telegram {
            telegram
                .send_arbitrage_opportunity(symbol, bybit_price, hyperliquid_price, difference)
                .await;
        }
    }

    Ok(())
}
