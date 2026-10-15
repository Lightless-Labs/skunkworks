// R1, R2, R3, R14
#[test]
fn edge_flat_start_once() {
assert_eq!(quote(11, &Price::Flat { unit: 9, start: 13 }).unwrap(), Charge { amount: 112, bands: vec![Band { index: 0, units: 11, amount: 112 }] });
}

// R9, R13
#[test]
fn edge_zero_no_activation() {
assert_eq!(quote(0, &Price::Tiers(vec![Tier { up_to: None, unit: u64::MAX, start: u64::MAX }])).unwrap(), Charge { amount: 0, bands: vec![] });
}

// R4, R11
#[test]
fn edge_inclusive_boundary() {
assert_eq!(quote(5, &Price::Tiers(vec![Tier { up_to: Some(5), unit: 2, start: 7 }, Tier { up_to: None, unit: 99, start: 99 }])).unwrap(), Charge { amount: 17, bands: vec![Band { index: 0, units: 5, amount: 17 }] });
}

// R4, R10, R12
#[test]
fn edge_boundary_plus_one() {
assert_eq!(quote(6, &Price::Tiers(vec![Tier { up_to: Some(5), unit: 2, start: 7 }, Tier { up_to: None, unit: 4, start: 3 }])).unwrap(), Charge { amount: 24, bands: vec![Band { index: 0, units: 5, amount: 17 }, Band { index: 1, units: 1, amount: 7 }] });
}

// R5
#[test]
fn edge_empty() {
assert_eq!(quote(0, &Price::Tiers(vec![])), Err(Error::Empty));
}

// R5
#[test]
fn edge_too_many_precedence() {
let ts = vec![Tier { up_to: Some(0), unit: 0, start: 0 }; 65]; assert_eq!(quote(0, &Price::Tiers(ts)), Err(Error::TooMany));
}

// R6
#[test]
fn edge_zero_bound() {
assert_eq!(quote(0, &Price::Tiers(vec![Tier { up_to: Some(0), unit: 1, start: 0 }, Tier { up_to: None, unit: 1, start: 0 }])), Err(Error::Bound { index: 0 }));
}

// R6
#[test]
fn edge_equal_bound() {
assert_eq!(quote(1, &Price::Tiers(vec![Tier { up_to: Some(9), unit: 1, start: 0 }, Tier { up_to: Some(9), unit: 2, start: 0 }, Tier { up_to: None, unit: 3, start: 0 }])), Err(Error::Bound { index: 1 }));
}

// R6
#[test]
fn edge_descending_bound() {
assert_eq!(quote(1, &Price::Tiers(vec![Tier { up_to: Some(9), unit: 1, start: 0 }, Tier { up_to: Some(8), unit: 2, start: 0 }, Tier { up_to: None, unit: 3, start: 0 }])), Err(Error::Bound { index: 1 }));
}

// R7
#[test]
fn edge_early_unbounded() {
assert_eq!(quote(0, &Price::Tiers(vec![Tier { up_to: None, unit: 1, start: 0 }, Tier { up_to: Some(0), unit: 1, start: 0 }])), Err(Error::Unbounded { index: 0 }));
}

// R8
#[test]
fn edge_missing_final() {
assert_eq!(quote(1, &Price::Tiers(vec![Tier { up_to: Some(7), unit: 1, start: 0 }])), Err(Error::MissingFinal));
}

// R6, R8
#[test]
fn edge_bound_before_missing() {
assert_eq!(quote(2, &Price::Tiers(vec![Tier { up_to: Some(7), unit: 1, start: 0 }, Tier { up_to: Some(6), unit: 1, start: 0 }])), Err(Error::Bound { index: 1 }));
}

// R9, R15
#[test]
fn edge_structure_before_overflow() {
assert_eq!(quote(2, &Price::Tiers(vec![Tier { up_to: Some(3), unit: u64::MAX, start: 0 }, Tier { up_to: Some(3), unit: 1, start: 0 }])), Err(Error::Bound { index: 1 }));
}

