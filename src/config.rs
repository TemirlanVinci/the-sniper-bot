use config::{Config, ConfigError, File};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct StrategyConfig {
    pub rsi_period: usize,
    pub obi_threshold: f64,
    pub bb_period: usize,
    pub bb_std_dev: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub api_key: String,
    pub secret_key: String,
    pub symbol: String,
    pub leverage: u8,
    pub order_size_usdt: f64, // Renamed from order_qty_usdt
    pub strategy: StrategyConfig,
}

impl AppConfig {
    pub fn new() -> Result<Self, ConfigError> {
        let builder = Config::builder()
            .add_source(File::with_name("Settings"))
            .add_source(config::Environment::with_prefix("APP"));

        let config = builder.build()?;
        config.try_deserialize()
    }
}
