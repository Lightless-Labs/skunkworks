// Inherited clauses (individual citations below)
#[test]
fn inherited_core() {
// R1, R2, R3, R14
{
assert_eq!(quote(11, &Price::Flat { unit: 9, start: 13 }).unwrap(), Charge { amount: 112, bands: vec![Band { index: 0, units: 11, amount: 112 }] });
}
// R9, R13
{
assert_eq!(quote(0, &Price::Tiers(vec![Tier { up_to: None, unit: u64::MAX, start: u64::MAX }])).unwrap(), Charge { amount: 0, bands: vec![] });
}
// R4, R11
{
assert_eq!(quote(5, &Price::Tiers(vec![Tier { up_to: Some(5), unit: 2, start: 7 }, Tier { up_to: None, unit: 99, start: 99 }])).unwrap(), Charge { amount: 17, bands: vec![Band { index: 0, units: 5, amount: 17 }] });
}
// R4, R10, R12
{
assert_eq!(quote(6, &Price::Tiers(vec![Tier { up_to: Some(5), unit: 2, start: 7 }, Tier { up_to: None, unit: 4, start: 3 }])).unwrap(), Charge { amount: 24, bands: vec![Band { index: 0, units: 5, amount: 17 }, Band { index: 1, units: 1, amount: 7 }] });
}
// R5
{
assert_eq!(quote(0, &Price::Tiers(vec![])), Err(Error::Empty));
}
// R5
{
let ts = vec![Tier { up_to: Some(0), unit: 0, start: 0 }; 65]; assert_eq!(quote(0, &Price::Tiers(ts)), Err(Error::TooMany));
}
// R6
{
assert_eq!(quote(0, &Price::Tiers(vec![Tier { up_to: Some(0), unit: 1, start: 0 }, Tier { up_to: None, unit: 1, start: 0 }])), Err(Error::Bound { index: 0 }));
}
// R6
{
assert_eq!(quote(1, &Price::Tiers(vec![Tier { up_to: Some(9), unit: 1, start: 0 }, Tier { up_to: Some(9), unit: 2, start: 0 }, Tier { up_to: None, unit: 3, start: 0 }])), Err(Error::Bound { index: 1 }));
}
// R6
{
assert_eq!(quote(1, &Price::Tiers(vec![Tier { up_to: Some(9), unit: 1, start: 0 }, Tier { up_to: Some(8), unit: 2, start: 0 }, Tier { up_to: None, unit: 3, start: 0 }])), Err(Error::Bound { index: 1 }));
}
// R7
{
assert_eq!(quote(0, &Price::Tiers(vec![Tier { up_to: None, unit: 1, start: 0 }, Tier { up_to: Some(0), unit: 1, start: 0 }])), Err(Error::Unbounded { index: 0 }));
}
// R8
{
assert_eq!(quote(1, &Price::Tiers(vec![Tier { up_to: Some(7), unit: 1, start: 0 }])), Err(Error::MissingFinal));
}
// R6, R8
{
assert_eq!(quote(2, &Price::Tiers(vec![Tier { up_to: Some(7), unit: 1, start: 0 }, Tier { up_to: Some(6), unit: 1, start: 0 }])), Err(Error::Bound { index: 1 }));
}
// R9, R15
{
assert_eq!(quote(2, &Price::Tiers(vec![Tier { up_to: Some(3), unit: u64::MAX, start: 0 }, Tier { up_to: Some(3), unit: 1, start: 0 }])), Err(Error::Bound { index: 1 }));
}
// R12, R13
{
assert_eq!(quote(3, &Price::Tiers(vec![Tier { up_to: Some(2), unit: 0, start: 0 }, Tier { up_to: None, unit: 0, start: 0 }])).unwrap(), Charge { amount: 0, bands: vec![Band { index: 0, units: 2, amount: 0 }, Band { index: 1, units: 1, amount: 0 }] });
}
// R10, R16
{
assert_eq!(quote(9, &Price::Tiers(vec![Tier { up_to: Some(2), unit: 10, start: 1 }, Tier { up_to: Some(6), unit: 1, start: 2 }, Tier { up_to: None, unit: 0, start: 3 }])).unwrap(), Charge { amount: 30, bands: vec![Band { index: 0, units: 2, amount: 21 }, Band { index: 1, units: 4, amount: 6 }, Band { index: 2, units: 3, amount: 3 }] });
}
// R15
{
assert_eq!(quote(2, &Price::Flat { unit: u64::MAX, start: 0 }), Err(Error::Overflow { index: 0 }));
}
// R15
{
assert_eq!(quote(1, &Price::Flat { unit: u64::MAX, start: 1 }), Err(Error::Overflow { index: 0 }));
}
// R15
{
assert_eq!(quote(u64::MAX, &Price::Flat { unit: 1, start: 0 }).unwrap().amount, u64::MAX);
}
// R15
{
assert_eq!(quote(2, &Price::Tiers(vec![Tier { up_to: Some(1), unit: u64::MAX, start: 0 }, Tier { up_to: None, unit: 1, start: 0 }])), Err(Error::Overflow { index: 1 }));
}
// R15
{
assert_eq!(quote(3, &Price::Tiers(vec![Tier { up_to: Some(1), unit: 0, start: 0 }, Tier { up_to: None, unit: u64::MAX, start: 0 }])), Err(Error::Overflow { index: 1 }));
}
// R11, R15
{
assert_eq!(quote(1, &Price::Tiers(vec![Tier { up_to: Some(1), unit: 3, start: 0 }, Tier { up_to: None, unit: u64::MAX, start: u64::MAX }])).unwrap().amount, 3);
}
// R4, R11, R15
{
assert_eq!(quote(u64::MAX, &Price::Tiers(vec![Tier { up_to: Some(u64::MAX), unit: 0, start: 6 }, Tier { up_to: None, unit: 1, start: 9 }])).unwrap(), Charge { amount: 6, bands: vec![Band { index: 0, units: u64::MAX, amount: 6 }] });
}
// R5, R10, R12
{
let mut ts: Vec<Tier> = (1..64).map(|n| Tier { up_to: Some(n), unit: 1, start: 1 }).collect(); ts.push(Tier { up_to: None, unit: 1, start: 1 }); let c=quote(64,&Price::Tiers(ts)).unwrap(); assert_eq!(c.amount,128); assert_eq!(c.bands.len(),64); assert_eq!(c.bands[63], Band { index:63, units:1, amount:2 });
}
// R6, R7, R8
{
assert_eq!(quote(0, &Price::Tiers(vec![Tier { up_to: Some(2), unit: 1, start: 0 }, Tier { up_to: Some(1), unit: 1, start: 0 }, Tier { up_to: None, unit: 1, start: 0 }, Tier { up_to: None, unit: 1, start: 0 }])), Err(Error::Bound { index: 1 }));
}
}