// R12, R13
#[test]
fn edge_free_band_retained() {
assert_eq!(quote(3, &Price::Tiers(vec![Tier { up_to: Some(2), unit: 0, start: 0 }, Tier { up_to: None, unit: 0, start: 0 }])).unwrap(), Charge { amount: 0, bands: vec![Band { index: 0, units: 2, amount: 0 }, Band { index: 1, units: 1, amount: 0 }] });
}

// R10, R16
#[test]
fn edge_decreasing_prices() {
assert_eq!(quote(9, &Price::Tiers(vec![Tier { up_to: Some(2), unit: 10, start: 1 }, Tier { up_to: Some(6), unit: 1, start: 2 }, Tier { up_to: None, unit: 0, start: 3 }])).unwrap(), Charge { amount: 30, bands: vec![Band { index: 0, units: 2, amount: 21 }, Band { index: 1, units: 4, amount: 6 }, Band { index: 2, units: 3, amount: 3 }] });
}

// R15
#[test]
fn edge_flat_product_overflow() {
assert_eq!(quote(2, &Price::Flat { unit: u64::MAX, start: 0 }), Err(Error::Overflow { index: 0 }));
}

// R15
#[test]
fn edge_flat_start_overflow() {
assert_eq!(quote(1, &Price::Flat { unit: u64::MAX, start: 1 }), Err(Error::Overflow { index: 0 }));
}

// R15
#[test]
fn edge_exact_max() {
assert_eq!(quote(u64::MAX, &Price::Flat { unit: 1, start: 0 }).unwrap().amount, u64::MAX);
}

// R15
#[test]
fn edge_running_sum_overflow() {
assert_eq!(quote(2, &Price::Tiers(vec![Tier { up_to: Some(1), unit: u64::MAX, start: 0 }, Tier { up_to: None, unit: 1, start: 0 }])), Err(Error::Overflow { index: 1 }));
}

// R15
#[test]
fn edge_later_product_overflow() {
assert_eq!(quote(3, &Price::Tiers(vec![Tier { up_to: Some(1), unit: 0, start: 0 }, Tier { up_to: None, unit: u64::MAX, start: 0 }])), Err(Error::Overflow { index: 1 }));
}

// R11, R15
#[test]
fn edge_unused_huge_prices() {
assert_eq!(quote(1, &Price::Tiers(vec![Tier { up_to: Some(1), unit: 3, start: 0 }, Tier { up_to: None, unit: u64::MAX, start: u64::MAX }])).unwrap().amount, 3);
}

// R4, R11, R15
#[test]
fn edge_max_bound_tail_unused() {
assert_eq!(quote(u64::MAX, &Price::Tiers(vec![Tier { up_to: Some(u64::MAX), unit: 0, start: 6 }, Tier { up_to: None, unit: 1, start: 9 }])).unwrap(), Charge { amount: 6, bands: vec![Band { index: 0, units: u64::MAX, amount: 6 }] });
}

// R5, R10, R12
#[test]
fn edge_sixty_four_tiers() {
let mut ts: Vec<Tier> = (1..64).map(|n| Tier { up_to: Some(n), unit: 1, start: 1 }).collect(); ts.push(Tier { up_to: None, unit: 1, start: 1 }); let c=quote(64,&Price::Tiers(ts)).unwrap(); assert_eq!(c.amount,128); assert_eq!(c.bands.len(),64); assert_eq!(c.bands[63], Band { index:63, units:1, amount:2 });
}

// R6, R7, R8
#[test]
fn edge_first_error_index() {
assert_eq!(quote(0, &Price::Tiers(vec![Tier { up_to: Some(2), unit: 1, start: 0 }, Tier { up_to: Some(1), unit: 1, start: 0 }, Tier { up_to: None, unit: 1, start: 0 }, Tier { up_to: None, unit: 1, start: 0 }])), Err(Error::Bound { index: 1 }));
}
