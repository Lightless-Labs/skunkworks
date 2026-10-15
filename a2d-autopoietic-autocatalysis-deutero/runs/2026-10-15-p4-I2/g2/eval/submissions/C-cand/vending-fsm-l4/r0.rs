#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Product {
    pub id: String,
    pub price: u16,
    pub stock: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub credit: u16,
    pub selected: Option<String>,
    pub escrow: Vec<u16>,
    pub tubes: [u16; 4],
    pub maintenance: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub id: String,
    pub price: u16,
    pub change: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    EmptyCatalog,
    BadId { id: String },
    DuplicateId { id: String },
    BadPrice { id: String },
    BadCoin { coin: u16 },
    CreditLimit,
    UnknownProduct { id: String },
    SameSelection,
    NoSelection,
    Underpaid { price: u16, credit: u16 },
    TubeLimit { coin: u16 },
    NoChange { amount: u16 },
    StockLimit { id: String },
    SoldOut { id: String },
    Busy,
    ZeroRestock,
    MaintenanceActive,
    MaintenanceRequired,
    AlreadyMode,
    ReconcileMismatch {
        coin: u16,
        expected: u16,
        found: u16,
    },
    ValueDrift { before: u64, after: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    Inserted { coin: u16, credit: u16 },
    Selected { id: String },
    Vended {
        id: String,
        price: u16,
        change: Vec<u16>,
    },
    Refunded { coins: Vec<u16> },
    PriceChanged { id: String, old: u16, new: u16 },
    Restocked { id: String, added: u16, stock: u16 },
    Maintenance { enabled: bool },
    Reconciled { before: [u16; 4], after: [u16; 4] },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub seq: usize,
    pub kind: EventKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Audit {
    pub inserted: u64,
    pub refunded: u64,
    pub sales: u64,
    pub change: u64,
    pub adjustment: i64,
    pub cash: u64,
}

pub struct Machine {
    products: Vec<Product>,
    tubes: [u16; 4],
    credit: u16,
    selected: Option<String>,
    escrow: Vec<u16>,
    maintenance: bool,
    events: Vec<Event>,
    inserted: u64,
    refunded: u64,
    sales: u64,
    change: u64,
    adjustment: i64,
}

const DENOMS: [u16; 4] = [1, 3, 4, 10];

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut seen: Vec<&str> = Vec::new();
        for product in &products {
            if product.id.is_empty()
                || product.id.len() > 8
                || !product.id.bytes().all(|b| b.is_ascii_lowercase())
            {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }
            if seen.iter().any(|id| *id == product.id) {
                return Err(Error::DuplicateId {
                    id: product.id.clone(),
                });
            }
            seen.push(&product.id);

            if !(1..=100).contains(&product.price) {
                return Err(Error::BadPrice {
                    id: product.id.clone(),
                });
            }
            if product.stock > 100 {
                return Err(Error::StockLimit {
                    id: product.id.clone(),
                });
            }
        }

        for i in 0..4 {
            if tubes[i] > 100 {
                return Err(Error::TubeLimit { coin: DENOMS[i] });
            }
        }

        Ok(Self {
            products,
            tubes,
            credit: 0,
            selected: None,
            escrow: Vec::new(),
            maintenance: false,
            events: Vec::new(),
            inserted: 0,
            refunded: 0,
            sales: 0,
            change: 0,
            adjustment: 0,
        })
    }

    pub fn state(&self) -> State {
        State {
            credit: self.credit,
            selected: self.selected.clone(),
            escrow: self.escrow.clone(),
            tubes: self.tubes,
            maintenance: self.maintenance,
        }
    }

    pub fn insert(&mut self, coin: u16) -> Result<(), Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }
        if !DENOMS.contains(&coin) {
            return Err(Error::BadCoin { coin });
        }
        if u32::from(self.credit) + u32::from(coin) > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit += coin;
        self.escrow.push(coin);
        self.inserted = self.inserted.saturating_add(u64::from(coin));
        self.log(EventKind::Inserted {
            coin,
            credit: self.credit,
        });
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }
        if !self.products.iter().any(|p| p.id == id) {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        }
        if self.selected.as_deref() == Some(id) {
            return Err(Error::SameSelection);
        }

        self.selected = Some(id.to_owned());
        self.log(EventKind::Selected { id: id.to_owned() });
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }
        let id = self.selected.clone().ok_or(Error::NoSelection)?;
        let index = self.products.iter().position(|p| p.id == id).unwrap();
        let product = &self.products[index];
        if product.stock == 0 {
            return Err(Error::SoldOut { id });
        }
        let price = product.price;
        if self.credit < price {
            return Err(Error::Underpaid {
                price,
                credit: self.credit,
            });
        }

        let amount = self.credit - price;
        let change = match self.choose_change(amount) {
            Some(coins) => coins,
            None => return Err(Error::NoChange { amount }),
        };

        let mut proposed = self.tubes;
        for &coin in &change {
            let tube_index = denom_index(coin).unwrap();
            proposed[tube_index] -= 1;
        }
        for &coin in &self.escrow {
            let tube_index = denom_index(coin).unwrap();
            if proposed[tube_index] == 100 {
                return Err(Error::TubeLimit { coin });
            }
            proposed[tube_index] += 1;
        }

        let change_value = change
            .iter()
            .fold(0u64, |sum, &coin| sum.saturating_add(u64::from(coin)));

        self.products[index].stock -= 1;
        self.tubes = proposed;
        self.credit = 0;
        self.escrow.clear();
        self.selected = None;
        self.sales = self.sales.saturating_add(u64::from(price));
        self.change = self.change.saturating_add(change_value);
        self.log(EventKind::Vended {
            id: id.clone(),
            price,
            change: change.clone(),
        });

        Ok(Receipt { id, price, change })
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }

        let coins: Vec<u16> = self.escrow.iter().rev().copied().collect();
        let refunded = u64::from(self.credit);
        self.credit = 0;
        self.escrow.clear();
        self.selected = None;
        self.refunded = self.refunded.saturating_add(refunded);
        self.log(EventKind::Refunded {
            coins: coins.clone(),
        });
        Ok(coins)
    }

    pub fn products(&self) -> Vec<Product> {
        let mut products = self.products.clone();
        products.sort_by(|a, b| a.id.as_bytes().cmp(b.id.as_bytes()));
        products
    }

    pub fn set_price(&mut self, id: &str, price: u16) -> Result<(), Error> {
        if !self.maintenance {
            return Err(Error::MaintenanceRequired);
        }
        let index = self
            .products
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| Error::UnknownProduct { id: id.to_owned() })?;
        if self.credit != 0 {
            return Err(Error::Busy);
        }
        if !(1..=100).contains(&price) {
            return Err(Error::BadPrice { id: id.to_owned() });
        }

        let old = self.products[index].price;
        self.products[index].price = price;
        self.log(EventKind::PriceChanged {
            id: id.to_owned(),
            old,
            new: price,
        });
        Ok(())
    }

    pub fn restock(&mut self, id: &str, count: u16) -> Result<(), Error> {
        if !self.maintenance {
            return Err(Error::MaintenanceRequired);
        }
        let index = self
            .products
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| Error::UnknownProduct { id: id.to_owned() })?;
        if count == 0 {
            return Err(Error::ZeroRestock);
        }
        let stock = u32::from(self.products[index].stock) + u32::from(count);
        if stock > 100 {
            return Err(Error::StockLimit { id: id.to_owned() });
        }

        self.products[index].stock = stock as u16;
        self.log(EventKind::Restocked {
            id: id.to_owned(),
            added: count,
            stock: stock as u16,
        });
        Ok(())
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn set_maintenance(&mut self, enabled: bool) -> Result<(), Error> {
        if enabled == self.maintenance {
            return Err(Error::AlreadyMode);
        }
        if self.credit != 0 {
            return Err(Error::Busy);
        }

        self.maintenance = enabled;
        self.selected = None;
        self.log(EventKind::Maintenance { enabled });
        Ok(())
    }

    pub fn audit(&self) -> Audit {
        let tube_value = self.tubes.iter().enumerate().fold(0u64, |sum, (i, count)| {
            sum.saturating_add(u64::from(*count) * u64::from(DENOMS[i]))
        });
        Audit {
            inserted: self.inserted,
            refunded: self.refunded,
            sales: self.sales,
            change: self.change,
            adjustment: self.adjustment,
            cash: tube_value.saturating_add(u64::from(self.credit)),
        }
    }

    pub fn reconcile(&mut self, expected: [u16; 4], actual: [u16; 4]) -> Result<(), Error> {
        if !self.maintenance {
            return Err(Error::MaintenanceRequired);
        }
        for i in 0..4 {
            if expected[i] != self.tubes[i] {
                return Err(Error::ReconcileMismatch {
                    coin: DENOMS[i],
                    expected: expected[i],
                    found: self.tubes[i],
                });
            }
        }
        for i in 0..4 {
            if actual[i] > 100 {
                return Err(Error::TubeLimit { coin: DENOMS[i] });
            }
        }

        let before_value = tube_value(self.tubes);
        let after_value = tube_value(actual);
        if before_value.abs_diff(after_value) > 20 {
            return Err(Error::ValueDrift {
                before: before_value,
                after: after_value,
            });
        }

        let delta = if after_value >= before_value {
            let d = (after_value - before_value).min(i64::MAX as u64) as i64;
            d
        } else {
            let d = (before_value - after_value).min(i64::MAX as u64) as i64;
            -d
        };

        self.tubes = actual;
        self.adjustment = self.adjustment.saturating_add(delta);
        self.log(EventKind::Reconciled {
            before: expected,
            after: actual,
        });
        Ok(())
    }
}

