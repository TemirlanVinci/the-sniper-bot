// src/connectors/binance.rs
use crate::connectors::messages::BookTickerEvent;
use crate::connectors::traits::{ExecutionHandler, StreamClient};
use crate::types::{OrderResponse, Side, Ticker};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use futures_util::StreamExt;
use hmac::{Hmac, Mac};
use reqwest::{Client, Method};
use rust_decimal::prelude::*;
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
            base_rest_url: "https://fapi.binance.com".to_string(),
        }
    }

    /// Настройка плеча и типа маржи
    pub async fn init_futures_settings(&self, symbol: &str, leverage: u8) -> Result<()> {
        info!("⚙️ Configuring Futures: Leverage {}x, Isolated", leverage);

        // 1. Margin Type
        let _ = self
            .send_signed_request::<serde_json::Value>(
                Method::POST,
                "/fapi/v1/marginType",
                vec![
                    ("symbol", symbol.to_string()),
                    ("marginType", "ISOLATED".to_string()),
                ],
            )
            .await;

        // 2. Leverage
        let _ = self
            .send_signed_request::<serde_json::Value>(
                Method::POST,
                "/fapi/v1/leverage",
                vec![
                    ("symbol", symbol.to_string()),
                    ("leverage", leverage.to_string()),
                ],
            )
            .await?;

        Ok(())
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
        struct Asset {
            asset: String,
            #[serde(rename = "walletBalance")]
            wallet_balance: String,
        }
        #[derive(Deserialize)]
        struct AccountInfo {
            assets: Vec<Asset>,
        }

        let resp: AccountInfo = self
            .send_signed_request(Method::GET, "/fapi/v2/account", vec![])
            .await?;

        let balance = resp
            .assets
            .iter()
            .find(|b| b.asset == asset)
            .ok_or_else(|| anyhow!("Asset {} not found in Futures wallet", asset))?;

        balance
            .wallet_balance
            .parse::<Decimal>()
            .map_err(|e| anyhow!(e))
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

        // FIXED: Changed "GTC" to "IOC" to prevent orders from resting in the book
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
            .send_signed_request(Method::POST, "/fapi/v1/order", params)
            .await?;

        // FIXED: Check status. Fail if not filled immediately.
        match resp.status.as_str() {
            "FILLED" | "PARTIALLY_FILLED" => Ok(OrderResponse {
                id: resp.order_id.to_string(),
                symbol: resp.symbol,
                status: resp.status,
            }),
            // Treat EXPIRED (IOC not met) or CANCELED as a failure
            _ => Err(anyhow!(
                "Order not filled (Slippage/IOC). Status: {}",
                resp.status
            )),
        }
    }

    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<()> {
        let params = vec![
            ("symbol", symbol.to_string()),
            ("orderId", order_id.to_string()),
        ];
        let _: serde_json::Value = self
            .send_signed_request(Method::DELETE, "/fapi/v1/order", params)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl StreamClient for BinanceClient {
    async fn subscribe_ticker(&mut self, symbol: &str, sender: mpsc::Sender<Ticker>) -> Result<()> {
        let ws_url = format!(
            "wss://fstream.binance.com/ws/{}@bookTicker",
            symbol.to_lowercase()
        );
        let url = Url::parse(&ws_url)?;
        let symbol_clone = symbol.to_string();

        info!("🔌 Initializing WebSocket connection for {}...", symbol);

        tokio::spawn(async move {
            loop {
                info!("Connecting to WS: {}", url);
                match connect_async(url.clone()).await {
                    Ok((ws_stream, _)) => {
                        info!("✅ WS Connected: {}", symbol_clone);
                        let (_, mut read) = ws_stream.split();

                        while let Some(msg_result) = read.next().await {
                            match msg_result {
                                Ok(msg) => {
                                    if let Ok(text) = msg.to_text() {
                                        if let Ok(event) =
                                            serde_json::from_str::<BookTickerEvent>(text)
                                        {
                                            let mid_price = (event.best_bid_price
                                                + event.best_ask_price)
                                                / Decimal::from(2);

                                            let ticker = Ticker {
                                                symbol: symbol_clone.clone(),
                                                price: mid_price,
                                                bid_price: event.best_bid_price,
                                                ask_price: event.best_ask_price,
                                                bid_qty: event.best_bid_qty,
                                                ask_qty: event.best_ask_qty,
                                                timestamp: event.event_time,
                                            };

                                            if sender.try_send(ticker).is_err() {
                                                // Channel full/closed
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("❌ WS Read Error: {}. Reconnecting...", e);
                                    break; // Break inner loop to trigger reconnect
                                }
                            }
                        }
                        warn!("⚠️ WS Stream ended. Reconnecting...");
                    }
                    Err(e) => {
                        error!("❌ WS Connection Failed: {}. Retrying in 5s...", e);
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        });

        Ok(())
    }
}
