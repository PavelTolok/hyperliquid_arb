use std::collections::HashMap;
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use log::{error, info, warn};
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use sha2::Sha256;
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

/// Клиент для работы с BingX Perpetual Futures.
///
/// Задачи:
/// - проверка открытых позиций по символу;
/// - получение баланса USDT;
/// - выставление маркет-ордеров в кросс-марже с заданным плечом.
#[derive(Debug)]
pub struct BingXClient {
    api_key: String,
    api_secret: String,
    http_client: Client,
    base_url: String,
}

#[derive(Debug, Clone)]
pub enum BingXTradeOutcome {
    /// Новая позиция была открыта.
    Opened {
        symbol: String,
        direction: String, // LONG / SHORT
        quantity: f64,
        leverage: f64,
    },
    /// Ничего не сделали (например, уже есть открытая позиция).
    Skipped { reason: String },
}

#[derive(Debug, Error)]
pub enum BingXError {
    #[error("missing env var: {0}")]
    MissingEnv(String),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("api error: {0}")]
    Api(String),
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Deserialize)]
struct ApiResponse<T>
where
    T: Default,
{
    code: i32,
    msg: Option<String>,
    #[serde(default)]
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct Position {
    symbol: String,
    #[serde(rename = "positionSide")]
    position_side: Option<String>,
    #[serde(rename = "positionAmt")]
    position_amt: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct PositionsData {
    positions: Vec<Position>,
}

#[derive(Debug, Deserialize)]
struct BalanceItem {
    asset: String,
    #[serde(rename = "availableBalance")]
    available_balance: String,
}

#[derive(Debug, Deserialize, Default)]
struct BalanceData {
    balances: Vec<BalanceItem>,
}

#[derive(Debug, Deserialize, Default)]
struct OrderResponse {
    #[allow(dead_code)]
    order_id: Option<String>,
}

impl BingXClient {
    /// Приводим тикер из формата проекта (`AXSUSDT`) к формату BingX (`AXS-USDT`).
    /// Если символ уже содержит `-`, возвращаем как есть.
    fn normalize_symbol(symbol: &str) -> String {
        let s = symbol.trim();
        if s.contains('-') {
            return s.to_string();
        }
        if let Some(base) = s.strip_suffix("USDT") {
            return format!("{}-USDT", base);
        }
        s.to_string()
    }

    pub fn from_env() -> Result<Self, BingXError> {
        let api_key = env::var("BINGX_API_KEY")
            .map_err(|_| BingXError::MissingEnv("BINGX_API_KEY".into()))?;
        let api_secret = env::var("BINGX_API_SECRET")
            .map_err(|_| BingXError::MissingEnv("BINGX_API_SECRET".into()))?;

        if api_key.is_empty() {
            return Err(BingXError::MissingEnv(
                "BINGX_API_KEY is empty".to_string(),
            ));
        }
        if api_secret.is_empty() {
            return Err(BingXError::MissingEnv(
                "BINGX_API_SECRET is empty".to_string(),
            ));
        }

        let http_client = Client::new();

        Ok(Self {
            api_key,
            api_secret,
            http_client,
            base_url: "https://open-api.bingx.com".to_string(),
        })
    }

    fn timestamp_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn sign(&self, query: &str) -> Result<String, BingXError> {
        let mut mac =
            HmacSha256::new_from_slice(self.api_secret.as_bytes()).map_err(|e| {
                BingXError::Internal(format!("failed to create HMAC instance: {}", e))
            })?;
        mac.update(query.as_bytes());
        let result = mac.finalize().into_bytes();
        Ok(hex::encode(result))
    }

    async fn get_signed<T: for<'de> Deserialize<'de> + Default>(
        &self,
        path: &str,
        mut params: HashMap<String, String>,
    ) -> Result<T, BingXError> {
        params.insert("timestamp".to_string(), Self::timestamp_ms().to_string());
        let query = Self::build_query(&params);
        let signature = self.sign(&query)?;
        let full_query = format!("{}&signature={}", query, signature);

        let url = format!("{}{}?{}", self.base_url, path, full_query);
        let resp = self
            .http_client
            .get(&url)
            .header("X-BX-APIKEY", &self.api_key)
            .send()
            .await
            .map_err(BingXError::Http)?;

        let text = resp.text().await.map_err(BingXError::Http)?;
        let api_resp: ApiResponse<T> = serde_json::from_str(&text).map_err(BingXError::Serde)?;

        if api_resp.code != 0 {
            return Err(BingXError::Api(
                api_resp
                    .msg
                    .unwrap_or_else(|| format!("unknown error, body: {}", text)),
            ));
        }

        api_resp
            .data
            .ok_or_else(|| BingXError::Api("missing data field in response".into()))
    }

    async fn post_signed<T: for<'de> Deserialize<'de> + Default>(
        &self,
        path: &str,
        mut params: HashMap<String, String>,
    ) -> Result<T, BingXError> {
        params.insert("timestamp".to_string(), Self::timestamp_ms().to_string());
        let query = Self::build_query(&params);
        let signature = self.sign(&query)?;
        let full_body = format!("{}&signature={}", query, signature);

        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http_client
            .post(&url)
            .header("X-BX-APIKEY", &self.api_key)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(full_body)
            .send()
            .await
            .map_err(BingXError::Http)?;

        let text = resp.text().await.map_err(BingXError::Http)?;
        let api_resp: ApiResponse<T> = serde_json::from_str(&text).map_err(BingXError::Serde)?;

        if api_resp.code != 0 {
            return Err(BingXError::Api(
                api_resp
                    .msg
                    .unwrap_or_else(|| format!("unknown error, body: {}", text)),
            ));
        }

        api_resp
            .data
            .ok_or_else(|| BingXError::Api("missing data field in response".into()))
    }

    fn build_query(params: &HashMap<String, String>) -> String {
        let mut items: Vec<(String, String)> = params.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        items.sort_by(|a, b| a.0.cmp(&b.0));
        items
            .into_iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&")
    }

    /// Возвращает количество открытых позиций на BingX (по всем символам).
    ///
    /// Твое требование: если есть ХОТЯ БЫ ОДНА открытая позиция — не открывать ничего нового.
    pub async fn count_open_positions(&self) -> Result<usize, BingXError> {
        let params: HashMap<String, String> = HashMap::new();

        // Важно: у BingX структура data может отличаться.
        // Поэтому сначала получаем как Value, а затем пытаемся извлечь позиции из разных форматов.
        let raw: Value = match self
            .get_signed("/openApi/swap/v2/user/positions", params)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                error!("BingX: positions request failed: {}", e);
                return Err(e);
            }
        };

        // Популярные варианты:
        // - { positions: [...] }
        // - { data: { positions: [...] } }
        // - { data: [ ... ] }
        // - [ ... ]
        let positions_val = if raw.get("positions").is_some() {
            raw.get("positions").cloned().unwrap_or(Value::Null)
        } else if raw.get("data").and_then(|d| d.get("positions")).is_some() {
            raw.get("data")
                .and_then(|d| d.get("positions"))
                .cloned()
                .unwrap_or(Value::Null)
        } else if raw.get("data").map(|d| d.is_array()).unwrap_or(false) {
            raw.get("data").cloned().unwrap_or(Value::Null)
        } else {
            raw.clone()
        };

        let positions: Vec<Position> = match serde_json::from_value(positions_val) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    "BingX: unexpected positions response format. raw_data={}. error={}",
                    raw, e
                );
                return Err(BingXError::Serde(e));
            }
        };

        let open_count = positions
            .iter()
            .filter(|p| {
                p.position_amt
                    .as_ref()
                    .and_then(|s| s.parse::<f64>().ok())
                    .map(|v| v.abs() > 0.0)
                    .unwrap_or(false)
            })
            .count();

        Ok(open_count)
    }

    /// Получаем доступный баланс USDT на фьючерсном аккаунте.
    pub async fn get_available_usdt(&self) -> Result<f64, BingXError> {
        let params = HashMap::new();

        // Получаем raw, потому что формат у BingX нестабилен: бывают варианты с data.balances, balances, массивами и т.п.
        let raw: Value = self
            .get_signed("/openApi/swap/v2/user/balance", params)
            .await
            .map_err(|e| {
                error!("BingX: balance request failed: {}", e);
                e
            })?;

        // Пытаемся извлечь список балансов.
        let balances = Self::extract_balances(&raw);
        if balances.is_empty() {
            error!("BingX: unexpected balance response format, raw={}", raw);
            return Err(BingXError::Api(
                "USDT balance not found in BingX response".into(),
            ));
        }

        for bal in balances {
            if bal.asset.eq_ignore_ascii_case("USDT") {
                if let Ok(v) = bal.available_balance.parse::<f64>() {
                    return Ok(v);
                }
            }
        }

        error!("BingX: USDT balance not found, raw={}", raw);
        Err(BingXError::Api(
            "USDT balance not found in BingX response".into(),
        ))
    }

    /// Выставляем кросс маржу и плечо для символа (если требуется отдельным вызовом).
    pub async fn ensure_cross_margin_10x(&self, symbol: &str, position_side: &str) {
        let bingx_symbol = Self::normalize_symbol(symbol);
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), bingx_symbol.clone());
        params.insert("marginMode".to_string(), "CROSSED".to_string());
        params.insert("leverage".to_string(), "10".to_string());
        // BingX требует side для установки плеча со значениями LONG, SHORT или BOTH
        params.insert("side".to_string(), position_side.to_string());

        // Эндпоинт для настройки плеча/маржи может отличаться.
        // Здесь мы сознательно игнорируем ошибку, чтобы не блокировать основную торговлю,
        // но логируем все детали.
        match self
            .post_signed::<serde_json::Value>("/openApi/swap/v2/trade/leverage", params)
            .await
        {
            Ok(_) => {
                info!(
                    "BingX: successfully ensured cross margin 10x for symbol {}",
                    bingx_symbol
                );
            }
            Err(e) => {
                warn!(
                    "BingX: failed to ensure cross margin 10x for {}: {}. Please verify API endpoint and params.",
                    bingx_symbol, e
                );
            }
        }
    }

    /// Открытие маркет-позиции на BingX.
    ///
    /// - direction: \"LONG\" или \"SHORT\"
    /// - open_on_fraction_of_deposit: доля депозита, которую хотим использовать как маржу (например, 0.75).
    /// - leverage: плечо (например, 10).
    pub async fn open_market_position(
        &self,
        symbol: &str,
        direction: &str,
        open_on_fraction_of_deposit: f64,
        leverage: f64,
        reference_price: f64,
    ) -> Result<BingXTradeOutcome, BingXError> {
        let bingx_symbol = Self::normalize_symbol(symbol);
        if reference_price <= 0.0 {
            return Err(BingXError::Internal(
                "reference_price must be positive".into(),
            ));
        }

        let available_usdt = self.get_available_usdt().await?;
        if available_usdt <= 0.0 {
            return Err(BingXError::Api(
                "available USDT balance is zero on BingX".into(),
            ));
        }

        // Подход: используем 75% депозита как маржу под позицию с плечом.
        // Итоговый notional = deposit * fraction * leverage.
        let margin_to_use = available_usdt * open_on_fraction_of_deposit;
        let notional = margin_to_use * leverage;

        if notional <= 0.0 {
            return Err(BingXError::Internal(
                "computed notional for order is non-positive".into(),
            ));
        }

        let quantity = notional / reference_price;

        info!(
            "BingX: preparing to open {} market position on {}. available_usdt={}, margin_to_use={}, leverage={}, notional={}, qty={}, reference_price={}",
            direction, bingx_symbol, available_usdt, margin_to_use, leverage, notional, quantity, reference_price
        );

        if quantity <= 0.0 {
            return Err(BingXError::Internal(
                "computed quantity for order is non-positive".into(),
            ));
        }

        let side = match direction {
            "LONG" => "BUY",
            "SHORT" => "SELL",
            other => {
                return Err(BingXError::Internal(format!(
                    "unknown direction: {}",
                    other
                )))
            }
        };

        // Убедимся, что включена кросс маржа и 10x плечо (если API это требует отдельным вызовом)
        self.ensure_cross_margin_10x(&bingx_symbol, direction).await;

        let mut params = HashMap::new();
        params.insert("symbol".to_string(), bingx_symbol.clone());
        params.insert("side".to_string(), side.to_string());
        params.insert("positionSide".to_string(), direction.to_string()); // BingX требует positionSide: LONG или SHORT
        params.insert("type".to_string(), "MARKET".to_string());
        // BingX требует quantity (в базовой валюте) или quoteOrderQty (в USDT)
        // Используем quantity для количества контрактов/базовой валюты
        params.insert("quantity".to_string(), quantity.to_string());
        params.insert("marginMode".to_string(), "CROSSED".to_string());
        params.insert("leverage".to_string(), format!("{:.0}", leverage));

        let _resp: OrderResponse = self
            .post_signed("/openApi/swap/v2/trade/order", params)
            .await?;

        info!(
            "BingX: successfully opened {} market position on {} with qty={} and leverage={}",
            direction, bingx_symbol, quantity, leverage
        );

        Ok(BingXTradeOutcome::Opened {
            symbol: bingx_symbol,
            direction: direction.to_string(),
            quantity,
            leverage,
        })
    }

    /// Основной обработчик арбитражной возможности.
    ///
    /// Логика:
    /// 1. Проверить, есть ли уже открытая позиция по символу – если да, НИЧЕГО не делать.
    /// 2. Определить направление (LONG/SHORT) по разнице цен.
    /// 3. Открыть маркет-позицию в кросс марже с 10x плечом на 75% от депозита.
    pub async fn handle_arbitrage_opportunity(
        &self,
        symbol: &str,
        bybit_price: f64,
        hyperliquid_price: f64,
        aster_price: f64,
    ) -> Result<BingXTradeOutcome, BingXError> {
        // 1. КРИТИЧНО: проверка общего числа открытых позиций.
        // Если есть хотя бы одна открытая позиция — НИЧЕГО не открываем.
        match self.count_open_positions().await {
            Ok(open_count) if open_count > 0 => {
                info!(
                    "BingX: {} open position(s) exist. Skipping new order for {}.",
                    open_count, symbol
                );
                return Ok(BingXTradeOutcome::Skipped {
                    reason: format!("{} open position(s) exist", open_count),
                });
            }
            Ok(_) => {
                info!(
                    "BingX: no open positions at all – allowed to open new one for {}.",
                    symbol
                );
            }
            Err(e) => {
                error!(
                    "BingX: failed to check existing positions (global) for {}: {}. Aborting trade.",
                    symbol, e
                );
                return Err(e);
            }
        }

        // 2. Определяем направление по разнице цен
        // SHORT если Price_Hyperliquid > Price_Bybit ИЛИ Price_ASTER > Price_Bybit
        // LONG если Price_Bybit > Price_Hyperliquid ИЛИ Price_Bybit > Price_ASTER
        let direction = if hyperliquid_price > bybit_price || aster_price > bybit_price {
            "SHORT"
        } else if bybit_price > hyperliquid_price || bybit_price > aster_price {
            "LONG"
        } else {
            warn!(
                "BingX: bybit_price == hyperliquid_price == aster_price for {} – no trade direction.",
                symbol
            );
            return Ok(BingXTradeOutcome::Skipped {
                reason: "prices equal".to_string(),
            });
        };

        info!(
            "BingX: arbitrage detected for {}. bybit_price={}, hyperliquid_price={}, direction={}",
            symbol, bybit_price, hyperliquid_price, direction
        );

        // 3. Открываем позицию – 75% от депозита, 10x, маркет.
        // В качестве референсной цены берем цену Bybit (как более ликвидную/центральную).
        let reference_price = bybit_price;
        let outcome = match self
            .open_market_position(symbol, direction, 0.75, 10.0, reference_price)
            .await
        {
            Ok(o) => o,
            Err(e) => {
                error!(
                    "BingX: failed to open {} position for {}: {}",
                    direction, symbol, e
                );
                return Err(e);
            }
        };

        Ok(outcome)
    }
}

