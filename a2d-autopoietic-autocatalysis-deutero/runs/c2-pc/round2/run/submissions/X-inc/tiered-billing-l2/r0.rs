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

    let last_index = tiers.len() - 1;
    let mut previous_bound = 0;

    for (index, tier) in tiers.iter().enumerate() {
        match tier.up_to {
            Some(bound) => {
                if bound <= previous_bound {
                    return Err(Error::Bound { index });
                }
                previous_bound = bound;
            }
            None => {
                if index != last_index {
                    return Err(Error::Unbounded { index });
                }
            }
        }
    }

    if tiers[last_index].up_to.is_some() {
        return Err(Error::MissingFinal);
    }

    Ok(())
}

fn checked_band_amount(units: u64, unit: u64, start: u64, index: usize) -> Result<u64, Error> {
    units
        .checked_mul(unit)
        .and_then(|product| product.checked_add(start))
        .ok_or(Error::Overflow { index })
}

fn graduated_charge(quantity: u64, tiers: &[Tier]) -> Result<Charge, Error> {
    let mut remaining = quantity;
    let mut previous_bound = 0;
    let mut amount = 0u64;
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
            let band_amount = checked_band_amount(units, tier.unit, tier.start, index)?;
            amount = amount
                .checked_add(band_amount)
                .ok_or(Error::Overflow { index })?;
            bands.push(Band {
                index,
                units,
                amount: band_amount,
            });
            remaining -= units;
        }
    }

    Ok(Charge { amount, bands })
}

fn volume_charge(quantity: u64, tiers: &[Tier]) -> Result<Charge, Error> {
    if quantity == 0 {
        return Ok(Charge {
            amount: 0,
            bands: Vec::new(),
        });
    }

    let (index, tier) = tiers
        .iter()
        .enumerate()
        .find(|(_, tier)| tier.up_to.is_none_or(|bound| quantity <= bound))
        .expect("validated tiers have a final unbounded tier");

    let amount = checked_band_amount(quantity, tier.unit, tier.start, index)?;
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

    let amount = checked_band_amount(quantity, unit, start, 0)?;
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
        Price::Tiers(tiers) => {
            validate_tiers(tiers)?;
            graduated_charge(quantity, tiers)
        }
    }
}

pub fn bill(quantity: u64, price: &Price, mode: Mode, minimum: u64) -> Result<Bill, Error> {
    let charge = match price {
        Price::Flat { unit, start } => flat_charge(quantity, *unit, *start)?,
        Price::Tiers(tiers) => {
            validate_tiers(tiers)?;
            match mode {
                Mode::Graduated => graduated_charge(quantity, tiers)?,
                Mode::Volume => volume_charge(quantity, tiers)?,
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
        .expect("minimum adjustment yields a total no greater than minimum or charge");

    Ok(Bill {
        charge,
        minimum_adjustment,
        total,
    })
}