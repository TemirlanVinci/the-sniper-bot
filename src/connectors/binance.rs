// src/connectors/binance.rs
use crate::connectors::traits::{ExchangeClient, StreamClient};
use crate::types::{OrderResponse, Side, Ticker};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use futures_util::StreamExt;
use hmac::{Hmac, Mac};
use reqwest::{Client, Method};
use rust_decimal::prelude::*; // Для парсинга и методов
use rust_decimal::Decimal;
use serde::Deserialize;
use sha2::Sha256;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tracing::{error, info}; // Non-blocking I/O
use url::Url;

type HmacSha256 = Hmac<Sha256>;

pub struct BinanceClient {
    api_key: String,
    secret_key: String,
    http_client: Client,
    base_rest_url: String,
}

impl BinanceClient {
    pub fn new(api_key: String, secret_key: String) -> Self {
        Self {
            api_key,
            secret_key,
            http_client: Client::new(),
            base_rest_url: "https://api.binance.com".to_string(),
        }
    }

    fn sign_and_build_query(&self, params: Vec<(&str, String)>) -> Result<String> {
        let mut params = params;
        let timestamp = Utc::now().timestamp_millis().to_string();
        params.push(("timestamp", timestamp));

        let query_string = serde_urlencoded::to_string(&params)?;

        let mut mac = HmacSha256::new_from_slice(self.secret_key.as_bytes())
            .context("Invalid secret key length")?;
        mac.update(query_string.as_bytes());
        let result = mac.finalize();
        let signature = hex::encode(result.into_bytes());

        Ok(format!("{}&signature={}", query_string, signature))
    }

    async fn send_signed_request<T: for<'de> Deserialize<'de>>(
        &self,
        method: Method,
        endpoint: &str,
        params: Vec<(&str, String)>,
    ) -> Result<T> {
        let full_query = self.sign_and_build_query(params)?;
        let url = format!("{}{}?{}", self.base_rest_url, endpoint, full_query);

        let response = self
            .http_client
            .request(method, &url)
            .header("X-MBX-APIKEY", &self.api_key)
            .send()
            .await?
            .error_for_status()?;

        let json_resp = response.json::<T>().await?;
        Ok(json_resp)
    }
}

#[async_trait]
impl ExchangeClient for BinanceClient {
    async fn connect(&mut self) -> Result<()> {
        let url = format!("{}/api/v3/ping", self.base_rest_url);
        self.http_client
            .get(&url)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    async fn fetch_price(&self, symbol: &str) -> Result<Ticker> {
        let url = format!(
            "{}/api/v3/ticker/price?symbol={}",
            self.base_rest_url, symbol
        );
        let resp = self
            .http_client
            .get(&url)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        let price_str = resp
            .get("price")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Failed to parse price for {}", symbol))?;

        // Парсинг в Decimal
        let price = Decimal::from_str(price_str)?;

        Ok(Ticker {
            symbol: symbol.to_string(),
            price,
            timestamp: Utc::now().timestamp_millis() as u64,
        })
    }

    async fn place_order(
        &self,
        pair: &str,
        side: Side,
        amount: Decimal,
        price: Option<Decimal>,
    ) -> Result<OrderResponse> {
        let side_str = match side {
            Side::Buy => "BUY",
            Side::Sell => "SELL",
        };

        // Slippage Protection & FOK/IOC Logic
        let (type_str, time_in_force, price_val) = match price {
            Some(p) => {
                // Если цена есть, используем LIMIT IOC (Immediate Or Cancel)
                // Это защищает от зависания ордера (Maker) и гарантирует исполнение или отмену
                ("LIMIT", Some("IOC"), Some(p))
            }
            None => {
                // Если цены нет, БЛОКИРУЕМ отправку "голого" маркета в HFT контексте
                // Либо можно реализовать тут же запрос цены, но это задержка.
                // Движок (Engine) должен был рассчитать цену.
                // Fallback: Market (не рекомендуется, но для совместимости оставим с варнингом)
                error!("⚠️ WARNING: Sending MARKET order without protection!");
                ("MARKET", None, None)
            }
        };

        let mut params = vec![
            ("symbol", pair.to_string()),
            ("side", side_str.to_string()),
            ("type", type_str.to_string()),
            ("quantity", amount.to_string()),
        ];

        if let Some(p) = price_val {
            params.push(("price", p.to_string()));
        }
        if let Some(tif) = time_in_force {
            params.push(("timeInForce", tif.to_string()));
        }

        #[derive(Deserialize)]
        struct BinanceOrderResponse {
            #[serde(rename = "orderId")]
            order_id: u64,
            symbol: String,
            status: String,
        }

        info!(
            "🚀 Sending Order: {} {} {} @ {:?}",
            side_str, amount, pair, price_val
        );

        let resp: BinanceOrderResponse = self
            .send_signed_request(Method::POST, "/api/v3/order", params)
            .await?;

        Ok(OrderResponse {
            id: resp.order_id.to_string(),
            symbol: resp.symbol,
            status: resp.status,
        })
    }

    async fn get_balance(&self, asset: &str) -> Result<Decimal> {
        #[derive(Deserialize)]
        struct Balance {
            asset: String,
            free: String,
        }
        #[derive(Deserialize)]
        struct AccountInfo {
            balances: Vec<Balance>,
        }

        let resp: AccountInfo = self
            .send_signed_request(Method::GET, "/api/v3/account", vec![])
            .await?;

        let balance = resp
            .balances
            .iter()
            .find(|b| b.asset == asset)
            .ok_or_else(|| anyhow!("Asset {} not found in account", asset))?;

        Ok(Decimal::from_str(&balance.free)?)
    }

    async fn get_open_orders(&self, _pair: &str) -> Result<Vec<OrderResponse>> {
        // ... (implementation same but updated tracing if needed, abbreviated for length)
        Ok(vec![])
    }
}

#[async_trait]
impl StreamClient for BinanceClient {
    async fn subscribe_ticker(&mut self, symbol: &str, sender: mpsc::Sender<Ticker>) -> Result<()> {
        let ws_url = format!(
            "wss://stream.binance.com:9443/ws/{}@trade",
            symbol.to_lowercase()
        );
        let url = Url::parse(&ws_url)?;

        info!("Starting WebSocket task for: {}", symbol);

        let symbol = symbol.to_string();
        tokio::spawn(async move {
            match connect_async(url).await {
                Ok((ws_stream, _)) => {
                    let (_, mut read) = ws_stream.split();
                    info!("WebSocket connected for {}", symbol);

                    while let Some(message) = read.next().await {
                        match message {
                            Ok(msg) => {
                                if let Ok(text) = msg.to_text() {
                                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
                                        if let Some(price_str) = v.get("p").and_then(|p| p.as_str())
                                        {
                                            // Парсинг Decimal
                                            if let Ok(price) = Decimal::from_str(price_str) {
                                                let ticker = Ticker {
                                                    symbol: symbol.clone(),
                                                    price,
                                                    timestamp: Utc::now().timestamp_millis() as u64,
                                                };
                                                let _ = sender.send(ticker).await;
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => error!("WebSocket Error for {}: {}", symbol, e),
                        }
                    }
                }
                Err(e) => error!("Failed to connect WebSocket for {}: {}", symbol, e),
            }
            info!("WebSocket task finished for {}", symbol);
        });

        Ok(())
    }
}
