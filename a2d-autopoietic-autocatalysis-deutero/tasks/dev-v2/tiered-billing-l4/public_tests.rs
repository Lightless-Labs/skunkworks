// Inherited clauses (individual citations below)
#[test]
fn public_pricing() {
// R3, R10
{
assert_eq!(quote(6, &Price::Tiers(vec![Tier { up_to: Some(4), unit: 3, start: 2 }, Tier { up_to: None, unit: 2, start: 5 }])).unwrap().amount,23);
}
// R19, R20
{
let b=bill(6, &Price::Tiers(vec![Tier { up_to: Some(4), unit: 3, start: 2 }, Tier { up_to: None, unit: 2, start: 5 }]), Mode::Volume, 0).unwrap(); assert_eq!(b.total,17); assert_eq!(b.charge.bands,vec![Band { index: 1, units: 6, amount: 17 }]);
}
// R18, R22, R23
{
let b=bill(2, &Price::Flat { unit: 3, start: 0 }, Mode::Graduated, 10).unwrap(); assert_eq!(b.total,10); assert_eq!(b.minimum_adjustment,4); assert_eq!(b.charge.amount,6);
}
}

// R26, R31, R32
#[test]
fn public_proration() {
assert_eq!(prorate(300, 30, 0, 15, &[DayRange { start: 2, end: 5 }], Rounding::Down).unwrap(), Proration { active_days:12, amount:120 });
}

// R34, R39, R40, R43, R45, R46
#[test]
fn public_invoice() {
let i=invoice(&[Meter { name: ("cpu").to_string(), quantity: 3, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 40, monthly_fee: 20, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 1000 }], 2000, 5).unwrap(); assert_eq!(i.subtotal,54); assert_eq!(i.discount,10); assert_eq!(i.credit,5); assert_eq!(i.total,39); assert_eq!(i.rendered,"3:cpu|30|10|20|6|54\nTOTAL|54|10|5|39\n");
}

// R42
#[test]
fn public_sorted_lines() {
let i=invoice(&[Meter { name: ("z").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }, Meter { name: ("a").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }], 0, 0).unwrap(); assert_eq!(i.lines.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(),vec!["a","z"]);
}
