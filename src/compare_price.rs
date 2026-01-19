use crate::share_state::SharedState;
use std::{collections::HashSet, error, sync::Arc, sync::LazyLock};
use log::{info, error};

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

// Кэшируем HashSet исключенных токенов, чтобы не создавать его каждый раз
static EXCLUDED_TOKENS_SET: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    EXCLUDED_TOKENS.iter().copied().collect()
});

pub async fn compare_prices(
    shared_state: &Arc<SharedState>,
    symbol: &str,
) -> Result<(), Box<dyn error::Error>> {
    // Пропускаем токены из списка исключений
    if EXCLUDED_TOKENS_SET.contains(symbol) {
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

    if difference >= 1.0 {
        let message = format!(
            ">5.0%: {}, bybit price: {}, hyperliquid price: {}, difference: {:.5}%",
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

        // Если инициализирован клиент BingX – пробуем автоматически открыть позицию по заданным правилам.
        if let Some(bingx) = &shared_state.bingx {
            if let Err(e) = bingx
                .handle_arbitrage_opportunity(symbol, bybit_price, hyperliquid_price)
                .await
            {
                error!(
                    "Failed to handle arbitrage opportunity on BingX for {}: {}",
                    symbol, e
                );
            }
        }
    }

    Ok(())
}
