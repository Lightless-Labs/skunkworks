// Fault: unicode-length
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tier { pub up_to: Option<u64>, pub unit: u64, pub start: u64 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Price { Flat { unit: u64, start: u64 }, Tiers(Vec<Tier>) }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Band { pub index: usize, pub units: u64, pub amount: u64 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Charge { pub amount: u64, pub bands: Vec<Band> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode { Graduated, Volume }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bill { pub charge: Charge, pub minimum_adjustment: u64, pub total: u64 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayRange { pub start: u32, pub end: u32 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rounding { Down, Up, HalfEven }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proration { pub active_days: u32, pub amount: u64 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meter {
    pub name: String, pub quantity: u64, pub price: Price, pub mode: Mode,
    pub minimum: u64, pub monthly_fee: u64, pub days: u32, pub start: u32,
    pub end: u32, pub credits: Vec<DayRange>, pub rounding: Rounding,
    pub discount_bps: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvoiceLine {
    pub name: String, pub usage: u64, pub minimum_adjustment: u64,
    pub monthly: u64, pub discount: u64, pub total: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invoice {
    pub lines: Vec<InvoiceLine>, pub subtotal: u64, pub discount: u64,
    pub credit: u64, pub total: u64, pub rendered: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error { Empty, TooMany, Bound { index: usize }, Unbounded { index: usize }, MissingFinal, Overflow { index: usize }, Month, Period, Credit { index: usize }, CreditOrder { index: usize }, InvoiceDiscount, InvalidName { index: usize }, DuplicateName { index: usize }, Discount, CombinedOverflow, Meter { index: usize, source: Box<Error> }, InvoiceOverflow }
fn validate(price: &Price) -> Result<(), Error> {
    if let Price::Tiers(ts) = price {
        if ts.is_empty() { return Err(Error::Empty); }
        if ts.len() > 64 { return Err(Error::TooMany); }
        let mut previous = 0;
        for (index, t) in ts.iter().enumerate() {
            match t.up_to {
                Some(n) => {
                    if n <= previous { return Err(Error::Bound { index }); }
                    previous = n;
                }
                None => if index + 1 != ts.len() { return Err(Error::Unbounded { index }); },
            }
        }
        if ts.last().unwrap().up_to.is_some() { return Err(Error::MissingFinal); }
    }
    Ok(())
}
fn add_band(c: &mut Charge, index: usize, units: u64, unit: u64, start: u64) -> Result<(), Error> {
    if units == 0 { return Ok(()); }
    let amount = units as u128 * unit as u128 + start as u128;
    if amount > u64::MAX as u128 || c.amount as u128 + amount > u64::MAX as u128 {
        return Err(Error::Overflow { index });
    }
    c.amount += amount as u64;
    c.bands.push(Band { index, units, amount: amount as u64 });
    Ok(())
}
pub fn quote(quantity: u64, price: &Price) -> Result<Charge, Error> {
    validate(price)?;
    let mut c = Charge { amount: 0, bands: Vec::new() };
    match price {
        Price::Flat { unit, start } => add_band(&mut c, 0, quantity, *unit, *start)?,
        Price::Tiers(ts) => {
            let mut previous = 0;
            for (index, t) in ts.iter().enumerate() {
                let end = quantity.min(t.up_to.unwrap_or(quantity));
                let units = end.saturating_sub(previous);
                add_band(&mut c, index, units, t.unit, t.start)?;
                if end == quantity { break; }
                previous = end;
            }
        }
    }
    Ok(c)
}
pub fn bill(quantity: u64, price: &Price, mode: Mode, minimum: u64) -> Result<Bill, Error> {
    let charge = match mode {
        Mode::Graduated => quote(quantity, price)?,
        Mode::Volume => {
            validate(price)?;
            let mut c = Charge { amount: 0, bands: Vec::new() };
            if quantity > 0 {
                match price {
                    Price::Flat { unit, start } => add_band(&mut c, 0, quantity, *unit, *start)?,
                    Price::Tiers(ts) => {
                        let (index, t) = ts.iter().enumerate().find(|(_, t)| t.up_to.map_or(true, |n| quantity <= n)).unwrap();
                        add_band(&mut c, index, quantity, t.unit, t.start)?;
                    }
                }
            }
            c
        }
    };
    let minimum_adjustment = if quantity == 0 { 0 } else { minimum.saturating_sub(charge.amount) };
    let total = charge.amount + minimum_adjustment;
    Ok(Bill { charge, minimum_adjustment, total })
}
pub fn prorate(fee: u64, days: u32, start: u32, end: u32, credits: &[DayRange], rounding: Rounding) -> Result<Proration, Error> {
    if !(28..=31).contains(&days) { return Err(Error::Month); }
    if start > end || end > days { return Err(Error::Period); }
    let mut active_days = end - start;
    let mut previous_end = start;
    for (index, r) in credits.iter().enumerate() {
        if r.start < start || r.start >= r.end || r.end > end { return Err(Error::Credit { index }); }
        if r.start < previous_end { return Err(Error::CreditOrder { index }); }
        previous_end = r.end;
        active_days -= r.end - r.start;
    }
    let n = fee as u128 * active_days as u128;
    let d = days as u128;
    let q = n / d;
    let r = n % d;
    let increment = match rounding {
        Rounding::Down => false,
        Rounding::Up => r != 0,
        Rounding::HalfEven => r * 2 > d || (r * 2 == d && q % 2 == 1),
    };
    Ok(Proration { active_days, amount: (q + u128::from(increment)) as u64 })
}
pub fn invoice(meters: &[Meter], invoice_discount_bps: u32, credit: u64) -> Result<Invoice, Error> {
    if invoice_discount_bps > 10000 { return Err(Error::InvoiceDiscount); }
    let mut names = std::collections::HashSet::new();
    for (index, m) in meters.iter().enumerate() {
        if m.name.is_empty() || m.name.len() > 32 || m.name.chars().any(|c| c.is_ascii_control() || c == '|' || c == ':') {
            return Err(Error::InvalidName { index });
        }
        if !names.insert(m.name.as_str()) { return Err(Error::DuplicateName { index }); }
    }
    let mut lines = Vec::new();
    let mut subtotal = 0u64;
    for (index, m) in meters.iter().enumerate() {
        let line = (|| -> Result<InvoiceLine, Error> {
            if m.discount_bps > 10000 { return Err(Error::Discount); }
            let b = bill(m.quantity, &m.price, m.mode.clone(), m.minimum)?;
            let p = prorate(m.monthly_fee, m.days, m.start, m.end, &m.credits, m.rounding.clone())?;
            let base = b.total.checked_add(p.amount).ok_or(Error::CombinedOverflow)?;
            let discount = (base as u128 * m.discount_bps as u128 / 10000) as u64;
            Ok(InvoiceLine { name: m.name.clone(), usage: b.charge.amount,
                minimum_adjustment: b.minimum_adjustment, monthly: p.amount,
                discount, total: base - discount })
        })().map_err(|source| Error::Meter { index, source: Box::new(source) })?;
        subtotal = subtotal.checked_add(line.total).ok_or(Error::InvoiceOverflow)?;
        lines.push(line);
    }
    lines.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
    let discount = (subtotal as u128 * invoice_discount_bps as u128 / 10000) as u64;
    let after = subtotal - discount;
    let credit = credit.min(after);
    let total = after - credit;
    let mut rendered = String::new();
    for l in &lines {
        rendered.push_str(&format!("{}:{}|{}|{}|{}|{}|{}\n", l.name.chars().count(), l.name, l.usage, l.minimum_adjustment, l.monthly, l.discount, l.total));
    }
    rendered.push_str(&format!("TOTAL|{}|{}|{}|{}\n", subtotal, discount, credit, total));
    Ok(Invoice { lines, subtotal, discount, credit, total, rendered })
}
