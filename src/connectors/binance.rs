use crate::connectors::traits::{ExchangeClient, StreamClient};
use crate::types::{OrderResponse, Side, Ticker};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use futures_util::StreamExt;
use hmac::{Hmac, Mac};
use reqwest::{header, Client, Method};
use serde::Deserialize;
use sha2::Sha256;
use tokio_tungstenite::connect_async;
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

    /// Generates the HMAC-SHA256 signature and returns the full query string
    fn sign_and_build_query(&self, params: Vec<(&str, String)>) -> Result<String> {
        // 1. Append timestamp (essential for security/replay protection)
        let mut params = params;
        let timestamp = Utc::now().timestamp_millis().to_string();
        params.push(("timestamp", timestamp));

        // 2. Create the query string (e.g., "symbol=BTCUSDT&timestamp=1600000000")
        let query_string = serde_urlencoded::to_string(&params)?;

        // 3. Create HMAC signature
        let mut mac = HmacSha256::new_from_slice(self.secret_key.as_bytes())
            .context("Invalid secret key length")?;
        mac.update(query_string.as_bytes());
        let result = mac.finalize();
        let signature = hex::encode(result.into_bytes());

        // 4. Return full query with signature appended
        Ok(format!("{}&signature={}", query_string, signature))
    }

    /// Helper for authenticated requests
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
        // Simple connectivity check to Binance API
        let url = format!("{}/api/v3/ping", self.base_rest_url);
        self.http_client
            .get(&url)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    async fn fetch_price(&self, symbol: &str) -> Result<Ticker> {
        // Public endpoint, no signature needed
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

        let price = price_str.parse::<f64>()?;

        Ok(Ticker {
            symbol: symbol.to_string(),
            price,
        })
    }

    async fn place_order(
        &self,
        pair: &str,
        side: Side,
        amount: f64,
        price: Option<f64>,
    ) -> Result<OrderResponse> {
        let side_str = match side {
            Side::Buy => "BUY",
            Side::Sell => "SELL",
        };

        // Determine order type based on price presence
        let (type_str, time_in_force) = match price {
            Some(_) => ("LIMIT", Some("GTC")), // Good Till Cancelled
            None => ("MARKET", None),
        };

        let mut params = vec![
            ("symbol", pair.to_string()),
            ("side", side_str.to_string()),
            ("type", type_str.to_string()),
            ("quantity", amount.to_string()),
        ];

        if let Some(p) = price {
            params.push(("price", p.to_string()));
        }
        if let Some(tif) = time_in_force {
            params.push(("timeInForce", tif.to_string()));
        }

        // Use a temporary struct for deserialization to map Binance response to our generic OrderResponse
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

    async fn get_balance(&self, asset: &str) -> Result<f64> {
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

        // Find the specific asset in the balances list
        let balance = resp
            .balances
            .iter()
            .find(|b| b.asset == asset)
            .ok_or_else(|| anyhow!("Asset {} not found in account", asset))?;

        Ok(balance.free.parse()?)
    }

    async fn get_open_orders(&self, pair: &str) -> Result<Vec<OrderResponse>> {
        #[derive(Deserialize)]
        struct BinanceOpenOrder {
            #[serde(rename = "orderId")]
            order_id: u64,
            symbol: String,
            status: String,
        }

        let resp: Vec<BinanceOpenOrder> = self
            .send_signed_request(
                Method::GET,
                "/api/v3/openOrders",
                vec![("symbol", pair.to_string())],
            )
            .await?;

        let orders = resp
            .into_iter()
            .map(|o| OrderResponse {
                id: o.order_id.to_string(),
                symbol: o.symbol,
                status: o.status,
            })
            .collect();

        Ok(orders)
    }
}

#[async_trait]
impl StreamClient for BinanceClient {
    async fn subscribe_ticker(&mut self, symbol: &str) -> Result<()> {
        let ws_url = format!(
            "wss://stream.binance.com:9443/ws/{}@trade",
            symbol.to_lowercase()
        );
        let url = Url::parse(&ws_url)?;

        println!("Starting WebSocket task for: {}", symbol);

        // Spawn a detached background task
        tokio::spawn(async move {
            match connect_async(url).await {
                Ok((ws_stream, _)) => {
                    let (_, mut read) = ws_stream.split();
                    println!("WebSocket connected for {}", symbol);

                    while let Some(message) = read.next().await {
                        match message {
                            Ok(msg) => {
                                if msg.is_text() || msg.is_binary() {
                                    // In a real app, you'd deserialize and send to a channel.
                                    // For this task: simply print to stdout.
                                    println!("{} Stream Data: {}", symbol, msg);
                                }
                            }
                            Err(e) => eprintln!("WebSocket Error for {}: {}", symbol, e),
                        }
                    }
                }
                Err(e) => eprintln!("Failed to connect WebSocket for {}: {}", symbol, e),
            }
            println!("WebSocket task finished for {}", symbol);
        });

        Ok(())
    }
}
