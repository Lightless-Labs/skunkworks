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
pub enum Error {
    Empty,
    TooMany,
    Bound { index: usize },
    Unbounded { index: usize },
    MissingFinal,
    Overflow { index: usize },
}

fn add_band(
    total: &mut u64,
    bands: &mut Vec<Band>,
    index: usize,
    units: u64,
    unit: u64,
    start: u64,
) -> Result<(), Error> {
    if units == 0 {
        return Ok(());
    }

    let amount = units
        .checked_mul(unit)
        .and_then(|product| product.checked_add(start))
        .ok_or(Error::Overflow { index })?;
    let new_total = total
        .checked_add(amount)
        .ok_or(Error::Overflow { index })?;

    *total = new_total;
    bands.push(Band {
        index,
        units,
        amount,
    });
    Ok(())
}

pub fn quote(quantity: u64, price: &Price) -> Result<Charge, Error> {
    match price {
        Price::Flat { unit, start } => {
            let mut amount = 0;
            let mut bands = Vec::new();
            add_band(&mut amount, &mut bands, 0, quantity, *unit, *start)?;
            Ok(Charge { amount, bands })
        }
        Price::Tiers(tiers) => {
            if tiers.is_empty() {
                return Err(Error::Empty);
            }
            if tiers.len() > 64 {
                return Err(Error::TooMany);
            }

            let mut previous_bound = 0u64;
            for (index, tier) in tiers.iter().enumerate() {
                match tier.up_to {
                    Some(bound) => {
                        if bound <= previous_bound {
                            return Err(Error::Bound { index });
                        }
                        previous_bound = bound;
                    }
                    None => {
                        if index != tiers.len() - 1 {
                            return Err(Error::Unbounded { index });
                        }
                    }
                }
            }

            if tiers.last().and_then(|tier| tier.up_to).is_some() {
                return Err(Error::MissingFinal);
            }

            let mut remaining = quantity;
            let mut previous_bound = 0u64;
            let mut amount = 0u64;
            let mut bands = Vec::new();

            for (index, tier) in tiers.iter().enumerate() {
                let units = match tier.up_to {
                    Some(bound) => {
                        let width = bound - previous_bound;
                        previous_bound = bound;
                        remaining.min(width)
                    }
                    None => remaining,
                };

                add_band(
                    &mut amount,
                    &mut bands,
                    index,
                    units,
                    tier.unit,
                    tier.start,
                )?;
                remaining -= units;
            }

            Ok(Charge { amount, bands })
        }
    }
}

fn main() {}