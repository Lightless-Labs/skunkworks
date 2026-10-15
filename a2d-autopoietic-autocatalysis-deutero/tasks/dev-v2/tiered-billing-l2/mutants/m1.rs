// Fault: volume-boundary
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
pub enum Error { Empty, TooMany, Bound { index: usize }, Unbounded { index: usize }, MissingFinal, Overflow { index: usize } }
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
                        let (index, t) = ts.iter().enumerate().find(|(_, t)| t.up_to.map_or(true, |n| quantity < n)).unwrap();
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
