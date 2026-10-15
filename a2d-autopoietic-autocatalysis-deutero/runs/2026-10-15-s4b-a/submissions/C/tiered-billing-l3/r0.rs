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

fn validate_tiers(tiers: &[Tier]) -> Result<(), Error> {
    if tiers.is_empty() {
        return Err(Error::Empty);
    }
    if tiers.len() > 64 {
        return Err(Error::TooMany);
    }

    let mut previous = 0u64;
    for (index, tier) in tiers.iter().enumerate() {
        match tier.up_to {
            Some(bound) => {
                if bound <= previous {
                    return Err(Error::Bound { index });
                }
                previous = bound;
            }
            None => {
                if index + 1 != tiers.len() {
                    return Err(Error::Unbounded { index });
                }
            }
        }
    }

    if tiers.last().unwrap().up_to.is_some() {
        return Err(Error::MissingFinal);
    }
    Ok(())
}

fn band_charge(units: u64, unit: u64, start: u64, index: usize) -> Result<u64, Error> {
    units
        .checked_mul(unit)
        .and_then(|product| product.checked_add(start))
        .ok_or(Error::Overflow { index })
}

fn add_band(
    amount: &mut u64,
    bands: &mut Vec<Band>,
    index: usize,
    units: u64,
    unit: u64,
    start: u64,
) -> Result<(), Error> {
    if units == 0 {
        return Ok(());
    }
    let charge = band_charge(units, unit, start, index)?;
    let total = amount
        .checked_add(charge)
        .ok_or(Error::Overflow { index })?;
    *amount = total;
    bands.push(Band {
        index,
        units,
        amount: charge,
    });
    Ok(())
}

pub fn quote(quantity: u64, price: &Price) -> Result<Charge, Error> {
    match price {
        Price::Flat { unit, start } => {
            if quantity == 0 {
                return Ok(Charge {
                    amount: 0,
                    bands: Vec::new(),
                });
            }
            let amount = band_charge(quantity, *unit, *start, 0)?;
            Ok(Charge {
                amount,
                bands: vec![Band {
                    index: 0,
                    units: quantity,
                    amount,
                }],
            })
        }
        Price::Tiers(tiers) => {
            validate_tiers(tiers)?;
            let mut amount = 0u64;
            let mut bands = Vec::new();
            let mut remaining = quantity;
            let mut previous = 0u64;

            for (index, tier) in tiers.iter().enumerate() {
                if remaining == 0 {
                    break;
                }
                let units = match tier.up_to {
                    Some(bound) => {
                        let width = bound - previous;
                        let allocated = remaining.min(width);
                        previous = bound;
                        allocated
                    }
                    None => remaining,
                };
                add_band(&mut amount, &mut bands, index, units, tier.unit, tier.start)?;
                remaining -= units;
            }

            Ok(Charge { amount, bands })
        }
    }
}

pub fn bill(
    quantity: u64,
    price: &Price,
    mode: Mode,
    minimum: u64,
) -> Result<Bill, Error> {
    let charge = match (price, mode) {
        (_, Mode::Graduated) => quote(quantity, price)?,
        (Price::Flat { .. }, Mode::Volume) => quote(quantity, price)?,
        (Price::Tiers(tiers), Mode::Volume) => {
            validate_tiers(tiers)?;
            if quantity == 0 {
                Charge {
                    amount: 0,
                    bands: Vec::new(),
                }
            } else {
                let (index, tier) = tiers
                    .iter()
                    .enumerate()
                    .find(|(_, tier)| tier.up_to.map_or(true, |bound| bound >= quantity))
                    .unwrap();
                let amount = band_charge(quantity, tier.unit, tier.start, index)?;
                Charge {
                    amount,
                    bands: vec![Band {
                        index,
                        units: quantity,
                        amount,
                    }],
                }
            }
        }
    };

    let minimum_adjustment = if quantity > 0 && minimum > charge.amount {
        minimum - charge.amount
    } else {
        0
    };
    let total = charge
        .amount
        .checked_add(minimum_adjustment)
        .ok_or(Error::Overflow {
            index: charge.bands.last().map_or(0, |band| band.index),
        })?;

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
    if days != 28 && days != 29 && days != 30 && days != 31 {
        return Err(Error::Month);
    }
    if start > end || end > days {
        return Err(Error::Period);
    }

    let mut previous_end = start;
    let mut credited_days = 0u32;
    for (index, credit) in credits.iter().enumerate() {
        if credit.start < start || credit.start >= credit.end || credit.end > end {
            return Err(Error::Credit { index });
        }
        if credit.start < previous_end {
            return Err(Error::CreditOrder { index });
        }
        credited_days += credit.end - credit.start;
        previous_end = credit.end;
    }

    let active_days = end - start - credited_days;
    let numerator = (fee as u128) * (active_days as u128);
    let denominator = days as u128;
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;

    let rounded = match rounding {
        Rounding::Down => quotient,
        Rounding::Up => quotient + u128::from(remainder != 0),
        Rounding::HalfEven => {
            if remainder * 2 > denominator
                || (remainder * 2 == denominator && quotient % 2 == 1)
            {
                quotient + 1
            } else {
                quotient
            }
        }
    };

    Ok(Proration {
        active_days,
        amount: rounded as u64,
    })
}

fn main() {}