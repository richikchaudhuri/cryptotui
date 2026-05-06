//! Integration tests exercising the public indicator API end-to-end.
//! Unit-level tests (math, edge cases) live alongside each module —
//! these tests cover only the public surface.

use cryptotui::indicators::{default_indicators, Bollinger, IndicatorValue, RingBuffer, Rsi};

#[test]
fn ring_buffer_round_trips_through_public_api() {
    let mut rb = RingBuffer::<f64>::new(3).expect("non-zero capacity");
    rb.push(1.0);
    rb.push(2.0);
    rb.push(3.0);
    assert_eq!(rb.push(4.0), Some(1.0));
    let snapshot: Vec<f64> = rb.iter().copied().collect();
    assert_eq!(snapshot, vec![2.0, 3.0, 4.0]);
}

#[test]
fn rsi_warmup_then_smoothing_via_public_api() {
    let mut rsi = Rsi::new(14).expect("non-zero period");
    // 14 ticks of warm-up, none yields a value.
    for i in 1..=14 {
        assert_eq!(rsi.update(i as f64), None);
    }
    // 15th tick yields the first RSI; subsequent ticks keep yielding.
    let first = rsi.update(15.0).expect("first RSI after 15 ticks");
    assert!((0.0..=100.0).contains(&first));
    let next = rsi.update(16.0).expect("smoothing tick");
    assert!((0.0..=100.0).contains(&next));
}

#[test]
fn rsi_default_matches_explicit_period_14() {
    let mut a = Rsi::default();
    let mut b = Rsi::new(14).expect("non-zero period");
    let prices = [
        10.0, 11.0, 10.5, 11.5, 12.0, 11.0, 12.5, 13.0, 12.0, 13.5, 14.0, 13.0, 14.5, 15.0, 14.0,
    ];
    let mut last_a = None;
    let mut last_b = None;
    for p in prices {
        last_a = a.update(p);
        last_b = b.update(p);
    }
    assert_eq!(last_a, last_b);
}

#[test]
fn bollinger_warmup_then_bands_via_public_api() {
    let mut b = Bollinger::new(20, 2.0).expect("valid config");
    for i in 1..=19 {
        assert_eq!(b.update(i as f64), None);
    }
    let bands = b.update(20.0).expect("first bands at tick 20");
    assert!(bands.lower < bands.middle);
    assert!(bands.middle < bands.upper);
}

#[test]
fn default_registry_holds_rsi_and_bollinger() {
    let indicators = default_indicators();
    let names: Vec<&str> = indicators.iter().map(|i| i.name()).collect();
    assert_eq!(names, vec!["rsi", "bollinger"]);
}

#[test]
fn registry_dispatches_via_indicator_trait() {
    // Drive every indicator in the registry from the same tick stream
    // and verify each emits its expected variant after warm-up.
    let mut indicators = default_indicators();
    let prices: Vec<f64> = (1..=30).map(|i| i as f64).collect();
    let mut last_per_indicator: Vec<Option<IndicatorValue>> = vec![None; indicators.len()];
    for p in prices {
        for (i, ind) in indicators.iter_mut().enumerate() {
            if let Some(v) = ind.update(p) {
                last_per_indicator[i] = Some(v);
            }
        }
    }
    let last_rsi = last_per_indicator[0].expect("rsi produced a value");
    assert!(matches!(last_rsi, IndicatorValue::Single(_)));
    let last_boll = last_per_indicator[1].expect("bollinger produced a value");
    assert!(matches!(last_boll, IndicatorValue::Bands(_)));
}
