// R3, R14
#[test]
fn public_flat() {
assert_eq!(quote(3, &Price::Flat { unit: 7, start: 2 }).unwrap(), Charge { amount: 23, bands: vec![Band { index: 0, units: 3, amount: 23 }] });
}

// R4, R10, R12
#[test]
fn public_graduated() {
assert_eq!(quote(6, &Price::Tiers(vec![Tier { up_to: Some(4), unit: 3, start: 2 }, Tier { up_to: None, unit: 2, start: 5 }])).unwrap(), Charge { amount: 23, bands: vec![Band { index: 0, units: 4, amount: 14 }, Band { index: 1, units: 2, amount: 9 }] });
}

// R5, R13
#[test]
fn public_zero_and_validation() {
assert_eq!(quote(0, &Price::Flat { unit: 8, start: 4 }).unwrap(), Charge { amount: 0, bands: vec![] }); assert_eq!(quote(1, &Price::Tiers(vec![])), Err(Error::Empty));
}
