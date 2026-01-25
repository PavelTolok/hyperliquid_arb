use std::env;
use std::time::Duration;
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

        // Валидация входных данных
        if bot_token.is_empty() {
            return Err("TELEGRAM_BOT_TOKEN is empty".into());
        }
        if chat_id.is_empty() {
            return Err("TELEGRAM_CHAT_ID is empty".into());
        }
        // Проверка формата chat_id (должен быть числом или начинаться с @)
        if chat_id.parse::<i64>().is_err() && !chat_id.starts_with('@') {
            return Err("TELEGRAM_CHAT_ID has invalid format".into());
        }

        // Создаем HTTP клиент с таймаутами для защиты от DoS
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        Ok(Self {
            bot_token,
            chat_id,
            client,
        })
    }

    /// Экранирует HTML символы для безопасной вставки в HTML
    fn escape_html(text: &str) -> String {
        text.chars()
            .flat_map(|c| match c {
                '<' => "&lt;".chars().collect::<Vec<_>>(),
                '>' => "&gt;".chars().collect::<Vec<_>>(),
                '&' => "&amp;".chars().collect::<Vec<_>>(),
                '"' => "&quot;".chars().collect::<Vec<_>>(),
                '\'' => "&#x27;".chars().collect::<Vec<_>>(),
                _ => vec![c],
            })
            .collect()
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
        dex_price: f64,
        dex_name: &str,
        difference: f64,
    ) {
        // Валидация и экранирование символа для защиты от HTML injection
        let safe_symbol = if symbol.len() > 50 {
            // Ограничиваем длину символа
            &symbol[..50]
        } else {
            symbol
        };
        let escaped_symbol = Self::escape_html(safe_symbol);
        let escaped_dex_name = Self::escape_html(dex_name);

        let message = format!(
            "🔔 <b>Арбитражная возможность!</b>\n\n\
            Символ: <code>{}</code>\n\
            Bybit цена: <code>{:.8}</code>\n\
            {} цена: <code>{:.8}</code>\n\
            Разница: <code>{:.5}%</code>",
            escaped_symbol, bybit_price, escaped_dex_name, dex_price, difference
        );

        self.send_message(&message).await;
    }
}
