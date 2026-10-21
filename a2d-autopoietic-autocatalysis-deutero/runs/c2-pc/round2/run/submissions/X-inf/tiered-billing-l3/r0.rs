#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tier {
    pub up_to: Option<u64>,
    pub unit: u64,
    pub start: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Price {
    Flat { unit: u64, start: u64 },
    Tiers(Vec<Tier>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Band {
    pub index: usize,
    pub units: u64,
    pub amount: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Charge {
    pub amount: u64,
    pub bands: Vec<Band>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Graduated,
    Volume,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bill {
    pub charge: Charge,
    pub minimum_adjustment: u64,
    pub total: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayRange {
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rounding {
    Down,
    Up,
    HalfEven,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proration {
    pub active_days: u32,
    pub amount: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    TooMany,
    Bound { index: usize },
    Unbounded { index: usize },
    MissingFinal,
    Overflow { index: usize },
    Month,
    Period,
    Credit { index: usize },
    CreditOrder { index: usize },
}

fn validate_schedule(tiers: &[Tier]) -> Result<(), Error> {
    if tiers.is_empty() {
        return Err(Error::Empty);
    }
    if tiers.len() > 64 {
        return Err(Error::TooMany);
    }

    let mut last_finite = 0u64;
    for (i, tier) in tiers.iter().enumerate() {
        if let Some(bound) = tier.up_to {
            if bound <= last_finite {
                return Err(Error::Bound { index: i });
            }
            last_finite = bound;
        } else if i != tiers.len() - 1 {
            return Err(Error::Unbounded { index: i });
        }
    }

    if tiers[tiers.len() - 1].up_to.is_some() {
        Err(Error::MissingFinal)
    } else {
        Ok(())
    }
}

fn charge_one(index: usize, units: u64, unit: u64, start: u64) -> Result<u64, Error> {
    units
        .checked_mul(unit)
        .and_then(|v| v.checked_add(start))
        .ok_or(Error::Overflow { index })
}

fn make_flat(quantity: u64, unit: u64, start: u64) -> Result<Charge, Error> {
    if quantity == 0 {
        return Ok(Charge { amount: 0, bands: vec![] });
    }
    let amount = charge_one(0, quantity, unit, start)?;
    Ok(Charge {
        amount,
        bands: vec![Band {
            index: 0,
            units: quantity,
            amount,
        }],
    })
}

fn make_graduated(quantity: u64, tiers: &[Tier]) -> Result<Charge, Error> {
    validate_schedule(tiers)?;
    let mut remaining = quantity;
    let mut lower = 0u64;
    let mut total = 0u64;
    let mut bands = Vec::new();

    for (i, tier) in tiers.iter().enumerate() {
        if remaining == 0 {
            break;
        }

        let units = if let Some(bound) = tier.up_to {
            let width = bound - lower;
            lower = bound;
            remaining.min(width)
        } else {
            remaining
        };

        if units != 0 {
            let amount = charge_one(i, units, tier.unit, tier.start)?;
            total = total.checked_add(amount).ok_or(Error::Overflow { index: i })?;
            bands.push(Band { index: i, units, amount });
            remaining -= units;
        }
    }

    Ok(Charge { amount: total, bands })
}

fn make_volume(quantity: u64, tiers: &[Tier]) -> Result<Charge, Error> {
    validate_schedule(tiers)?;
    if quantity == 0 {
        return Ok(Charge { amount: 0, bands: vec![] });
    }

    let index = tiers
        .iter()
        .position(|tier| tier.up_to.map_or(true, |bound| bound >= quantity))
        .unwrap();
    let tier = &tiers[index];
    let amount = charge_one(index, quantity, tier.unit, tier.start)?;
    Ok(Charge {
        amount,
        bands: vec![Band {
            index,
            units: quantity,
            amount,
        }],
    })
}

pub fn quote(quantity: u64, price: &Price) -> Result<Charge, Error> {
    match price {
        Price::Flat { unit, start } => make_flat(quantity, *unit, *start),
        Price::Tiers(tiers) => make_graduated(quantity, tiers),
    }
}

pub fn bill(quantity: u64, price: &Price, mode: Mode, minimum: u64) -> Result<Bill, Error> {
    let charge = match price {
        Price::Flat { unit, start } => make_flat(quantity, *unit, *start)?,
        Price::Tiers(tiers) => match mode {
            Mode::Graduated => make_graduated(quantity, tiers)?,
            Mode::Volume => make_volume(quantity, tiers)?,
        },
    };

    let minimum_adjustment = if quantity > 0 {
        minimum.saturating_sub(charge.amount)
    } else {
        0
    };
    let total = charge
        .amount
        .checked_add(minimum_adjustment)
        .ok_or(Error::Overflow { index: 0 })?;

    Ok(Bill {
        charge,
        minimum_adjustment,
        total,
    })
}

pub fn prorate(
    fee: u64,
    days: u32,
    start: u32,
    end: u32,
    credits: &[DayRange],
    rounding: Rounding,
) -> Result<Proration, Error> {
    if !matches!(days, 28 | 29 | 30 | 31) {
        return Err(Error::Month);
    }
    if start > end || end > days {
        return Err(Error::Period);
    }

    let mut previous_end = start;
    let mut credited = 0u32;
    for (i, credit) in credits.iter().enumerate() {
        if credit.start < start || credit.start >= credit.end || credit.end > end {
            return Err(Error::Credit { index: i });
        }
        if credit.start < previous_end {
            return Err(Error::CreditOrder { index: i });
        }
        credited += credit.end - credit.start;
        previous_end = credit.end;
    }

    let active_days = end - start - credited;
    let numerator = u128::from(fee) * u128::from(active_days);
    let denominator = u128::from(days);
    let whole = numerator / denominator;
    let rem = numerator % denominator;

    let result = match rounding {
        Rounding::Down => whole,
        Rounding::Up if rem != 0 => whole + 1,
        Rounding::Up => whole,
        Rounding::HalfEven => {
            let twice = rem * 2;
            if twice < denominator {
                whole
            } else if twice > denominator || whole & 1 == 1 {
                whole + 1
            } else {
                whole
            }
        }
    };

    Ok(Proration {
        active_days,
        amount: result as u64,
    })
}