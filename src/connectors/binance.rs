use crate::connectors::traits::{ExchangeClient, StreamClient};
use crate::types::{OrderResponse, Side, Ticker};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use futures_util::StreamExt;
use hmac::{Hmac, Mac};
// ИСПРАВЛЕНИЕ 1: Убрали 'header' из импортов, чтобы не было warning
use reqwest::{Client, Method};
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

        let price = price_str.parse::<f64>()?;

        // ИСПРАВЛЕНИЕ 2: Добавлено поле timestamp
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
        amount: f64,
        price: Option<f64>,
    ) -> Result<OrderResponse> {
        let side_str = match side {
            Side::Buy => "BUY",
            Side::Sell => "SELL",
        };

        let (type_str, time_in_force) = match price {
            Some(_) => ("LIMIT", Some("GTC")),
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
    async fn subscribe_ticker(&mut self, symbol: &str, sender: mpsc::Sender<Ticker>) -> Result<()> {
        let ws_url = format!(
            "wss://stream.binance.com:9443/ws/{}@trade",
            symbol.to_lowercase()
        );
        let url = Url::parse(&ws_url)?;

        tokio::spawn(async move {
            let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
            let (_, mut read) = ws_stream.split();

            while let Some(msg) = read.next().await {
                if let Ok(m) = msg {
                    if let Ok(text) = m.to_text() {
                        // ... parsing logic here ...
                        // sender.send(ticker).await;
                    }
                }
            }
        });
        Ok(())
    }
}
