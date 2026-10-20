use std::collections::HashSet;

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
    credit: u16,
    selected: Option<String>,
    escrow: Vec<u16>,
    tubes: [u16; 4],
    maintenance: bool,
    events: Vec<Event>,
    inserted: u64,
    refunded: u64,
    sales: u64,
    change: u64,
    adjustment: i64,
}

const DENOMINATIONS: [u16; 4] = [1, 3, 4, 10];

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut seen = HashSet::new();
        for product in &products {
            if !valid_id(&product.id) {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }
            if !seen.insert(product.id.clone()) {
                return Err(Error::DuplicateId {
                    id: product.id.clone(),
                });
            }
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

        for (index, count) in tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[index],
                });
            }
        }

        Ok(Self {
            products,
            credit: 0,
            selected: None,
            escrow: Vec::new(),
            tubes,
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

        denomination_index(coin).ok_or(Error::BadCoin { coin })?;
        let new_credit = self.credit as u32 + coin as u32;
        if new_credit > 100 {
            return Err(Error::CreditLimit);
        }
        let new_credit = new_credit as u16;

        self.credit = new_credit;
        self.escrow.push(coin);
        self.inserted = self.inserted.saturating_add(coin as u64);
        self.log(EventKind::Inserted {
            coin,
            credit: new_credit,
        });
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }

        if !self.products.iter().any(|product| product.id == id) {
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
        let product_index = self
            .products
            .iter()
            .position(|product| product.id == id)
            .expect("selected product must be in catalog");
        let product = &self.products[product_index];

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
        let change = make_change(amount, self.tubes).ok_or(Error::NoChange { amount })?;
        let mut proposed_tubes = self.tubes;

        for (index, count) in change_counts(&change).iter().enumerate() {
            proposed_tubes[index] -= count;
        }
        for coin in &self.escrow {
            let index = denomination_index(*coin).expect("escrow contains accepted coins");
            proposed_tubes[index] += 1;
        }

        for (index, count) in proposed_tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[index],
                });
            }
        }

        self.tubes = proposed_tubes;
        self.products[product_index].stock -= 1;
        self.credit = 0;
        self.escrow.clear();
        self.selected = None;
        self.sales = self.sales.saturating_add(price as u64);
        self.change = self
            .change
            .saturating_add(change.iter().map(|coin| *coin as u64).sum());
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
        self.refunded = self.refunded.saturating_add(self.credit as u64);
        self.credit = 0;
        self.escrow.clear();
        self.selected = None;
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
            .position(|product| product.id == id)
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
            .position(|product| product.id == id)
            .ok_or_else(|| Error::UnknownProduct { id: id.to_owned() })?;
        if count == 0 {
            return Err(Error::ZeroRestock);
        }

        let total = self.products[index].stock as u32 + count as u32;
        if total > 100 {
            return Err(Error::StockLimit { id: id.to_owned() });
        }

        self.products[index].stock = total as u16;
        self.log(EventKind::Restocked {
            id: id.to_owned(),
            added: count,
            stock: total as u16,
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
        let tube_value = tube_value(self.tubes);
        Audit {
            inserted: self.inserted,
            refunded: self.refunded,
            sales: self.sales,
            change: self.change,
            adjustment: self.adjustment,
            cash: tube_value.saturating_add(self.credit as u64),
        }
    }

    pub fn reconcile(
        &mut self,
        expected: [u16; 4],
        actual: [u16; 4],
    ) -> Result<(), Error> {
        if !self.maintenance {
            return Err(Error::MaintenanceRequired);
        }

        for index in 0..4 {
            if expected[index] != self.tubes[index] {
                return Err(Error::ReconcileMismatch {
                    coin: DENOMINATIONS[index],
                    expected: expected[index],
                    found: self.tubes[index],
                });
            }
        }
        for index in 0..4 {
            if actual[index] > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[index],
                });
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

        let delta = after_value as i128 - before_value as i128;
        let delta = delta.clamp(i64::MIN as i128, i64::MAX as i128) as i64;
        self.adjustment = self.adjustment.saturating_add(delta);
        let before = self.tubes;
        self.tubes = actual;
        self.log(EventKind::Reconciled {
            before,
            after: actual,
        });
        Ok(())
    }

    fn log(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
    }
}

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_lowercase())
}

fn denomination_index(coin: u16) -> Option<usize> {
    DENOMINATIONS
        .iter()
        .position(|denomination| *denomination == coin)
}

fn tube_value(tubes: [u16; 4]) -> u64 {
    tubes
        .iter()
        .zip(DENOMINATIONS)
        .map(|(count, denomination)| *count as u64 * denomination as u64)
        .sum()
}

fn change_counts(change: &[u16]) -> [u16; 4] {
    let mut counts = [0; 4];
    for coin in change {
        if let Some(index) = denomination_index(*coin) {
            counts[index] += 1;
        }
    }
    counts
}

fn make_change(amount: u16, tubes: [u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    let descending = [10u16, 4, 3, 1];
    let mut remaining = amount;
    let mut greedy = Vec::new();
    for coin in descending {
        let index = denomination_index(coin).unwrap();
        let take = tubes[index].min(remaining / coin);
        greedy.extend(std::iter::repeat_n(coin, take as usize));
        remaining -= take * coin;
    }
    if remaining == 0 {
        return Some(greedy);
    }

    let mut best: Option<Vec<u16>> = None;
    fn search(
        position: usize,
        remaining: u16,
        tubes: [u16; 4],
        current: &mut Vec<u16>,
        best: &mut Option<Vec<u16>>,
    ) {
        if remaining == 0 {
            let replace = match best {
                None => true,
                Some(existing) => {
                    current.len() < existing.len()
                        || (current.len() == existing.len()
                            && current.as_slice() > existing.as_slice())
                }
            };
            if replace {
                *best = Some(current.clone());
            }
            return;
        }
        if position == 4 {
            return;
        }
        if let Some(existing) = best.as_ref() {
            if current.len() >= existing.len() {
                return;
            }
        }

        let coin = [10u16, 4, 3, 1][position];
        let index = denomination_index(coin).unwrap();
        let max = tubes[index].min(remaining / coin);
        for count in (0..=max).rev() {
            let old_len = current.len();
            current.extend(std::iter::repeat_n(coin, count as usize));
            search(
                position + 1,
                remaining - count * coin,
                tubes,
                current,
                best,
            );
            current.truncate(old_len);
        }
    }

    search(0, amount, tubes, &mut Vec::new(), &mut best);
    best
}