// Inherited clauses (individual citations below)
#[test]
fn inherited_modes() {
// R19, R20
{
let b=bill(7, &Price::Tiers(vec![Tier { up_to: Some(7), unit: 9, start: 1 }, Tier { up_to: None, unit: 2, start: 3 }]), Mode::Volume, 0).unwrap(); assert_eq!(b.charge,Charge { amount: 64, bands: vec![Band { index: 0, units: 7, amount: 64 }] });
}
// R19, R20
{
let b=bill(8, &Price::Tiers(vec![Tier { up_to: Some(7), unit: 9, start: 1 }, Tier { up_to: None, unit: 2, start: 3 }]), Mode::Volume, 0).unwrap(); assert_eq!(b.charge,Charge { amount: 19, bands: vec![Band { index: 1, units: 8, amount: 19 }] });
}
// R21, R22
{
let b=bill(0, &Price::Tiers(vec![Tier { up_to: None, unit: 8, start: 99 }]), Mode::Volume, u64::MAX).unwrap(); assert_eq!(b, Bill { charge: Charge { amount: 0, bands: vec![] }, minimum_adjustment: 0, total: 0 });
}
// R22, R23
{
let b=bill(19, &Price::Flat { unit: 0, start: 0 }, Mode::Graduated, 31).unwrap(); assert_eq!(b.total,31); assert_eq!(b.minimum_adjustment,31); assert_eq!(b.charge,Charge { amount: 0, bands: vec![Band { index: 0, units: 19, amount: 0 }] });
}
// R22, R23
{
for min in [0,20,21] { let b=bill(3, &Price::Flat { unit: 7, start: 0 }, Mode::Volume, min).unwrap(); assert_eq!(b.total,21); assert_eq!(b.minimum_adjustment,0); }
}
// R18, R21
{
for qty in [0,1,29] { assert_eq!(bill(qty, &Price::Flat { unit: 6, start: 17 }, Mode::Volume, 90), bill(qty, &Price::Flat { unit: 6, start: 17 }, Mode::Graduated, 90)); }
}
// R18, R21
{
assert_eq!(bill(0, &Price::Tiers(vec![Tier { up_to: Some(4), unit: 2, start: 0 }, Tier { up_to: Some(3), unit: 0, start: 0 }]), Mode::Volume, 80), Err(Error::Bound { index: 1 }));
}
// R20, R24
{
assert_eq!(bill(4, &Price::Tiers(vec![Tier { up_to: Some(2), unit: 0, start: 0 }, Tier { up_to: None, unit: u64::MAX, start: 0 }]), Mode::Volume, 0), Err(Error::Overflow { index: 1 }));
}
// R19, R20, R24
{
assert_eq!(bill(8, &Price::Tiers(vec![Tier { up_to: Some(2), unit: u64::MAX, start: u64::MAX }, Tier { up_to: None, unit: 1, start: 2 }]), Mode::Volume, 0).unwrap().total,10);
}
// R23, R24
{
let b=bill(1, &Price::Flat { unit: 1, start: 0 }, Mode::Volume, u64::MAX).unwrap(); assert_eq!(b.total,u64::MAX); assert_eq!(b.minimum_adjustment,u64::MAX-1);
}
// R17, R18, R24
{
assert_eq!(bill(2, &Price::Flat { unit: u64::MAX, start: 0 }, Mode::Graduated, u64::MAX), Err(Error::Overflow { index: 0 }));
}
// R17, R18, R23
{
let p=Price::Tiers(vec![Tier { up_to: Some(3), unit: 5, start: 7 }, Tier { up_to: None, unit: 2, start: 4 }]); let b=bill(9,&p,Mode::Graduated,50).unwrap(); assert_eq!(b.charge,quote(9,&p).unwrap()); assert_eq!(b.charge.amount,38); assert_eq!(b.minimum_adjustment,12);
}
}

