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
pub struct Meter {
    pub name: String,
    pub quantity: u64,
    pub price: Price,
    pub mode: Mode,
    pub minimum: u64,
    pub monthly_fee: u64,
    pub days: u32,
    pub start: u32,
    pub end: u32,
    pub credits: Vec<DayRange>,
    pub rounding: Rounding,
    pub discount_bps: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvoiceLine {
    pub name: String,
    pub usage: u64,
    pub minimum_adjustment: u64,
    pub monthly: u64,
    pub discount: u64,
    pub total: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invoice {
    pub lines: Vec<InvoiceLine>,
    pub subtotal: u64,
    pub discount: u64,
    pub credit: u64,
    pub total: u64,
    pub rendered: String,
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
    InvoiceDiscount,
    InvalidName { index: usize },
    DuplicateName { index: usize },
    Discount,
    CombinedOverflow,
    Meter { index: usize, source: Box<Error> },
    InvoiceOverflow,
}

fn checked_charge(units: u64, unit: u64, start: u64, index: usize) -> Result<u64, Error> {
    units
        .checked_mul(unit)
        .and_then(|amount| amount.checked_add(start))
        .ok_or(Error::Overflow { index })
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
                if index != tiers.len() - 1 {
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

fn graduated(quantity: u64, price: &Price) -> Result<Charge, Error> {
    match price {
        Price::Flat { unit, start } => {
            if quantity == 0 {
                return Ok(Charge {
                    amount: 0,
                    bands: Vec::new(),
                });
            }
            let amount = checked_charge(quantity, *unit, *start, 0)?;
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

                let allocated = match tier.up_to {
                    Some(bound) => {
                        let width = bound - previous;
                        let amount = remaining.min(width);
                        previous = bound;
                        amount
                    }
                    None => remaining,
                };

                if allocated > 0 {
                    let amount = checked_charge(allocated, tier.unit, tier.start, index)?;
                    total = total.checked_add(amount).ok_or(Error::Overflow { index })?;
                    bands.push(Band {
                        index,
                        units: allocated,
                        amount,
                    });
                    remaining -= allocated;
                }
            }

            Ok(Charge {
                amount: total,
                bands,
            })
        }
    }
}

pub fn quote(quantity: u64, price: &Price) -> Result<Charge, Error> {
    graduated(quantity, price)
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
                let selected_index = tiers
                    .iter()
                    .position(|tier| tier.up_to.map_or(true, |bound| bound >= quantity))
                    .expect("validated schedule has an unbounded final tier");
                let tier = &tiers[selected_index];
                let amount = checked_charge(quantity, tier.unit, tier.start, selected_index)?;
                Charge {
                    amount,
                    bands: vec![Band {
                        index: selected_index,
                        units: quantity,
                        amount,
                    }],
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
        .expect("minimum adjustment cannot make total exceed u64::MAX");

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
    if !(28..=31).contains(&days) {
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

    let active_days = (end - start) - credited_days;
    if active_days == 0 || fee == 0 {
        return Ok(Proration {
            active_days,
            amount: 0,
        });
    }

    let numerator = u128::from(fee) * u128::from(active_days);
    let denominator = u128::from(days);
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;

    let rounded = match rounding {
        Rounding::Down => quotient,
        Rounding::Up => {
            if remainder == 0 {
                quotient
            } else {
                quotient + 1
            }
        }
        Rounding::HalfEven => {
            let twice_remainder = remainder * 2;
            if twice_remainder < denominator {
                quotient
            } else if twice_remainder > denominator || quotient % 2 == 1 {
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

fn discount_amount(base: u64, bps: u32) -> u64 {
    ((u128::from(base) * u128::from(bps)) / 10_000) as u64
}

pub fn invoice(
    meters: &[Meter],
    invoice_discount_bps: u32,
    credit: u64,
) -> Result<Invoice, Error> {
    if invoice_discount_bps > 10_000 {
        return Err(Error::InvoiceDiscount);
    }

    for (index, meter) in meters.iter().enumerate() {
        let name = meter.name.as_bytes();
        if name.is_empty()
            || name.len() > 32
            || name
                .iter()
                .any(|byte| *byte <= 0x1f || *byte == 0x7f || *byte == b'|' || *byte == b':')
        {
            return Err(Error::InvalidName { index });
        }
        if meters[..index]
            .iter()
            .any(|earlier| earlier.name == meter.name)
        {
            return Err(Error::DuplicateName { index });
        }
    }

    let mut lines = Vec::with_capacity(meters.len());
    let mut subtotal = 0u64;

    for (index, meter) in meters.iter().enumerate() {
        if meter.discount_bps > 10_000 {
            return Err(Error::Meter {
                index,
                source: Box::new(Error::Discount),
            });
        }

        let bill = bill(
            meter.quantity,
            &meter.price,
            meter.mode.clone(),
            meter.minimum,
        )
        .map_err(|source| Error::Meter {
            index,
            source: Box::new(source),
        })?;

        let proration = prorate(
            meter.monthly_fee,
            meter.days,
            meter.start,
            meter.end,
            &meter.credits,
            meter.rounding.clone(),
        )
        .map_err(|source| Error::Meter {
            index,
            source: Box::new(source),
        })?;

        let base = bill
            .total
            .checked_add(proration.amount)
            .ok_or_else(|| Error::Meter {
                index,
                source: Box::new(Error::CombinedOverflow),
            })?;
        let discount = discount_amount(base, meter.discount_bps);
        let line_total = base - discount;

        subtotal = subtotal
            .checked_add(line_total)
            .ok_or(Error::InvoiceOverflow)?;

        lines.push(InvoiceLine {
            name: meter.name.clone(),
            usage: bill.charge.amount,
            minimum_adjustment: bill.minimum_adjustment,
            monthly: proration.amount,
            discount,
            total: line_total,
        });
    }

    lines.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));

    let invoice_discount = discount_amount(subtotal, invoice_discount_bps);
    let applied_credit = credit.min(subtotal - invoice_discount);
    let total = subtotal - invoice_discount - applied_credit;

    let mut rendered = String::new();
    for line in &lines {
        rendered.push_str(&format!(
            "{}:{}|{}|{}|{}|{}|{}\n",
            line.name.len(),
            line.name,
            line.usage,
            line.minimum_adjustment,
            line.monthly,
            line.discount,
            line.total
        ));
    }
    rendered.push_str(&format!(
        "TOTAL|{}|{}|{}|{}\n",
        subtotal, invoice_discount, applied_credit, total
    ));

    Ok(Invoice {
        lines,
        subtotal,
        discount: invoice_discount,
        credit: applied_credit,
        total,
        rendered,
    })
}