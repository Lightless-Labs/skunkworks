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

const DENOMS: [u16; 4] = [1, 3, 4, 10];

fn denom_index(coin: u16) -> Option<usize> {
    DENOMS.iter().position(|&d| d == coin)
}

fn product_index(products: &[Product], id: &str) -> Option<usize> {
    products.iter().position(|p| p.id == id)
}

fn valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (1..=8).contains(&bytes.len()) && bytes.iter().all(|b| b.is_ascii_lowercase())
}

fn push_event(events: &mut Vec<Event>, kind: EventKind) {
    let seq = events.len();
    events.push(Event { seq, kind });
}

fn add_u64(a: u64, b: u64) -> u64 {
    a.saturating_add(b)
}

fn tube_value(tubes: &[u16; 4]) -> u64 {
    tubes
        .iter()
        .zip(DENOMS)
        .map(|(&count, denom)| u64::from(count) * u64::from(denom))
        .sum()
}

fn greedy_change(amount: u16, tubes: &[u16; 4]) -> Option<Vec<u16>> {
    let mut remaining = amount;
    let mut counts = [0u16; 4];

    for i in (0..4).rev() {
        let take = tubes[i].min(remaining / DENOMS[i]);
        counts[i] = take;
        remaining -= take * DENOMS[i];
    }

    if remaining != 0 {
        return None;
    }

    Some(expand_change(counts))
}

fn expand_change(counts: [u16; 4]) -> Vec<u16> {
    let mut coins = Vec::new();
    for i in (0..4).rev() {
        for _ in 0..counts[i] {
            coins.push(DENOMS[i]);
        }
    }
    coins
}

fn exact_min_change(amount: u16, tubes: &[u16; 4]) -> Option<Vec<u16>> {
    fn search(
        idx: isize,
        remaining: u16,
        tubes: &[u16; 4],
        chosen: &mut [u16; 4],
        best: &mut Option<[u16; 4]>,
        best_count: &mut u32,
    ) {
        if idx < 0 {
            if remaining == 0 {
                let count: u32 = chosen.iter().map(|&x| u32::from(x)).sum();
                let is_better = match best {
                    None => true,
                    Some(current) => {
                        if count < *best_count {
                            true
                        } else if count > *best_count {
                            false
                        } else {
                            let mut better = false;
                            for i in (0..4).rev() {
                                if chosen[i] != current[i] {
                                    better = chosen[i] > current[i];
                                    break;
                                }
                            }
                            better
                        }
                    }
                };
                if is_better {
                    *best = Some(*chosen);
                    *best_count = count;
                }
            }
            return;
        }

        let i = idx as usize;
        let max_take = tubes[i].min(remaining / DENOMS[i]);
        for take in 0..=max_take {
            chosen[i] = take;
            search(
                idx - 1,
                remaining - take * DENOMS[i],
                tubes,
                chosen,
                best,
                best_count,
            );
        }
        chosen[i] = 0;
    }

    let mut chosen = [0u16; 4];
    let mut best = None;
    let mut best_count = u32::MAX;
    search(3, amount, tubes, &mut chosen, &mut best, &mut best_count);
    best.map(expand_change)
}