// Inherited clauses (individual citations below)
#[test]
fn inherited_days() {
// R25, R26
{
assert_eq!(prorate(0, 27, 9, 1, &[DayRange { start: 0, end: 99 }], Rounding::HalfEven),Err(Error::Month)); assert_eq!(prorate(1, 32, 0, 1, &[], Rounding::HalfEven),Err(Error::Month));
}
// R27
{
assert_eq!(prorate(0, 30, 8, 7, &[DayRange { start: 0, end: 99 }], Rounding::HalfEven),Err(Error::Period)); assert_eq!(prorate(1, 28, 0, 29, &[], Rounding::HalfEven),Err(Error::Period));
}
// R27, R30
{
assert_eq!(prorate(u64::MAX, 31, 31, 31, &[], Rounding::HalfEven).unwrap(),Proration { active_days:0, amount:0 }); assert_eq!(prorate(0, 30, 4, 4, &[DayRange { start: 4, end: 5 }], Rounding::HalfEven),Err(Error::Credit { index:0 }));
}
// R28
{
for (a,b) in [(2,4),(4,4),(6,5),(6,10)] { assert_eq!(prorate(0, 30, 3, 9, &[DayRange { start: a, end: b }], Rounding::HalfEven),Err(Error::Credit { index:0 })); }
}
// R29
{
for credits in [&[DayRange { start: 5, end: 7 }, DayRange { start: 4, end: 5 }],&[DayRange { start: 4, end: 7 }, DayRange { start: 6, end: 8 }]] { assert_eq!(prorate(100, 30, 3, 9, credits, Rounding::HalfEven),Err(Error::CreditOrder { index:1 })); }
}
// R28, R29
{
assert_eq!(prorate(0, 30, 3, 12, &[DayRange { start: 8, end: 10 }, DayRange { start: 2, end: 4 }], Rounding::HalfEven),Err(Error::Credit { index:1 }));
}
// R29, R30, R31
{
assert_eq!(prorate(999, 29, 4, 20, &[DayRange { start: 4, end: 11 }, DayRange { start: 11, end: 20 }], Rounding::HalfEven).unwrap(),Proration { active_days:0,amount:0 });
}
// R31
{
assert_eq!(prorate(310, 31, 7, 17, &[DayRange { start: 9, end: 12 }], Rounding::HalfEven).unwrap(),Proration { active_days:7,amount:70 });
}
// R32
{
assert_eq!(prorate(7, 28, 0, 14, &[], Rounding::HalfEven).unwrap().amount,4); assert_eq!(prorate(5, 28, 0, 14, &[], Rounding::HalfEven).unwrap().amount,2); assert_eq!(prorate(1, 30, 0, 15, &[], Rounding::HalfEven).unwrap().amount,0);
}
// R32
{
assert_eq!(prorate(5, 31, 0, 15, &[], Rounding::HalfEven).unwrap().amount,2); assert_eq!(prorate(5, 31, 0, 16, &[], Rounding::HalfEven).unwrap().amount,3);
}
// R32
{
assert_eq!(prorate(1, 29, 0, 1, &[], Rounding::Up).unwrap().amount,1); assert_eq!(prorate(1, 29, 0, 28, &[], Rounding::Down).unwrap().amount,0); assert_eq!(prorate(29, 29, 0, 2, &[], Rounding::Up).unwrap().amount,2);
}
// R31, R32
{
assert_eq!(prorate(7, 30, 0, 10, &[DayRange { start: 1, end: 2 }, DayRange { start: 5, end: 6 }], Rounding::Up).unwrap(), Proration { active_days:8,amount:2 });
}
// R33
{
for days in 28..=31 { for mode in [Rounding::Down,Rounding::Up,Rounding::HalfEven] { assert_eq!(prorate(u64::MAX,days,0,days,&[],mode).unwrap().amount,u64::MAX); } } assert_eq!(prorate(u64::MAX, 30, 0, 15, &[], Rounding::HalfEven).unwrap().amount,9223372036854775808);
}
// R28, R30
{
assert_eq!(prorate(0, 30, 0, 9, &[DayRange { start: 0, end: 9 }, DayRange { start: 8, end: 9 }], Rounding::HalfEven),Err(Error::CreditOrder { index:1 }));
}
}

