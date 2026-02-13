// src/utils/precision.rs
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal; // Для доступа к методам округления, если понадобятся, но основные есть у Decimal

/// Округляет количество ВНИЗ до ближайшего кратного step_size.
/// Пример: amount=10.999, step=1.0 -> 10.0
pub fn normalize_quantity(amount: Decimal, step_size: Decimal) -> Decimal {
    if step_size.is_zero() {
        return amount;
    }
    // (amount / step_size).floor() * step_size
    (amount / step_size).floor() * step_size
}

/// Округляет цену до БЛИЖАЙШЕГО кратного tick_size.
/// Пример: price=100.16, tick=0.1 -> 100.2
pub fn normalize_price(price: Decimal, tick_size: Decimal) -> Decimal {
    if tick_size.is_zero() {
        return price;
    }
    // (price / tick_size).round() * tick_size
    (price / tick_size).round() * tick_size
}
