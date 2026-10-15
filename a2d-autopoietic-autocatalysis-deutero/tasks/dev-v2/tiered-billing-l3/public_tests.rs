// R3, R10
#[test]
fn public_quote() {
assert_eq!(quote(6, &Price::Tiers(vec![Tier { up_to: Some(4), unit: 3, start: 2 }, Tier { up_to: None, unit: 2, start: 5 }])).unwrap().amount,23);
}

// R19, R20
#[test]
fn public_volume() {
let b=bill(6, &Price::Tiers(vec![Tier { up_to: Some(4), unit: 3, start: 2 }, Tier { up_to: None, unit: 2, start: 5 }]), Mode::Volume, 0).unwrap(); assert_eq!(b.total,17); assert_eq!(b.charge.bands,vec![Band { index: 1, units: 6, amount: 17 }]);
}

// R18, R22, R23
#[test]
fn public_minimum() {
let b=bill(2, &Price::Flat { unit: 3, start: 0 }, Mode::Graduated, 10).unwrap(); assert_eq!(b.total,10); assert_eq!(b.minimum_adjustment,4); assert_eq!(b.charge.amount,6);
}

// R26, R31, R32
#[test]
fn public_proration() {
assert_eq!(prorate(300, 30, 0, 15, &[DayRange { start: 2, end: 5 }], Rounding::Down).unwrap(), Proration { active_days:12, amount:120 });
}
