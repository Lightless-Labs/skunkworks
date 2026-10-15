// Inherited clauses (individual citations below)
#[test]
fn inherited_0() {
// R1, R2, R3, R14
{
assert_eq!(quote(11, &Price::Flat { unit: 9, start: 13 }).unwrap(), Charge { amount: 112, bands: vec![Band { index: 0, units: 11, amount: 112 }] });
}
// R9, R13
{
assert_eq!(quote(0, &Price::Tiers(vec![Tier { up_to: None, unit: u64::MAX, start: u64::MAX }])).unwrap(), Charge { amount: 0, bands: vec![] });
}
}

// Inherited clauses (individual citations below)
#[test]
fn inherited_1() {
// R4, R11
{
assert_eq!(quote(5, &Price::Tiers(vec![Tier { up_to: Some(5), unit: 2, start: 7 }, Tier { up_to: None, unit: 99, start: 99 }])).unwrap(), Charge { amount: 17, bands: vec![Band { index: 0, units: 5, amount: 17 }] });
}
// R4, R10, R12
{
assert_eq!(quote(6, &Price::Tiers(vec![Tier { up_to: Some(5), unit: 2, start: 7 }, Tier { up_to: None, unit: 4, start: 3 }])).unwrap(), Charge { amount: 24, bands: vec![Band { index: 0, units: 5, amount: 17 }, Band { index: 1, units: 1, amount: 7 }] });
}
}

// Inherited clauses (individual citations below)
#[test]
fn inherited_2() {
// R5
{
assert_eq!(quote(0, &Price::Tiers(vec![])), Err(Error::Empty));
}
// R5
{
let ts = vec![Tier { up_to: Some(0), unit: 0, start: 0 }; 65]; assert_eq!(quote(0, &Price::Tiers(ts)), Err(Error::TooMany));
}
}

// Inherited clauses (individual citations below)
#[test]
fn inherited_3() {
// R6
{
assert_eq!(quote(0, &Price::Tiers(vec![Tier { up_to: Some(0), unit: 1, start: 0 }, Tier { up_to: None, unit: 1, start: 0 }])), Err(Error::Bound { index: 0 }));
}
// R6
{
assert_eq!(quote(1, &Price::Tiers(vec![Tier { up_to: Some(9), unit: 1, start: 0 }, Tier { up_to: Some(9), unit: 2, start: 0 }, Tier { up_to: None, unit: 3, start: 0 }])), Err(Error::Bound { index: 1 }));
}
}

// Inherited clauses (individual citations below)
#[test]
fn inherited_4() {
// R6
{
assert_eq!(quote(1, &Price::Tiers(vec![Tier { up_to: Some(9), unit: 1, start: 0 }, Tier { up_to: Some(8), unit: 2, start: 0 }, Tier { up_to: None, unit: 3, start: 0 }])), Err(Error::Bound { index: 1 }));
}
// R7
{
assert_eq!(quote(0, &Price::Tiers(vec![Tier { up_to: None, unit: 1, start: 0 }, Tier { up_to: Some(0), unit: 1, start: 0 }])), Err(Error::Unbounded { index: 0 }));
}
}

// Inherited clauses (individual citations below)
#[test]
fn inherited_5() {
// R8
{
assert_eq!(quote(1, &Price::Tiers(vec![Tier { up_to: Some(7), unit: 1, start: 0 }])), Err(Error::MissingFinal));
}
// R6, R8
{
assert_eq!(quote(2, &Price::Tiers(vec![Tier { up_to: Some(7), unit: 1, start: 0 }, Tier { up_to: Some(6), unit: 1, start: 0 }])), Err(Error::Bound { index: 1 }));
}
}

// Inherited clauses (individual citations below)
#[test]
fn inherited_6() {
// R9, R15
{
assert_eq!(quote(2, &Price::Tiers(vec![Tier { up_to: Some(3), unit: u64::MAX, start: 0 }, Tier { up_to: Some(3), unit: 1, start: 0 }])), Err(Error::Bound { index: 1 }));
}
// R12, R13
{
assert_eq!(quote(3, &Price::Tiers(vec![Tier { up_to: Some(2), unit: 0, start: 0 }, Tier { up_to: None, unit: 0, start: 0 }])).unwrap(), Charge { amount: 0, bands: vec![Band { index: 0, units: 2, amount: 0 }, Band { index: 1, units: 1, amount: 0 }] });
}
}

