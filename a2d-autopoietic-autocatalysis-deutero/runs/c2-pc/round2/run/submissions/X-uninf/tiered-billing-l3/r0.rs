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

    if tiers.last().and_then(|tier| tier.up_to).is_some() {
        return Err(Error::MissingFinal);
    }
    Ok(())
}

fn band_charge(index: usize, units: u64, unit: u64, start: u64) -> Result<Band, Error> {
    let amount = units
        .checked_mul(unit)
        .and_then(|value| value.checked_add(start))
        .ok_or(Error::Overflow { index })?;
    Ok(Band {
        index,
        units,
        amount,
    })
}

fn add_band(total: &mut u64, band: Band) -> Result<(), Error> {
    *total = total
        .checked_add(band.amount)
        .ok_or(Error::Overflow { index: band.index })?;
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
            let band = band_charge(0, quantity, *unit, *start)?;
            Ok(Charge {
                amount: band.amount,
                bands: vec![band],
            })
        }
        Price::Tiers(tiers) => {
            validate_tiers(tiers)?;

            let mut remaining = quantity;
            let mut previous = 0u64;
            let mut total = 0u64;
            let mut bands = Vec::new();

            for (index, tier) in tiers.iter().enumerate() {
                if remaining == 0 {
                    break;
                }

                let units = match tier.up_to {
                    Some(bound) => {
                        let width = bound - previous;
                        previous = bound;
                        remaining.min(width)
                    }
                    None => remaining,
                };

                if units > 0 {
                    let band = band_charge(index, units, tier.unit, tier.start)?;
                    add_band(&mut total, band.clone())?;
                    bands.push(band);
                    remaining -= units;
                }
            }

            Ok(Charge {
                amount: total,
                bands,
            })
        }
    }
}

pub fn bill(
    quantity: u64,
    price: &Price,
    mode: Mode,
    minimum: u64,
) -> Result<Bill, Error> {
    let charge = match price {
        Price::Flat { .. } => quote(quantity, price)?,
        Price::Tiers(tiers) => {
            validate_tiers(tiers)?;
            match mode {
                Mode::Graduated => quote(quantity, price)?,
                Mode::Volume => {
                    if quantity == 0 {
                        Charge {
                            amount: 0,
                            bands: Vec::new(),
                        }
                    } else {
                        let (index, tier) = tiers
                            .iter()
                            .enumerate()
                            .find(|(_, tier)| {
                                tier.up_to.map_or(true, |bound| quantity <= bound)
                            })
                            .expect("validated tiers always have a final unbounded tier");
                        let band =
                            band_charge(index, quantity, tier.unit, tier.start)?;
                        Charge {
                            amount: band.amount,
                            bands: vec![band],
                        }
                    }
                }
            }
        }
    };

    let minimum_adjustment = if quantity > 0 {
        minimum.saturating_sub(charge.amount)
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
    if !matches!(days, 28 | 29 | 30 | 31) {
        return Err(Error::Month);
    }
    if start > end || end > days {
        return Err(Error::Period);
    }

    let mut previous_credit_end = start;
    let mut credited_days = 0u32;

    for (index, credit) in credits.iter().enumerate() {
        if credit.start < start || credit.start >= credit.end || credit.end > end {
            return Err(Error::Credit { index });
        }
        if credit.start < previous_credit_end {
            return Err(Error::CreditOrder { index });
        }
        previous_credit_end = credit.end;
        credited_days += credit.end - credit.start;
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
            let doubled_remainder = remainder * 2;
            if doubled_remainder > denominator
                || (doubled_remainder == denominator && quotient % 2 == 1)
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