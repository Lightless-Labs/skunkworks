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
pub enum Error {
    Empty,
    TooMany,
    Bound { index: usize },
    Unbounded { index: usize },
    MissingFinal,
    Overflow { index: usize },
}

fn validate_tiers(tiers: &[Tier]) -> Result<(), Error> {
    if tiers.is_empty() {
        return Err(Error::Empty);
    }
    if tiers.len() > 64 {
        return Err(Error::TooMany);
    }

    let mut previous_bound = 0_u64;

    for (index, tier) in tiers.iter().enumerate() {
        match tier.up_to {
            Some(bound) => {
                if bound <= previous_bound {
                    return Err(Error::Bound { index });
                }
                previous_bound = bound;
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

fn band_amount(units: u64, unit: u64, start: u64, index: usize) -> Result<u64, Error> {
    units
        .checked_mul(unit)
        .and_then(|amount| amount.checked_add(start))
        .ok_or(Error::Overflow { index })
}

fn add_amount(total: u64, amount: u64, index: usize) -> Result<u64, Error> {
    total.checked_add(amount).ok_or(Error::Overflow { index })
}

fn graduated_charge(quantity: u64, tiers: &[Tier]) -> Result<Charge, Error> {
    validate_tiers(tiers)?;

    let mut remaining = quantity;
    let mut previous_bound = 0_u64;
    let mut amount = 0_u64;
    let mut bands = Vec::new();

    for (index, tier) in tiers.iter().enumerate() {
        if remaining == 0 {
            break;
        }

        let units = match tier.up_to {
            Some(bound) => {
                let width = bound - previous_bound;
                let allocated = remaining.min(width);
                previous_bound = bound;
                allocated
            }
            None => remaining,
        };

        if units > 0 {
            let band_charge = band_amount(units, tier.unit, tier.start, index)?;
            amount = add_amount(amount, band_charge, index)?;
            bands.push(Band {
                index,
                units,
                amount: band_charge,
            });
            remaining -= units;
        }
    }

    Ok(Charge { amount, bands })
}

fn volume_charge(quantity: u64, tiers: &[Tier]) -> Result<Charge, Error> {
    validate_tiers(tiers)?;

    if quantity == 0 {
        return Ok(Charge {
            amount: 0,
            bands: Vec::new(),
        });
    }

    let (index, tier) = tiers
        .iter()
        .enumerate()
        .find(|(_, tier)| tier.up_to.is_none_or(|bound| bound >= quantity))
        .expect("validated tier schedule has a final unbounded tier");

    let amount = band_amount(quantity, tier.unit, tier.start, index)?;
    Ok(Charge {
        amount,
        bands: vec![Band {
            index,
            units: quantity,
            amount,
        }],
    })
}

fn flat_charge(quantity: u64, unit: u64, start: u64) -> Result<Charge, Error> {
    if quantity == 0 {
        return Ok(Charge {
            amount: 0,
            bands: Vec::new(),
        });
    }

    let amount = band_amount(quantity, unit, start, 0)?;
    Ok(Charge {
        amount,
        bands: vec![Band {
            index: 0,
            units: quantity,
            amount,
        }],
    })
}

pub fn quote(quantity: u64, price: &Price) -> Result<Charge, Error> {
    match price {
        Price::Flat { unit, start } => flat_charge(quantity, *unit, *start),
        Price::Tiers(tiers) => graduated_charge(quantity, tiers),
    }
}

pub fn bill(
    quantity: u64,
    price: &Price,
    mode: Mode,
    minimum: u64,
) -> Result<Bill, Error> {
    let charge = match price {
        Price::Flat { unit, start } => flat_charge(quantity, *unit, *start)?,
        Price::Tiers(tiers) => match mode {
            Mode::Graduated => graduated_charge(quantity, tiers)?,
            Mode::Volume => volume_charge(quantity, tiers)?,
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
        .ok_or(Error::Overflow {
            index: charge.bands.last().map_or(0, |band| band.index),
        })?;

    Ok(Bill {
        charge,
        minimum_adjustment,
        total,
    })
}