#[derive(Debug)]
struct SimpleBalance {
    asset: String,
    available_balance: String,
}

impl BingXClient {
    /// Рекурсивно извлекает балансы из разных вариантов ответа BingX.
    fn extract_balances(raw: &Value) -> Vec<SimpleBalance> {
        let mut out = Vec::new();

        // Если объект с полями asset/availableBalance (или availableMargin)
        if let Some(obj) = raw.as_object() {
            let asset = obj.get("asset").and_then(|v| v.as_str());
            let avail = obj
                .get("availableBalance")
                .and_then(|v| v.as_str())
                .or_else(|| obj.get("availableMargin").and_then(|v| v.as_str()))
                // fallback: иногда есть просто balance
                .or_else(|| obj.get("balance").and_then(|v| v.as_str()));

            if let (Some(asset_s), Some(avail_s)) = (asset, avail) {
                    out.push(SimpleBalance {
                        asset: asset_s.to_string(),
                        available_balance: avail_s.to_string(),
                    });
            }
        }

        // Если есть balances
        if let Some(balances) = raw.get("balances") {
            if let Some(arr) = balances.as_array() {
                out.extend(Self::extract_from_array(arr));
            }
        }

        // Если есть balance (один объект)
        if let Some(balance) = raw.get("balance") {
            out.extend(Self::extract_balances(balance));
        }

        // Если есть data
        if let Some(data) = raw.get("data") {
            // data может быть объектом или массивом
            if let Some(arr) = data.as_array() {
                out.extend(Self::extract_from_array(arr));
            } else {
                out.extend(Self::extract_balances(data));
            }
        }

        // Если сам raw — это массив
        if let Some(arr) = raw.as_array() {
            out.extend(Self::extract_from_array(arr));
        }

        out
    }

    fn extract_from_array(arr: &[Value]) -> Vec<SimpleBalance> {
        let mut out = Vec::new();
        for item in arr {
            // рекурсивно вытаскиваем
            out.extend(Self::extract_balances(item));
        }
        out
    }
}

