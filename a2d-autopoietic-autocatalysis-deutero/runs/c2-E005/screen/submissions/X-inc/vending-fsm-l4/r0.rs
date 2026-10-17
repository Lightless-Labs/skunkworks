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

#[derive(Clone, Debug, PartialEq, Eq)]
struct StoredProduct {
    id: String,
    price: u16,
    stock: u16,
}

pub struct Machine {
    products: Vec<StoredProduct>,
    product_indices: HashMap<String, usize>,
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

fn denomination_index(coin: u16) -> Option<usize> {
    match coin {
        1 => Some(0),
        3 => Some(1),
        4 => Some(2),
        10 => Some(3),
        _ => None,
    }
}

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase())
}

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut stored = Vec::with_capacity(products.len());
        let mut product_indices = HashMap::with_capacity(products.len());

        for product in products {
            if !valid_id(&product.id) {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }
            if product_indices.contains_key(&product.id) {
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

            let index = stored.len();
            product_indices.insert(product.id.clone(), index);
            stored.push(StoredProduct {
                id: product.id,
                price: product.price,
                stock: product.stock,
            });
        }

        for (index, count) in tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[index],
                });
            }
        }

        Ok(Self {
            products: stored,
            product_indices,
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

    fn log(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
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
        if denomination_index(coin).is_none() {
            return Err(Error::BadCoin { coin });
        }
        let new_credit = u32::from(self.credit) + u32::from(coin);
        if new_credit > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit = new_credit as u16;
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
        if !self.product_indices.contains_key(id) {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        }
        if self.selected.as_deref() == Some(id) {
            return Err(Error::SameSelection);
        }

        let selected = id.to_owned();
        self.selected = Some(selected.clone());
        self.log(EventKind::Selected { id: selected });
        Ok(())
    }

    fn choose_change(&self, amount: u16) -> Result<Vec<u16>, Error> {
        if amount == 0 {
            return Ok(Vec::new());
        }

        let mut remaining = amount;
        let mut greedy_counts = [0u16; 4];
        for index in (0..4).rev() {
            let denomination = DENOMINATIONS[index];
            let take = self.tubes[index].min(remaining / denomination);
            greedy_counts[index] = take;
            remaining -= take * denomination;
        }

        let counts = if remaining == 0 {
            greedy_counts
        } else {
            let mut best: Option<[u16; 4]> = None;
            let max10 = self.tubes[3].min(amount / 10);
            for n10 in 0..=max10 {
                let after10 = amount - n10 * 10;
                let max4 = self.tubes[2].min(after10 / 4);
                for n4 in 0..=max4 {
                    let after4 = after10 - n4 * 4;
                    let max3 = self.tubes[1].min(after4 / 3);
                    for n3 in 0..=max3 {
                        let after3 = after4 - n3 * 3;
                        if after3 > self.tubes[0] {
                            continue;
                        }
                        let candidate = [after3, n3, n4, n10];
                        let candidate_total: u32 = candidate.iter().map(|n| u32::from(*n)).sum();
                        let better = match best {
                            None => true,
                            Some(current) => {
                                let current_total: u32 =
                                    current.iter().map(|n| u32::from(*n)).sum();
                                if candidate_total != current_total {
                                    candidate_total < current_total
                                } else {
                                    (0..4)
                                        .rev()
                                        .find_map(|i| {
                                            (candidate[i] != current[i])
                                                .then_some(candidate[i] > current[i])
                                        })
                                        .unwrap_or(false)
                                }
                            }
                        };
                        if better {
                            best = Some(candidate);
                        }
                    }
                }
            }
            best.ok_or(Error::NoChange { amount })?
        };

        let mut coins = Vec::new();
        for index in (0..4).rev() {
            for _ in 0..counts[index] {
                coins.push(DENOMINATIONS[index]);
            }
        }
        Ok(coins)
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }

        let id = self.selected.clone().ok_or(Error::NoSelection)?;
        let index = *self
            .product_indices
            .get(&id)
            .expect("selected product must be in catalog");
        let product = &self.products[index];

        if product.stock == 0 {
            return Err(Error::SoldOut { id });
        }
        if self.credit < product.price {
            return Err(Error::Underpaid {
                price: product.price,
                credit: self.credit,
            });
        }

        let price = product.price;
        let amount = self.credit - price;
        let change_coins = self.choose_change(amount)?;
        let mut proposed_tubes = self.tubes;

        for coin in &change_coins {
            let tube_index = denomination_index(*coin).expect("change uses valid denominations");
            proposed_tubes[tube_index] -= 1;
        }
        for coin in &self.escrow {
            let tube_index = denomination_index(*coin).expect("escrow uses valid denominations");
            proposed_tubes[tube_index] += 1;
        }
        for (index, count) in proposed_tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[index],
                });
            }
        }

        let change_value: u64 = change_coins.iter().map(|coin| u64::from(*coin)).sum();
        let receipt = Receipt {
            id: id.clone(),
            price,
            change: change_coins.clone(),
        };

        self.tubes = proposed_tubes;
        self.products[index].stock -= 1;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.sales = self.sales.saturating_add(u64::from(price));
        self.change = self.change.saturating_add(change_value);
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
        self.refunded = self.refunded.saturating_add(u64::from(self.credit));
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.log(EventKind::Refunded {
            coins: coins.clone(),
        });
        Ok(coins)
    }

    pub fn products(&self) -> Vec<Product> {
        let mut result: Vec<Product> = self
            .products
            .iter()
            .map(|product| Product {
                id: product.id.clone(),
                price: product.price,
                stock: product.stock,
            })
            .collect();
        result.sort_by(|a, b| a.id.as_bytes().cmp(b.id.as_bytes()));
        result
    }

    pub fn set_price(&mut self, id: &str, price: u16) -> Result<(), Error> {
        if !self.maintenance {
            return Err(Error::MaintenanceRequired);
        }
        let Some(&index) = self.product_indices.get(id) else {
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
        let Some(&index) = self.product_indices.get(id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
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
        let tube_value: u64 = self
            .tubes
            .iter()
            .zip(DENOMINATIONS.iter())
            .map(|(count, denomination)| u64::from(*count) * u64::from(*denomination))
            .sum();
        Audit {
            inserted: self.inserted,
            refunded: self.refunded,
            sales: self.sales,
            change: self.change,
            adjustment: self.adjustment,
            cash: tube_value + u64::from(self.credit),
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

        let value = |counts: &[u16; 4]| -> u64 {
            counts
                .iter()
                .zip(DENOMINATIONS.iter())
                .map(|(count, denomination)| {
                    u64::from(*count) * u64::from(*denomination)
                })
                .sum()
        };
        let before_value = value(&self.tubes);
        let after_value = value(&actual);
        let difference = before_value.abs_diff(after_value);
        if difference > 20 {
            return Err(Error::ValueDrift {
                before: before_value,
                after: after_value,
            });
        }

        let delta = i128::from(after_value) - i128::from(before_value);
        let adjustment = (i128::from(self.adjustment) + delta)
            .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
        let before = self.tubes;
        self.tubes = actual;
        self.adjustment = adjustment;
        self.log(EventKind::Reconciled {
            before,
            after: actual,
        });
        Ok(())
    }
}