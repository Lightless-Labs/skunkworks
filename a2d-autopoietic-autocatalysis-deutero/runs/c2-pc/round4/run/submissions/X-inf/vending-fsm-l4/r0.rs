use std::collections::HashMap;

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
    index: HashMap<String, usize>,
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

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase())
}

fn coin_index(coin: u16) -> Option<usize> {
    match coin {
        1 => Some(0),
        3 => Some(1),
        4 => Some(2),
        10 => Some(3),
        _ => None,
    }
}

fn coin_value(counts: &[u16; 4]) -> u64 {
    counts
        .iter()
        .zip(DENOMINATIONS)
        .map(|(&count, denomination)| u64::from(count) * u64::from(denomination))
        .sum()
}

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut index = HashMap::new();
        for (position, product) in products.iter().enumerate() {
            if !valid_id(&product.id) {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }
            if index.contains_key(&product.id) {
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
            index.insert(product.id.clone(), position);
        }

        for (i, &count) in tubes.iter().enumerate() {
            if count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
                });
            }
        }

        Ok(Self {
            products,
            index,
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

    fn log(&mut self, kind: EventKind) {
        self.events.push(Event {
            seq: self.events.len(),
            kind,
        });
    }

    fn find_product(&self, id: &str) -> Result<usize, Error> {
        self.index
            .get(id)
            .copied()
            .ok_or_else(|| Error::UnknownProduct { id: id.to_owned() })
    }

    pub fn insert(&mut self, coin: u16) -> Result<(), Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }
        if coin_index(coin).is_none() {
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
        self.find_product(id)?;
        if self.selected.as_deref() == Some(id) {
            return Err(Error::SameSelection);
        }

        let id = id.to_owned();
        self.selected = Some(id.clone());
        self.log(EventKind::Selected { id });
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }

        let id = self.selected.clone().ok_or(Error::NoSelection)?;
        let position = self.index[&id];
        let product = &self.products[position];
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
        let mut chosen = [0u16; 4];
        let mut remaining = amount;

        for i in (0..4).rev() {
            let denomination = DENOMINATIONS[i];
            let take = self.tubes[i].min(remaining / denomination);
            chosen[i] = take;
            remaining -= take * denomination;
        }

        if remaining != 0 {
            let mut best: Option<[u16; 4]> = None;
            let mut best_total = u32::MAX;
            for n10 in 0..=self.tubes[3].min(amount / 10) {
                for n4 in 0..=self.tubes[2].min(amount / 4) {
                    for n3 in 0..=self.tubes[1].min(amount / 3) {
                        let used = n10 * 10 + n4 * 4 + n3 * 3;
                        if used > amount {
                            continue;
                        }
                        let remainder = amount - used;
                        if remainder > self.tubes[0] {
                            continue;
                        }
                        let candidate = [remainder, n3, n4, n10];
                        let total = u32::from(remainder + n3 + n4 + n10);
                        let better = match best {
                            None => true,
                            Some(current) => {
                                total < best_total
                                    || (total == best_total
                                        && (3..=0).rev().any(|_| false))
                                    || (total == best_total
                                        && (0..4)
                                            .rev()
                                            .find_map(|i| {
                                                if candidate[i] != current[i] {
                                                    Some(candidate[i] > current[i])
                                                } else {
                                                    None
                                                }
                                            })
                                            .unwrap_or(false))
                            }
                        };
                        if better {
                            best = Some(candidate);
                            best_total = total;
                        }
                    }
                }
            }
            chosen = best.ok_or(Error::NoChange { amount })?;
        }

        let mut change_coins = Vec::new();
        for i in (0..4).rev() {
            for _ in 0..chosen[i] {
                change_coins.push(DENOMINATIONS[i]);
            }
        }

        let mut proposed = self.tubes;
        for i in 0..4 {
            proposed[i] -= chosen[i];
        }
        for &coin in &self.escrow {
            let i = coin_index(coin).expect("escrow only contains accepted coins");
            proposed[i] += 1;
        }
        for i in 0..4 {
            if proposed[i] > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
                });
            }
        }

        self.tubes = proposed;
        self.products[position].stock -= 1;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.sales = self.sales.saturating_add(u64::from(price));
        let change_value = u64::from(amount);
        self.change = self.change.saturating_add(change_value);

        let receipt = Receipt {
            id: id.clone(),
            price,
            change: change_coins.clone(),
        };
        self.log(EventKind::Vended {
            id,
            price,
            change: change_coins,
        });
        Ok(receipt)
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }

        let coins: Vec<u16> = self.escrow.iter().rev().copied().collect();
        let refunded = u64::from(self.credit);
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
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
        let position = self.find_product(id)?;
        if self.credit != 0 {
            return Err(Error::Busy);
        }
        if !(1..=100).contains(&price) {
            return Err(Error::BadPrice { id: id.to_owned() });
        }

        let old = self.products[position].price;
        self.products[position].price = price;
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
        let position = self.find_product(id)?;
        if count == 0 {
            return Err(Error::ZeroRestock);
        }
        let stock = u32::from(self.products[position].stock) + u32::from(count);
        if stock > 100 {
            return Err(Error::StockLimit { id: id.to_owned() });
        }

        self.products[position].stock = stock as u16;
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
        Audit {
            inserted: self.inserted,
            refunded: self.refunded,
            sales: self.sales,
            change: self.change,
            adjustment: self.adjustment,
            cash: coin_value(&self.tubes) + u64::from(self.credit),
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

        for i in 0..4 {
            if expected[i] != self.tubes[i] {
                return Err(Error::ReconcileMismatch {
                    coin: DENOMINATIONS[i],
                    expected: expected[i],
                    found: self.tubes[i],
                });
            }
        }

        for i in 0..4 {
            if actual[i] > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
                });
            }
        }

        let before = coin_value(&self.tubes);
        let after = coin_value(&actual);
        if before.abs_diff(after) > 20 {
            return Err(Error::ValueDrift { before, after });
        }

        let delta = i128::from(after) - i128::from(before);
        self.adjustment = (i128::from(self.adjustment) + delta)
            .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
        let previous = self.tubes;
        self.tubes = actual;
        self.log(EventKind::Reconciled {
            before: previous,
            after: actual,
        });
        Ok(())
    }
}