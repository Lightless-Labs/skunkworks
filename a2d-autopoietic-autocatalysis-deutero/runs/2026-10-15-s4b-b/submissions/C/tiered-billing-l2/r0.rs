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

    let mut previous = 0;
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

fn band_amount(units: u64, unit: u64, start: u64, index: usize) -> Result<u64, Error> {
    units
        .checked_mul(unit)
        .and_then(|product| product.checked_add(start))
        .ok_or(Error::Overflow { index })
}

fn add_total(total: u64, amount: u64, index: usize) -> Result<u64, Error> {
    total.checked_add(amount).ok_or(Error::Overflow { index })
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
            let amount = band_amount(quantity, *unit, *start, 0)?;
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
            if quantity == 0 {
                return Ok(Charge {
                    amount: 0,
                    bands: Vec::new(),
                });
            }

            let mut remaining = quantity;
            let mut previous = 0u64;
            let mut total = 0u64;
            let mut bands = Vec::new();

            for (index, tier) in tiers.iter().enumerate() {
                if remaining == 0 {
                    break;
                }
                let units = match tier.up_to {
                    Some(bound) => remaining.min(bound - previous),
                    None => remaining,
                };
                if units > 0 {
                    let amount = band_amount(units, tier.unit, tier.start, index)?;
                    total = add_total(total, amount, index)?;
                    bands.push(Band {
                        index,
                        units,
                        amount,
                    });
                    remaining -= units;
                }
                if let Some(bound) = tier.up_to {
                    previous = bound;
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
                    .expect("validated schedule has a final unbounded tier");
                let amount = band_amount(quantity, tier.unit, tier.start, index)?;
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

    let minimum_adjustment = if quantity > 0 && charge.amount < minimum {
        minimum - charge.amount
    } else {
        0
    };
    let total = charge
        .amount
        .checked_add(minimum_adjustment)
        .expect("minimum adjustment makes total equal to minimum");

    Ok(Bill {
        charge,
        minimum_adjustment,
        total,
    })
}

fn main() {}