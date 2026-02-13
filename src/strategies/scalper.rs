// src/strategies/scalper.rs
use crate::strategies::traits::Strategy;
use crate::types::{Position, Side, Signal, Ticker};
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::prelude::*; // ToPrimitive, FromPrimitive
use rust_decimal::Decimal;
use ta::indicators::{BollingerBands, RelativeStrengthIndex};
use ta::{DataItem, Next};
use tracing::info;

/// Структура для накопления данных "виртуальной" минутной свечи
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
    last_rsi_value: f64, // Indicators work with f64 internally
    last_bb_values: Option<(f64, f64, f64)>,
    position: Option<Position>,
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
        }
    }

    fn close_candle(&mut self, candle: &CandleBuilder) {
        // Convert Decimal to f64 for `ta` crate
        let high_f = candle.high.to_f64().unwrap_or_default();
        let low_f = candle.low.to_f64().unwrap_or_default();
        let close_f = candle.close.to_f64().unwrap_or_default();
        let open_f = candle.open.to_f64().unwrap_or_default();

        let item = DataItem::builder()
            .high(high_f)
            .low(low_f)
            .close(close_f)
            .open(open_f)
            .volume(0.0)
            .build()
            .unwrap();

        self.last_rsi_value = self.rsi.next(&item);
        let bb_out = self.bb.next(&item);
        self.last_bb_values = Some((bb_out.lower, bb_out.average, bb_out.upper));

        info!(
            "🕯 Candle Closed [{}]: Close=${} | RSI={:.2} | BB_Low={:.2}",
            self.symbol, candle.close, self.last_rsi_value, bb_out.lower
        );
    }
}

#[async_trait]
impl Strategy for RsiBollingerStrategy {
    fn name(&self) -> String {
        "RsiBollingerScalper".to_string()
    }

    async fn init(&mut self) -> Result<()> {
        info!(
            "🚀 Strategy {} initialized for {}",
            self.name(),
            self.symbol
        );
        Ok(())
    }

    async fn on_tick(&mut self, tick: &Ticker) -> Result<Signal> {
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

        let (bb_lower_f, bb_mid_f, _bb_upper_f) = match self.last_bb_values {
            Some(vals) => vals,
            None => return Ok(Signal::Hold),
        };

        // Convert thresholds to Decimal for precise comparison logic
        let bb_lower = Decimal::from_f64(bb_lower_f).unwrap_or_default();
        let bb_mid = Decimal::from_f64(bb_mid_f).unwrap_or_default();

        match &self.position {
            None => {
                if tick.price < bb_lower && self.last_rsi_value < 30.0 {
                    info!(
                        "⚡ SIGNAL BUY: Price {} < BB_Low {} AND RSI {:.2} < 30",
                        tick.price, bb_lower, self.last_rsi_value
                    );
                    return Ok(Signal::Advice(Side::Buy, tick.price));
                }
            }
            Some(pos) => {
                // Stop Loss -1.0% using Decimal
                let stop_loss_price = pos.entry_price * Decimal::from_str("0.99").unwrap();
                if tick.price <= stop_loss_price {
                    info!(
                        "🛑 STOP LOSS TRIGGERED: {} <= {}",
                        tick.price, stop_loss_price
                    );
                    return Ok(Signal::Advice(Side::Sell, tick.price));
                }

                // Take Profit
                let tp_condition_1 = tick.price >= bb_mid;
                let tp_condition_2 = self.last_rsi_value > 50.0;

                if tp_condition_1 || tp_condition_2 {
                    info!(
                        "💰 TAKE PROFIT: Price {} >= BB_Mid {} OR RSI {:.2} > 50",
                        tick.price, bb_mid, self.last_rsi_value
                    );
                    return Ok(Signal::Advice(Side::Sell, tick.price));
                }
            }
        }

        Ok(Signal::Hold)
    }

    fn update_position(&mut self, position: Option<Position>) {
        if let Some(ref pos) = position {
            info!("✅ Position OPENED: {} @ ${}", pos.symbol, pos.entry_price);
        } else {
            info!("❎ Position CLOSED");
        }
        self.position = position;
    }
}
