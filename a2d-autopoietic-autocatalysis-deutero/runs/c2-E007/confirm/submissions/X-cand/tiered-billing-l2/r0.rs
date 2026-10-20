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

    let last = tiers.len() - 1;
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
                if index != last {
                    return Err(Error::Unbounded { index });
                }
            }
        }
    }

    if tiers[last].up_to.is_some() {
        return Err(Error::MissingFinal);
    }

    Ok(())
}

fn band_charge(index: usize, units: u64, unit: u64, start: u64) -> Result<Band, Error> {
    let product = units.checked_mul(unit).ok_or(Error::Overflow { index })?;
    let amount = product.checked_add(start).ok_or(Error::Overflow { index })?;
    Ok(Band { index, units, amount })
}

fn add_to_total(total: u64, band: &Band) -> Result<u64, Error> {
    total
        .checked_add(band.amount)
        .ok_or(Error::Overflow { index: band.index })
}

fn graduated_charge(quantity: u64, tiers: &[Tier]) -> Result<Charge, Error> {
    validate_tiers(tiers)?;

    let mut remaining = quantity;
    let mut previous_bound = 0;
    let mut total = 0;
    let mut bands = Vec::new();

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

        if units > 0 {
            let band = band_charge(index, units, tier.unit, tier.start)?;
            total = add_to_total(total, &band)?;
            bands.push(band);
            remaining -= units;
        }
    }

    Ok(Charge {
        amount: total,
        bands,
    })
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
        .find(|(_, tier)| tier.up_to.map_or(true, |bound| quantity <= bound))
        .expect("validated schedule has a final unbounded tier");

    let band = band_charge(index, quantity, tier.unit, tier.start)?;
    Ok(Charge {
        amount: band.amount,
        bands: vec![band],
    })
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
        Price::Flat { .. } => quote(quantity, price)?,
        Price::Tiers(tiers) => match mode {
            Mode::Graduated => graduated_charge(quantity, tiers)?,
            Mode::Volume => volume_charge(quantity, tiers)?,
        },
    };

    let minimum_adjustment = if quantity == 0 {
        0
    } else {
        minimum.saturating_sub(charge.amount)
    };
    let total = charge
        .amount
        .checked_add(minimum_adjustment)
        .ok_or_else(|| {
            let index = charge.bands.last().map_or(0, |band| band.index);
            Error::Overflow { index }
        })?;

    Ok(Bill {
        charge,
        minimum_adjustment,
        total,
    })
}