// R35, R47
#[test]
fn invoice_global_precedence() {
assert_eq!(invoice(&[Meter { name: ("").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 10001 }], 10001, 0), Err(Error::InvoiceDiscount)); assert_eq!(invoice(&[],10001,0),Err(Error::InvoiceDiscount));
}

// R34, R44, R46
#[test]
fn invoice_empty() {
let i=invoice(&[],5000,u64::MAX).unwrap(); assert_eq!(i, Invoice { lines:vec![], subtotal:0, discount:0, credit:0, total:0, rendered:"TOTAL|0|0|0|0
".into() });
}

// R36
#[test]
fn invoice_invalid_names() {
for name in ["", "bad|x", "bad:x", "x\ny", "x\ry", "\t", "\0", "\u{7f}"] { assert_eq!(invoice(&[Meter { name: (name).to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }], 0, 0),Err(Error::InvalidName { index:0 })); }
}

// R36, R45
#[test]
fn invoice_name_bytes() {
let valid="é".repeat(16); assert_eq!(invoice(&[Meter { name: (valid.clone()).to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }], 0, 0).unwrap().lines[0].name,valid); let invalid="é".repeat(17); assert_eq!(invoice(&[Meter { name: (invalid).to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }], 0, 0),Err(Error::InvalidName { index:0 }));
}

// R36, R38
#[test]
fn invoice_name_pass_first() {
let mut a=Meter { name: ("cpu").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }; a.price=Price::Tiers(vec![]); assert_eq!(invoice(&[a, Meter { name: ("bad:later").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }], 0, 0),Err(Error::InvalidName { index:1 }));
}

// R37
#[test]
fn invoice_duplicate_index() {
assert_eq!(invoice(&[Meter { name: ("X").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }, Meter { name: ("x").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }, Meter { name: ("X").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }], 0, 0),Err(Error::DuplicateName { index:2 }));
}

// R36, R37
#[test]
fn invoice_first_name_error() {
assert_eq!(invoice(&[Meter { name: ("a").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }, Meter { name: ("a").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }, Meter { name: ("").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }], 0, 0),Err(Error::DuplicateName { index:1 }));
}

// R38
#[test]
fn invoice_meter_discount_precedence() {
let mut m=Meter { name: ("cpu").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 10001 }; m.price=Price::Tiers(vec![]); m.days=0; assert_eq!(invoice(&[m],0,0), Err(Error::Meter { index: 0, source: Box::new(Error::Discount) }));
}

// R38
#[test]
fn invoice_bill_before_month() {
let mut m=Meter { name: ("cpu").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 0, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }; m.price=Price::Tiers(vec![]); assert_eq!(invoice(&[m],0,0),Err(Error::Meter { index: 0, source: Box::new(Error::Empty) }));
}

// R38, R42
#[test]
fn invoice_input_index() {
let m=Meter { name: ("a").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 27, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }; assert_eq!(invoice(&[Meter { name: ("z").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }, m], 0, 0),Err(Error::Meter { index: 1, source: Box::new(Error::Month) }));
}

// R28, R38
#[test]
fn invoice_component_credit() {
let m=Meter { name: ("cpu").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![DayRange { start: 3, end: 3 }], rounding: Rounding::Down, discount_bps: 0 }; assert_eq!(invoice(&[m],0,0),Err(Error::Meter { index: 0, source: Box::new(Error::Credit { index: 0 }) }));
}

// R39, R47
#[test]
fn invoice_combined_before_discount() {
let m=Meter { name: ("cpu").to_string(), quantity: 1, price: Price::Flat { unit: u64::MAX, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 1, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 10000 }; assert_eq!(invoice(&[m],10000,u64::MAX),Err(Error::Meter { index: 0, source: Box::new(Error::CombinedOverflow) }));
}

// R40
#[test]
fn invoice_minimum_then_monthly() {
let i=invoice(&[Meter { name: ("cpu").to_string(), quantity: 2, price: Price::Flat { unit: 3, start: 0 }, mode: Mode::Graduated, minimum: 11, monthly_fee: 9, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 2500 }], 0, 0).unwrap(); assert_eq!(i.lines,vec![InvoiceLine { name:"cpu".into(),usage:6,minimum_adjustment:5,monthly:9,discount:5,total:15 }]);
}

// R40, R43
#[test]
fn invoice_two_discount_floors() {
let i=invoice(&[Meter { name: ("cpu").to_string(), quantity: 1, price: Price::Flat { unit: 13, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 1500 }], 2500, 2).unwrap(); assert_eq!((i.subtotal,i.discount,i.credit,i.total),(12,3,2,7));
}

// R40
#[test]
fn invoice_meter_rounding_before_sum() {
let i=invoice(&[Meter { name: ("a").to_string(), quantity: 1, price: Price::Flat { unit: 3, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 5000 }, Meter { name: ("b").to_string(), quantity: 1, price: Price::Flat { unit: 3, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 5000 }], 5000, 0).unwrap(); assert_eq!((i.subtotal,i.discount,i.total),(4,2,2));
}

// R43, R44
#[test]
fn invoice_capped_credit() {
let i=invoice(&[Meter { name: ("cpu").to_string(), quantity: 1, price: Price::Flat { unit: 17, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }], 5000, u64::MAX).unwrap(); assert_eq!((i.discount,i.credit,i.total),(8,9,0));
}

// R22, R40
#[test]
fn invoice_zero_usage_monthly() {
let i=invoice(&[Meter { name: ("cpu").to_string(), quantity: 0, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 999, monthly_fee: 8, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }], 0, 0).unwrap(); assert_eq!((i.lines[0].usage,i.lines[0].minimum_adjustment,i.lines[0].monthly,i.total),(0,0,8,8));
}

// R41, R46
#[test]
fn invoice_zero_lines_preserved() {
let i=invoice(&[Meter { name: ("free").to_string(), quantity: 1, price: Price::Flat { unit: 0, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }], 0, 0).unwrap(); assert_eq!(i.lines.len(),1); assert_eq!(i.rendered,"4:free|0|0|0|0|0\nTOTAL|0|0|0|0\n");
}

// R36, R37, R42, R45, R46
#[test]
fn invoice_utf8_sort_render() {
let i=invoice(&[Meter { name: ("é").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }, Meter { name: ("a2").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }, Meter { name: ("A").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }, Meter { name: ("a10").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }, Meter { name: (" a ").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }], 0, 0).unwrap(); assert_eq!(i.lines.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(),vec![" a ","A","a10","a2","é"]); assert_eq!(i.rendered,"3: a |10|0|0|0|10\n1:A|10|0|0|0|10\n3:a10|10|0|0|0|10\n2:a2|10|0|0|0|10\n2:é|10|0|0|0|10\nTOTAL|50|0|0|50\n");
}

// R41, R47
#[test]
fn invoice_subtotal_overflow() {
assert_eq!(invoice(&[Meter { name: ("a").to_string(), quantity: 1, price: Price::Flat { unit: u64::MAX, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }, Meter { name: ("b").to_string(), quantity: 1, price: Price::Flat { unit: 1, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }], 10000, u64::MAX),Err(Error::InvoiceOverflow));
}

// R38, R41
#[test]
fn invoice_overflow_before_later_error() {
assert_eq!(invoice(&[Meter { name: ("z").to_string(), quantity: 1, price: Price::Flat { unit: u64::MAX, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }, Meter { name: ("y").to_string(), quantity: 1, price: Price::Flat { unit: 1, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }, Meter { name: ("a").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 10001 }], 0, 0),Err(Error::InvoiceOverflow));
}

// R40, R41, R47
#[test]
fn invoice_line_discounts_before_subtotal() {
let i=invoice(&[Meter { name: ("a").to_string(), quantity: 1, price: Price::Flat { unit: u64::MAX, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 10000 }, Meter { name: ("b").to_string(), quantity: 1, price: Price::Flat { unit: u64::MAX, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 10000 }], 0, 0).unwrap(); assert_eq!(i.subtotal,0); assert_eq!(i.lines[0].discount,u64::MAX);
}

// R43, R47
#[test]
fn invoice_max_discount_product() {
let i=invoice(&[Meter { name: ("cpu").to_string(), quantity: 1, price: Price::Flat { unit: u64::MAX, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }], 9999, 0).unwrap(); let d=(u64::MAX as u128*9999/10000) as u64; assert_eq!(i.discount,d); assert_eq!(i.total,u64::MAX-d);
}

// R38, R47
#[test]
fn invoice_full_discount_validates() {
let mut m=Meter { name: ("cpu").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 10000 }; m.price=Price::Tiers(vec![]); assert_eq!(invoice(&[m],10000,0),Err(Error::Meter { index: 0, source: Box::new(Error::Empty) }));
}

// R19, R31, R32, R40, R43
#[test]
fn invoice_prorated_volume_integration() {
let m=Meter { name: ("cpu").to_string(), quantity: 5, price: Price::Tiers(vec![Tier { up_to: Some(3), unit: 9, start: 0 }, Tier { up_to: None, unit: 2, start: 1 }]), mode: Mode::Volume, minimum: 15, monthly_fee: 7, days: 28, start: 0, end: 14, credits: vec![], rounding: Rounding::HalfEven, discount_bps: 1000 }; let i=invoice(&[m],2000,1).unwrap(); assert_eq!(i.lines,vec![InvoiceLine { name:"cpu".into(), usage:11, minimum_adjustment:4, monthly:4, discount:1, total:18 }]); assert_eq!((i.discount,i.credit,i.total),(3,1,14));
}

// R38
#[test]
fn invoice_earlier_meter_before_later_discount() {
assert_eq!(invoice(&[Meter { name: ("z").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 0, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 0 }, Meter { name: ("a").to_string(), quantity: 1, price: Price::Flat { unit: 10, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 10001 }], 0, 0),Err(Error::Meter { index: 0, source: Box::new(Error::Month) }));
}

// R40, R47
#[test]
fn invoice_max_meter_discount_product() {
let i=invoice(&[Meter { name: ("cpu").to_string(), quantity: 1, price: Price::Flat { unit: u64::MAX, start: 0 }, mode: Mode::Graduated, minimum: 0, monthly_fee: 0, days: 30, start: 0, end: 30, credits: vec![], rounding: Rounding::Down, discount_bps: 9999 }], 0, 0).unwrap(); let d=(u64::MAX as u128*9999/10000) as u64; assert_eq!(i.lines[0].discount,d); assert_eq!(i.total,u64::MAX-d);
}
