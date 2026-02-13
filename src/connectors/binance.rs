// src/connectors/binance.rs
use crate::connectors::messages::BinanceTradeEvent;
use crate::connectors::traits::{ExecutionHandler, StreamClient};
use crate::types::{OrderResponse, Side, Ticker};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use futures_util::StreamExt;
use hmac::{Hmac, Mac};
use reqwest::{Client, Method};
use rust_decimal::Decimal;
use serde::Deserialize;
use sha2::Sha256;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tracing::{error, info, warn};
use url::Url;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
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
impl ExecutionHandler for BinanceClient {
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
            .ok_or_else(|| anyhow!("Asset {} not found", asset))?;

        // Используем parse для Decimal, избегая float
        balance.free.parse::<Decimal>().map_err(|e| anyhow!(e))
    }

    async fn place_order(
        &self,
        symbol: &str,
        side: Side,
        amount: Decimal,
        price: Option<Decimal>,
    ) -> Result<OrderResponse> {
        let side_str = match side {
            Side::Buy => "BUY",
            Side::Sell => "SELL",
        };

        let (type_str, time_in_force, price_val) = match price {
            Some(p) => ("LIMIT", Some("IOC"), Some(p)),
            None => ("MARKET", None, None),
        };

        let mut params = vec![
            ("symbol", symbol.to_string()),
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

        let resp: BinanceOrderResponse = self
            .send_signed_request(Method::POST, "/api/v3/order", params)
            .await?;

        Ok(OrderResponse {
            id: resp.order_id.to_string(),
            symbol: resp.symbol,
            status: resp.status,
        })
    }

    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<()> {
        let params = vec![
            ("symbol", symbol.to_string()),
            ("orderId", order_id.to_string()),
        ];
        let _: serde_json::Value = self
            .send_signed_request(Method::DELETE, "/api/v3/order", params)
            .await?;
        Ok(())
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

        info!("Starting WebSocket task (Hot Path) for: {}", symbol);
        let symbol_clone = symbol.to_string();

        tokio::spawn(async move {
            loop {
                match connect_async(url.clone()).await {
                    Ok((ws_stream, _)) => {
                        info!("WebSocket connected: {}", symbol_clone);
                        let (_, mut read) = ws_stream.split();

                        while let Some(msg_result) = read.next().await {
                            match msg_result {
                                Ok(msg) => {
                                    if let Ok(text) = msg.to_text() {
                                        // 1. Hot Path Optimization: Deserialization
                                        match serde_json::from_str::<BinanceTradeEvent>(text) {
                                            Ok(event) => {
                                                let ticker = Ticker {
                                                    symbol: symbol_clone.clone(),
                                                    price: event.price,
                                                    timestamp: event.event_time,
                                                };

                                                // 2. Backpressure: Drop ticks if channel is full
                                                if let Err(mpsc::error::TrySendError::Full(_)) =
                                                    sender.try_send(ticker)
                                                {
                                                    // Логируем редко или используем метрики, чтобы не спамить в логи
                                                    // warn!("Channel full! Dropping tick for {}", symbol_clone);
                                                }
                                            }
                                            Err(e) => {
                                                // Log deserialization errors only occasionally or debug
                                                error!("Deserialization error: {}", e);
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("WebSocket read error: {}", e);
                                    break; // Reconnect
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to connect WS: {}. Retrying in 5s...", e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }
                }
            }
        });

        Ok(())
    }
}
