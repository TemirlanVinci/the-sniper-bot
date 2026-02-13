// src/strategies/scalper.rs
use crate::config::StrategyConfig;
use crate::strategies::traits::Strategy;
use crate::types::{Position, Side, Signal, Ticker};
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use ta::indicators::{AverageTrueRange, BollingerBands, RelativeStrengthIndex};
use ta::{DataItem, Next};
use tracing::{debug, info}; // Убрал warn

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
    atr: AverageTrueRange,

    current_candle: Option<CandleBuilder>,

    // Состояние индикаторов
    last_rsi_value: f64,
    last_atr_value: f64, // <--- Значение ATR
    last_bb_values: Option<(f64, f64, f64)>,

    position: Option<Position>,

    // Warm-up Logic
    warmup_period: usize,
    processed_candles: usize,

    // Strategy Parameters
    obi_threshold: Decimal,
    min_volatility: f64,
    // Заменили trailing_callback на atr_multiplier
    atr_multiplier: Decimal,
}

impl RsiBollingerStrategy {
    pub fn new(symbol: String, config: StrategyConfig) -> Self {
        Self {
            symbol,
            rsi: RelativeStrengthIndex::new(config.rsi_period).unwrap(),
            bb: BollingerBands::new(config.bb_period, config.bb_std_dev).unwrap(),
            atr: AverageTrueRange::new(14).unwrap(),

            current_candle: None,
            last_rsi_value: 50.0,
            last_atr_value: 0.0,
            last_bb_values: None,
            position: None,

            warmup_period: 50,
            processed_candles: 0,

            obi_threshold: Decimal::from_f64(config.obi_threshold).unwrap_or(Decimal::ZERO),
            min_volatility: config.min_volatility.to_f64().unwrap_or(0.003),
            // Инициализация множителя из конфига (default 2.0 если придет 0)
            atr_multiplier: Decimal::from_f64(config.atr_multiplier).unwrap_or(Decimal::from(2)),
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
        self.last_atr_value = self.atr.next(&item); // Сохраняем актуальный ATR

        let bb_out = self.bb.next(&item);
        self.last_bb_values = Some((bb_out.lower, bb_out.average, bb_out.upper));

        self.processed_candles += 1;
    }
}

#[async_trait]
impl Strategy for RsiBollingerStrategy {
    fn name(&self) -> String {
        "Fut_OBI_Scalper".to_string()
    }

    async fn init(&mut self) -> Result<()> {
        info!(
            "🚀 Strategy {} initialized. Warm-up target: {} candles. ATR Multiplier: {}",
            self.name(),
            self.warmup_period,
            self.atr_multiplier
        );
        Ok(())
    }

    async fn on_tick(&mut self, tick: &Ticker) -> Result<Signal> {
        // 1. Candle Logic
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

        // 2. Warm-up Check
        if self.processed_candles < self.warmup_period {
            if self.processed_candles % 10 == 0 {
                debug!(
                    "Warming up: {} / {} candles",
                    self.processed_candles, self.warmup_period
                );
            }
            return Ok(Signal::Hold);
        }

        // 3. Indicator Extraction
        let (bb_lower_f, _, _) = match self.last_bb_values {
            Some(vals) => vals,
            None => return Ok(Signal::Hold),
        };
        let bb_lower = Decimal::from_f64(bb_lower_f).unwrap_or_default();

        // 4. OBI Calculation
        let total_qty = tick.bid_qty + tick.ask_qty;
        let obi = if !total_qty.is_zero() {
            (tick.bid_qty - tick.ask_qty) / total_qty
        } else {
            Decimal::ZERO
        };

        // 5. Entry/Exit Logic
        match &mut self.position {
            None => {
                // --- VOLATILITY FILTER ---
                let current_atr = self.last_atr_value;
                let current_price = tick.price.to_f64().unwrap_or(1.0);
                let vol_pct = current_atr / current_price;

                if vol_pct < self.min_volatility {
                    return Ok(Signal::Hold);
                }

                // ENTRY LOGIC
                if tick.price < bb_lower && self.last_rsi_value < 30.0 && obi > self.obi_threshold {
                    info!(
                        "⚡ LONG SIGNAL: RSI {:.2} < 30 & OBI {:.2} > {}. Volatility: {:.4}%",
                        self.last_rsi_value,
                        obi,
                        self.obi_threshold,
                        vol_pct * 100.0
                    );
                    return Ok(Signal::Advice(Side::Buy, tick.price));
                }
            }
            Some(pos) => {
                let mut state_changed = false;

                if tick.price > pos.highest_price {
                    pos.highest_price = tick.price;
                    state_changed = true;
                }

                // --- DYNAMIC TRAILING STOP (ATR BASED) ---
                let current_atr_dec = Decimal::from_f64(self.last_atr_value).unwrap_or_default();
                let mut stop_dist = current_atr_dec * self.atr_multiplier;

                // Sanity Check: Минимальный стоп 0.1%, чтобы не выбивало шумом при нулевом ATR
                let min_dist = tick.price * Decimal::from_str("0.001").unwrap();
                if stop_dist < min_dist {
                    stop_dist = min_dist;
                }

                let trailing_stop_price = pos.highest_price - stop_dist;

                if tick.price < trailing_stop_price {
                    info!(
                        "🛡️ DYNAMIC TRAILING: Price {} < Stop {} (High {} - Dist {})",
                        tick.price, trailing_stop_price, pos.highest_price, stop_dist
                    );
                    return Ok(Signal::Advice(Side::Sell, tick.price));
                }

                let hard_stop = pos.entry_price * Decimal::from_str("0.99").unwrap();
                if tick.price < hard_stop {
                    info!("🛑 HARD STOP LOSS");
                    return Ok(Signal::Advice(Side::Sell, tick.price));
                }

                if state_changed {
                    return Ok(Signal::StateChanged);
                }
            }
        }

        Ok(Signal::Hold)
    }

    fn update_position(&mut self, position: Option<Position>) {
        self.position = position;
    }

    fn get_position(&self) -> Option<Position> {
        self.position.clone()
    }
}