fn change_counts(change: &[u16]) -> [u16; 4] {
    let mut counts = [0u16; 4];
    for &coin in change {
        if let Some(i) = denom_index(coin) {
            counts[i] = counts[i].saturating_add(1);
        }
    }
    counts
}

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut seen: Vec<&str> = Vec::new();
        for product in &products {
            if !valid_id(&product.id) {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }
            if seen.iter().any(|&id| id == product.id) {
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
        if denom_index(coin).is_none() {
            return Err(Error::BadCoin { coin });
        }
        let next_credit = u32::from(self.credit) + u32::from(coin);
        if next_credit > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit = next_credit as u16;
        self.escrow.push(coin);
        self.inserted = add_u64(self.inserted, u64::from(coin));
        push_event(
            &mut self.events,
            EventKind::Inserted {
                coin,
                credit: self.credit,
            },
        );
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }
        let Some(index) = product_index(&self.products, id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if self.selected.as_deref() == Some(self.products[index].id.as_str()) {
            return Err(Error::SameSelection);
        }

        let selected_id = self.products[index].id.clone();
        self.selected = Some(selected_id.clone());
        push_event(
            &mut self.events,
            EventKind::Selected { id: selected_id },
        );
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }
        let Some(selected_id) = self.selected.clone() else {
            return Err(Error::NoSelection);
        };
        let index = product_index(&self.products, &selected_id)
            .expect("selected product must be in the fixed catalog");
        let product = &self.products[index];

        if product.stock == 0 {
            return Err(Error::SoldOut {
                id: product.id.clone(),
            });
        }
        if self.credit < product.price {
            return Err(Error::Underpaid {
                price: product.price,
                credit: self.credit,
            });
        }

        let price = product.price;
        let amount = self.credit - price;
        let change = if amount == 0 {
            Vec::new()
        } else if let Some(greedy) = greedy_change(amount, &self.tubes) {
            greedy
        } else {
            exact_min_change(amount, &self.tubes)
                .ok_or(Error::NoChange { amount })?
        };

        let dispensed = change_counts(&change);
        let mut proposed = self.tubes;
        for i in 0..4 {
            proposed[i] -= dispensed[i];
        }
        for &coin in &self.escrow {
            let i = denom_index(coin).expect("escrow contains accepted denominations");
            proposed[i] += 1;
        }
        for i in 0..4 {
            if proposed[i] > 100 {
                return Err(Error::TubeLimit { coin: DENOMS[i] });
            }
        }

        self.tubes = proposed;
        self.products[index].stock -= 1;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        let change_value: u64 = change.iter().map(|&coin| u64::from(coin)).sum();
        self.sales = add_u64(self.sales, u64::from(price));
        self.change = add_u64(self.change, change_value);
        push_event(
            &mut self.events,
            EventKind::Vended {
                id: selected_id.clone(),
                price,
                change: change.clone(),
            },
        );

        Ok(Receipt {
            id: selected_id,
            price,
            change,
        })
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }
        let coins: Vec<u16> = self.escrow.iter().rev().copied().collect();
        let refunded_value = u64::from(self.credit);

        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.refunded = add_u64(self.refunded, refunded_value);
        push_event(
            &mut self.events,
            EventKind::Refunded {
                coins: coins.clone(),
            },
        );
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
        let Some(index) = product_index(&self.products, id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if self.credit != 0 {
            return Err(Error::Busy);
        }
        if !(1..=100).contains(&price) {
            return Err(Error::BadPrice { id: id.to_owned() });
        }

        let old = self.products[index].price;
        self.products[index].price = price;
        push_event(
            &mut self.events,
            EventKind::PriceChanged {
                id: self.products[index].id.clone(),
                old,
                new: price,
            },
        );
        Ok(())
    }

    pub fn restock(&mut self, id: &str, count: u16) -> Result<(), Error> {
        if !self.maintenance {
            return Err(Error::MaintenanceRequired);
        }
        let Some(index) = product_index(&self.products, id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if count == 0 {
            return Err(Error::ZeroRestock);
        }
        let new_stock = u32::from(self.products[index].stock) + u32::from(count);
        if new_stock > 100 {
            return Err(Error::StockLimit { id: id.to_owned() });
        }

        self.products[index].stock = new_stock as u16;
        push_event(
            &mut self.events,
            EventKind::Restocked {
                id: self.products[index].id.clone(),
                added: count,
                stock: self.products[index].stock,
            },
        );
        Ok(())
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn set_maintenance(&mut self, enabled: bool) -> Result<(), Error> {
        if self.maintenance == enabled {
            return Err(Error::AlreadyMode);
        }
        if self.credit != 0 {
            return Err(Error::Busy);
        }

        self.maintenance = enabled;
        self.selected = None;
        push_event(&mut self.events, EventKind::Maintenance { enabled });
        Ok(())
    }

    pub fn audit(&self) -> Audit {
        Audit {
            inserted: self.inserted,
            refunded: self.refunded,
            sales: self.sales,
            change: self.change,
            adjustment: self.adjustment,
            cash: tube_value(&self.tubes) + u64::from(self.credit),
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

        let before_value = tube_value(&self.tubes);
        let after_value = tube_value(&actual);
        if before_value.abs_diff(after_value) > 20 {
            return Err(Error::ValueDrift {
                before: before_value,
                after: after_value,
            });
        }

        let delta = i128::from(after_value) - i128::from(before_value);
        let delta_i64 = delta as i64;
        self.adjustment = self.adjustment.saturating_add(delta_i64);
        let before = self.tubes;
        self.tubes = actual;
        push_event(
            &mut self.events,
            EventKind::Reconciled {
                before,
                after: actual,
            },
        );
        Ok(())
    }
}