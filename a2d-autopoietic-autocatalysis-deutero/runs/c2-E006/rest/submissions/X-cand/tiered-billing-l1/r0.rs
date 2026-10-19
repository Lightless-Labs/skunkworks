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

fn checked_band_amount(units: u64, unit: u64, start: u64) -> Option<u64> {
    units.checked_mul(unit)?.checked_add(start)
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

            let amount =
                checked_band_amount(quantity, *unit, *start).ok_or(Error::Overflow { index: 0 })?;
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
                        if index + 1 != tiers.len() {
                            return Err(Error::Unbounded { index });
                        }
                    }
                }
            }

            if tiers.last().and_then(|tier| tier.up_to).is_some() {
                return Err(Error::MissingFinal);
            }

            let mut total = 0u64;
            let mut bands = Vec::new();
            let mut remaining = quantity;
            let mut previous_bound = 0u64;

            for (index, tier) in tiers.iter().enumerate() {
                if remaining == 0 {
                    break;
                }

                let units = match tier.up_to {
                    Some(bound) => {
                        let width = bound - previous_bound;
                        previous_bound = bound;
                        remaining.min(width)
                    }
                    None => remaining,
                };

                if units == 0 {
                    continue;
                }

                let amount = checked_band_amount(units, tier.unit, tier.start)
                    .ok_or(Error::Overflow { index })?;
                total = total
                    .checked_add(amount)
                    .ok_or(Error::Overflow { index })?;

                bands.push(Band {
                    index,
                    units,
                    amount,
                });
                remaining -= units;
            }

            Ok(Charge {
                amount: total,
                bands,
            })
        }
    }
}