impl Machine {
    fn log(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
    }

    fn choose_change(&self, amount: u16) -> Option<Vec<u16>> {
        if amount == 0 {
            return Some(Vec::new());
        }

        let mut remaining = amount;
        let mut greedy = Vec::new();
        for i in (0..4).rev() {
            let denom = DENOMS[i];
            let take = self.tubes[i].min(remaining / denom);
            for _ in 0..take {
                greedy.push(denom);
            }
            remaining -= take * denom;
        }
        if remaining == 0 {
            return Some(greedy);
        }

        let mut best: Option<Vec<u16>> = None;
        let mut counts = [0u16; 4];
        search_change(3, amount, &self.tubes, &mut counts, &mut best);
        best
    }
}

fn denom_index(coin: u16) -> Option<usize> {
    match coin {
        1 => Some(0),
        3 => Some(1),
        4 => Some(2),
        10 => Some(3),
        _ => None,
    }
}

fn tube_value(tubes: [u16; 4]) -> u64 {
    tubes.iter().enumerate().fold(0u64, |sum, (i, count)| {
        sum.saturating_add(u64::from(*count) * u64::from(DENOMS[i]))
    })
}

fn search_change(
    index: i32,
    remaining: u16,
    tubes: &[u16; 4],
    counts: &mut [u16; 4],
    best: &mut Option<Vec<u16>>,
) {
    if index < 0 {
        if remaining == 0 {
            let candidate = counts_to_descending(*counts);
            let replace = match best {
                None => true,
                Some(current) => {
                    candidate.len() < current.len()
                        || (candidate.len() == current.len()
                            && candidate.as_slice() > current.as_slice())
                }
            };
            if replace {
                *best = Some(candidate);
            }
        }
        return;
    }

    let i = index as usize;
    let denom = DENOMS[i];
    let max_take = tubes[i].min(remaining / denom);
    for take in 0..=max_take {
        counts[i] = take;
        search_change(
            index - 1,
            remaining - take * denom,
            tubes,
            counts,
            best,
        );
    }
    counts[i] = 0;
}

fn counts_to_descending(counts: [u16; 4]) -> Vec<u16> {
    let mut result = Vec::new();
    for i in (0..4).rev() {
        for _ in 0..counts[i] {
            result.push(DENOMS[i]);
        }
    }
    result
}

fn main() {}