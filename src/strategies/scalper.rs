// src/strategies/scalper.rs
use crate::strategies::traits::Strategy;
use crate::types::{Position, Side, Signal, Ticker};
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use ta::indicators::{BollingerBands, RelativeStrengthIndex};
use ta::{DataItem, Next};
use tracing::info;

#[derive(Debug, Clone)]
struct CandleBuilder {
    open_time: u64,
    open: Decimal,
    high: Decimal,
    low: Decimal,
    close: Decimal,
}

impl CandleBuilder {
    fn new(tick: &Ticker) -> Self {
        let open_time = (tick.timestamp / 60_000) * 60_000;
        Self {
            open_time,
            open: tick.price,
            high: tick.price,
            low: tick.price,
            close: tick.price,
        }
    }

    fn update(&mut self, tick: &Ticker) {
        if tick.price > self.high {
            self.high = tick.price;
        }
        if tick.price < self.low {
            self.low = tick.price;
        }
        self.close = tick.price;
    }
}

pub struct RsiBollingerStrategy {
    symbol: String,
    rsi: RelativeStrengthIndex,
    bb: BollingerBands,
    current_candle: Option<CandleBuilder>,
    last_rsi_value: f64,
    last_bb_values: Option<(f64, f64, f64)>,
    position: Option<Position>,

    // Параметры стратегии
    obi_threshold: Decimal,     // 0.2
    trailing_callback: Decimal, // 0.002 (0.2%)
}

impl RsiBollingerStrategy {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            rsi: RelativeStrengthIndex::new(14).unwrap(),
            bb: BollingerBands::new(20, 2.0).unwrap(),
            current_candle: None,
            last_rsi_value: 50.0,
            last_bb_values: None,
            position: None,
            obi_threshold: Decimal::from_str("0.2").unwrap(),
            trailing_callback: Decimal::from_str("0.002").unwrap(),
        }
    }

    fn close_candle(&mut self, candle: &CandleBuilder) {
        let item = DataItem::builder()
            .high(candle.high.to_f64().unwrap_or_default())
            .low(candle.low.to_f64().unwrap_or_default())
            .close(candle.close.to_f64().unwrap_or_default())
            .open(candle.open.to_f64().unwrap_or_default())
            .volume(0.0)
            .build()
            .unwrap();

        self.last_rsi_value = self.rsi.next(&item);
        let bb_out = self.bb.next(&item);
        self.last_bb_values = Some((bb_out.lower, bb_out.average, bb_out.upper));
    }
}

#[async_trait]
impl Strategy for RsiBollingerStrategy {
    fn name(&self) -> String {
        "Fut_OBI_Scalper".to_string()
    }

    async fn init(&mut self) -> Result<()> {
        info!("🚀 Strategy {} initialized (Futures Mode)", self.name());
        Ok(())
    }

    async fn on_tick(&mut self, tick: &Ticker) -> Result<Signal> {
        // 1. Candle Logic (Time-based aggregation)
        let tick_minute_start = (tick.timestamp / 60_000) * 60_000;
        match self.current_candle.clone() {
            Some(mut candle) => {
                if tick_minute_start > candle.open_time {
                    self.close_candle(&candle);
                    self.current_candle = Some(CandleBuilder::new(tick));
                } else {
                    candle.update(tick);
                    self.current_candle = Some(candle);
                }
            }
            None => {
                self.current_candle = Some(CandleBuilder::new(tick));
            }
        }

        let (bb_lower_f, _bb_mid_f, _bb_upper_f) = match self.last_bb_values {
            Some(vals) => vals,
            None => return Ok(Signal::Hold),
        };
        let bb_lower = Decimal::from_f64(bb_lower_f).unwrap_or_default();

        // 2. OBI Calculation (Real-time Order Flow)
        // OBI = (BidQty - AskQty) / (BidQty + AskQty)
        let total_qty = tick.bid_qty + tick.ask_qty;
        let obi = if !total_qty.is_zero() {
            (tick.bid_qty - tick.ask_qty) / total_qty
        } else {
            Decimal::ZERO
        };

        match &mut self.position {
            None => {
                // ENTRY LOGIC: RSI Oversold + Positive Order Book Imbalance
                // RSI < 30 AND OBI > 0.2
                if tick.price < bb_lower && self.last_rsi_value < 30.0 && obi > self.obi_threshold {
                    info!(
                        "⚡ LONG SIGNAL: RSI {:.2} < 30 & OBI {:.2} > 0.2",
                        self.last_rsi_value, obi
                    );
                    return Ok(Signal::Advice(Side::Buy, tick.price));
                }
            }
            Some(pos) => {
                // 3. TRAILING STOP LOGIC
                // Обновляем High Watermark
                if tick.price > pos.highest_price {
                    pos.highest_price = tick.price;
                    // info!("📈 New High: {}", pos.highest_price);
                }

                // Расчет цены выхода
                let trailing_stop_price =
                    pos.highest_price * (Decimal::ONE - self.trailing_callback);

                if tick.price < trailing_stop_price {
                    info!(
                        "🛡️ TRAILING STOP: Price {} < High {} - 0.2%",
                        tick.price, pos.highest_price
                    );
                    return Ok(Signal::Advice(Side::Sell, tick.price));
                }

                // Hard Stop Loss (Backup) - 1%
                let hard_stop = pos.entry_price * Decimal::from_str("0.99").unwrap();
                if tick.price < hard_stop {
                    info!("🛑 HARD STOP LOSS");
                    return Ok(Signal::Advice(Side::Sell, tick.price));
                }
            }
        }

        Ok(Signal::Hold)
    }

    fn update_position(&mut self, position: Option<Position>) {
        self.position = position;
    }
}
