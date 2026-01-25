use crate::share_state::SharedState;
use std::{collections::HashSet, error, sync::Arc, sync::LazyLock};
use log::{info, error};
// use crate::bingx::BingXTradeOutcome; // Закомментировано вместе с функционалом открытия позиций

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

    let aster_price = {
        let aster_prices = shared_state.aster_prices.read().await;
        *aster_prices.get(symbol).unwrap_or(&0.0)
    };

    // Сравниваем Bybit с Hyperliquid
    if bybit_price != 0.0 && hyperliquid_price != 0.0 {
        let difference = ((bybit_price - hyperliquid_price) / bybit_price).abs() * 100.0;

        if difference >= 0.1 {
            let message = format!(
                ">0.1%: {}, bybit price: {}, hyperliquid price: {}, difference: {:.5}%",
                symbol, bybit_price, hyperliquid_price, difference
            );
            
            // Логируем в консоль
            info!("{}", message);
            
            // Отправляем в Telegram, если доступно
            if let Some(telegram) = &shared_state.telegram {
                telegram
                    .send_arbitrage_opportunity(symbol, bybit_price, hyperliquid_price, "Hyperliquid", difference)
                    .await;
            }
        }
    }

    // Сравниваем Bybit с ASTER
    if bybit_price != 0.0 && aster_price != 0.0 {
        let difference = ((bybit_price - aster_price) / bybit_price).abs() * 100.0;

        if difference >= 0.1 {
            let message = format!(
                ">0.1%: {}, bybit price: {}, aster price: {}, difference: {:.5}%",
                symbol, bybit_price, aster_price, difference
            );
            
            // Логируем в консоль
            info!("{}", message);
            
            // Отправляем в Telegram, если доступно
            if let Some(telegram) = &shared_state.telegram {
                telegram
                    .send_arbitrage_opportunity(symbol, bybit_price, aster_price, "ASTER", difference)
                    .await;
            }
        }
    }

    // Если инициализирован клиент BingX – пробуем автоматически открыть позицию по заданным правилам.
    // ЗАКОММЕНТИРОВАНО: Автоматическое открытие позиций отключено
    /*
    if let Some(bingx) = &shared_state.bingx {
        match bingx
            .handle_arbitrage_opportunity(symbol, bybit_price, hyperliquid_price)
            .await
        {
            Ok(BingXTradeOutcome::Opened {
                symbol: opened_symbol,
                direction,
                quantity,
                leverage,
            }) => {
                info!(
                    "BingX position opened: symbol={}, direction={}, qty={}, leverage={}",
                    opened_symbol, direction, quantity, leverage
                );

                if let Some(telegram) = &shared_state.telegram {
                    let msg = format!(
                        "✅ <b>BingX position opened</b>\n\n\
                        Symbol: <code>{}</code>\n\
                        Side: <code>{}</code>\n\
                        Qty: <code>{:.8}</code>\n\
                        Leverage: <code>{:.0}x</code>\n\
                        Bybit: <code>{:.8}</code>\n\
                        Hyperliquid: <code>{:.8}</code>\n\
                        Diff: <code>{:.5}%</code>",
                        opened_symbol,
                        direction,
                        quantity,
                        leverage,
                        bybit_price,
                        hyperliquid_price,
                        difference
                    );
                    telegram.send_message(&msg).await;
                }
            }
            Ok(BingXTradeOutcome::Skipped { reason }) => {
                info!("BingX trade skipped for {}: {}", symbol, reason);
            }
            Err(e) => {
                error!(
                    "Failed to handle arbitrage opportunity on BingX for {}: {}",
                    symbol, e
                );
            }
        }
    }
    */

    Ok(())
}