// Inherited clauses (individual citations below)
#[test]
fn inherited_7() {
// R10, R16
{
assert_eq!(quote(9, &Price::Tiers(vec![Tier { up_to: Some(2), unit: 10, start: 1 }, Tier { up_to: Some(6), unit: 1, start: 2 }, Tier { up_to: None, unit: 0, start: 3 }])).unwrap(), Charge { amount: 30, bands: vec![Band { index: 0, units: 2, amount: 21 }, Band { index: 1, units: 4, amount: 6 }, Band { index: 2, units: 3, amount: 3 }] });
}
// R15
{
assert_eq!(quote(2, &Price::Flat { unit: u64::MAX, start: 0 }), Err(Error::Overflow { index: 0 }));
}
}

// Inherited clauses (individual citations below)
#[test]
fn inherited_8() {
// R15
{
assert_eq!(quote(1, &Price::Flat { unit: u64::MAX, start: 1 }), Err(Error::Overflow { index: 0 }));
}
// R15
{
assert_eq!(quote(u64::MAX, &Price::Flat { unit: 1, start: 0 }).unwrap().amount, u64::MAX);
}
}

// Inherited clauses (individual citations below)
#[test]
fn inherited_9() {
// R15
{
assert_eq!(quote(2, &Price::Tiers(vec![Tier { up_to: Some(1), unit: u64::MAX, start: 0 }, Tier { up_to: None, unit: 1, start: 0 }])), Err(Error::Overflow { index: 1 }));
}
// R15
{
assert_eq!(quote(3, &Price::Tiers(vec![Tier { up_to: Some(1), unit: 0, start: 0 }, Tier { up_to: None, unit: u64::MAX, start: 0 }])), Err(Error::Overflow { index: 1 }));
}
}

// Inherited clauses (individual citations below)
#[test]
fn inherited_10() {
// R11, R15
{
assert_eq!(quote(1, &Price::Tiers(vec![Tier { up_to: Some(1), unit: 3, start: 0 }, Tier { up_to: None, unit: u64::MAX, start: u64::MAX }])).unwrap().amount, 3);
}
// R4, R11, R15
{
assert_eq!(quote(u64::MAX, &Price::Tiers(vec![Tier { up_to: Some(u64::MAX), unit: 0, start: 6 }, Tier { up_to: None, unit: 1, start: 9 }])).unwrap(), Charge { amount: 6, bands: vec![Band { index: 0, units: u64::MAX, amount: 6 }] });
}
}

// Inherited clauses (individual citations below)
#[test]
fn inherited_11() {
// R5, R10, R12
{
let mut ts: Vec<Tier> = (1..64).map(|n| Tier { up_to: Some(n), unit: 1, start: 1 }).collect(); ts.push(Tier { up_to: None, unit: 1, start: 1 }); let c=quote(64,&Price::Tiers(ts)).unwrap(); assert_eq!(c.amount,128); assert_eq!(c.bands.len(),64); assert_eq!(c.bands[63], Band { index:63, units:1, amount:2 });
}
// R6, R7, R8
{
assert_eq!(quote(0, &Price::Tiers(vec![Tier { up_to: Some(2), unit: 1, start: 0 }, Tier { up_to: Some(1), unit: 1, start: 0 }, Tier { up_to: None, unit: 1, start: 0 }, Tier { up_to: None, unit: 1, start: 0 }])), Err(Error::Bound { index: 1 }));
}
}

// R19, R20
#[test]
fn mode_boundary() {
let b=bill(7, &Price::Tiers(vec![Tier { up_to: Some(7), unit: 9, start: 1 }, Tier { up_to: None, unit: 2, start: 3 }]), Mode::Volume, 0).unwrap(); assert_eq!(b.charge,Charge { amount: 64, bands: vec![Band { index: 0, units: 7, amount: 64 }] });
}

