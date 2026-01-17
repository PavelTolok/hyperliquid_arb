use std::env;
use log::error;

#[derive(Debug)]
pub struct TelegramNotifier {
    bot_token: String,
    chat_id: String,
    client: reqwest::Client,
}

impl TelegramNotifier {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let bot_token = env::var("TELEGRAM_BOT_TOKEN")
            .map_err(|_| "TELEGRAM_BOT_TOKEN not found in environment")?;
        let chat_id = env::var("TELEGRAM_CHAT_ID")
            .map_err(|_| "TELEGRAM_CHAT_ID not found in environment")?;

        Ok(Self {
            bot_token,
            chat_id,
            client: reqwest::Client::new(),
        })
    }

    pub async fn send_message(&self, message: &str) {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );

        let payload = serde_json::json!({
            "chat_id": self.chat_id,
            "text": message,
            "parse_mode": "HTML"
        });

        match self.client.post(&url).json(&payload).send().await {
            Ok(response) => {
                if !response.status().is_success() {
                    if let Ok(text) = response.text().await {
                        error!("Telegram API error: {}", text);
                    }
                }
            }
            Err(e) => {
                error!("Failed to send Telegram message: {}", e);
            }
        }
    }

    pub async fn send_arbitrage_opportunity(
        &self,
        symbol: &str,
        bybit_price: f64,
        hyperliquid_price: f64,
        difference: f64,
    ) {
        let message = format!(
            "🔔 <b>Арбитражная возможность!</b>\n\n\
            Символ: <code>{}</code>\n\
            Bybit цена: <code>{:.8}</code>\n\
            Hyperliquid цена: <code>{:.8}</code>\n\
            Разница: <code>{:.5}%</code>",
            symbol, bybit_price, hyperliquid_price, difference
        );

        self.send_message(&message).await;
    }
}