// R19, R20
#[test]
fn mode_next_band() {
let b=bill(8, &Price::Tiers(vec![Tier { up_to: Some(7), unit: 9, start: 1 }, Tier { up_to: None, unit: 2, start: 3 }]), Mode::Volume, 0).unwrap(); assert_eq!(b.charge,Charge { amount: 19, bands: vec![Band { index: 1, units: 8, amount: 19 }] });
}

// R21, R22
#[test]
fn mode_zero_minimum() {
let b=bill(0, &Price::Tiers(vec![Tier { up_to: None, unit: 8, start: 99 }]), Mode::Volume, u64::MAX).unwrap(); assert_eq!(b, Bill { charge: Charge { amount: 0, bands: vec![] }, minimum_adjustment: 0, total: 0 });
}

// R22, R23
#[test]
fn mode_free_usage_minimum() {
let b=bill(19, &Price::Flat { unit: 0, start: 0 }, Mode::Graduated, 31).unwrap(); assert_eq!(b.total,31); assert_eq!(b.minimum_adjustment,31); assert_eq!(b.charge,Charge { amount: 0, bands: vec![Band { index: 0, units: 19, amount: 0 }] });
}

// R22, R23
#[test]
fn mode_minimum_equal_or_below() {
for min in [0,20,21] { let b=bill(3, &Price::Flat { unit: 7, start: 0 }, Mode::Volume, min).unwrap(); assert_eq!(b.total,21); assert_eq!(b.minimum_adjustment,0); }
}

// R18, R21
#[test]
fn mode_flat_modes() {
for qty in [0,1,29] { assert_eq!(bill(qty, &Price::Flat { unit: 6, start: 17 }, Mode::Volume, 90), bill(qty, &Price::Flat { unit: 6, start: 17 }, Mode::Graduated, 90)); }
}

// R18, R21
#[test]
fn mode_validate_unused() {
assert_eq!(bill(0, &Price::Tiers(vec![Tier { up_to: Some(4), unit: 2, start: 0 }, Tier { up_to: Some(3), unit: 0, start: 0 }]), Mode::Volume, 80), Err(Error::Bound { index: 1 }));
}

// R20, R24
#[test]
fn mode_volume_overflow_index() {
assert_eq!(bill(4, &Price::Tiers(vec![Tier { up_to: Some(2), unit: 0, start: 0 }, Tier { up_to: None, unit: u64::MAX, start: 0 }]), Mode::Volume, 0), Err(Error::Overflow { index: 1 }));
}

// R19, R20, R24
#[test]
fn mode_ignore_unused_cost() {
assert_eq!(bill(8, &Price::Tiers(vec![Tier { up_to: Some(2), unit: u64::MAX, start: u64::MAX }, Tier { up_to: None, unit: 1, start: 2 }]), Mode::Volume, 0).unwrap().total,10);
}

// R23, R24
#[test]
fn mode_minimum_max() {
let b=bill(1, &Price::Flat { unit: 1, start: 0 }, Mode::Volume, u64::MAX).unwrap(); assert_eq!(b.total,u64::MAX); assert_eq!(b.minimum_adjustment,u64::MAX-1);
}

// R17, R18, R24
#[test]
fn mode_graduated_overflow() {
assert_eq!(bill(2, &Price::Flat { unit: u64::MAX, start: 0 }, Mode::Graduated, u64::MAX), Err(Error::Overflow { index: 0 }));
}

// R17, R18, R23
#[test]
fn mode_graduated_preserved() {
let p=Price::Tiers(vec![Tier { up_to: Some(3), unit: 5, start: 7 }, Tier { up_to: None, unit: 2, start: 4 }]); let b=bill(9,&p,Mode::Graduated,50).unwrap(); assert_eq!(b.charge,quote(9,&p).unwrap()); assert_eq!(b.charge.amount,38); assert_eq!(b.minimum_adjustment,12);
}
