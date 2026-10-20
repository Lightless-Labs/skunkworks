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

fn denomination_index(coin: u16) -> Option<usize> {
    DENOMINATIONS.iter().position(|&d| d == coin)
}

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase())
}

fn value_of(counts: &[u16; 4]) -> u64 {
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

        let mut seen: Vec<&str> = Vec::new();
        for product in &products {
            if !valid_id(&product.id) {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }
            if seen.contains(&product.id.as_str()) {
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

        for (i, &count) in tubes.iter().enumerate() {
            if count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
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
        let index = denomination_index(coin).ok_or(Error::BadCoin { coin })?;
        let new_credit = u32::from(self.credit) + u32::from(coin);
        if new_credit > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit = new_credit as u16;
        self.escrow.push(coin);
        self.inserted = self.inserted.saturating_add(u64::from(coin));
        let _ = index;
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
            .expect("selected product must remain in the catalog");
        let product = &self.products[product_index];

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
        let chosen = choose_change(self.tubes, amount).ok_or(Error::NoChange { amount })?;

        let mut proposed_tubes = self.tubes;
        for i in 0..4 {
            proposed_tubes[i] -= chosen[i];
        }
        for &coin in &self.escrow {
            let i = denomination_index(coin).expect("escrow contains accepted coins");
            proposed_tubes[i] += 1;
        }
        for i in 0..4 {
            if proposed_tubes[i] > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
                });
            }
        }

        let change_coins = coins_descending(&chosen);
        let receipt = Receipt {
            id: id.clone(),
            price,
            change: change_coins.clone(),
        };

        self.tubes = proposed_tubes;
        self.products[product_index].stock -= 1;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.sales = self.sales.saturating_add(u64::from(price));
        self.change = self
            .change
            .saturating_add(u64::from(amount));
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
        let new_stock = u32::from(self.products[index].stock) + u32::from(count);
        if new_stock > 100 {
            return Err(Error::StockLimit { id: id.to_owned() });
        }

        self.products[index].stock = new_stock as u16;
        self.log(EventKind::Restocked {
            id: id.to_owned(),
            added: count,
            stock: new_stock as u16,
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
        let cash = value_of(&self.tubes).saturating_add(u64::from(self.credit));
        Audit {
            inserted: self.inserted,
            refunded: self.refunded,
            sales: self.sales,
            change: self.change,
            adjustment: self.adjustment,
            cash,
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

        let before = value_of(&self.tubes);
        let after = value_of(&actual);
        if before.abs_diff(after) > 20 {
            return Err(Error::ValueDrift { before, after });
        }

        let old_tubes = self.tubes;
        self.tubes = actual;
        let delta = i128::from(after) - i128::from(before);
        let adjustment = (i128::from(self.adjustment) + delta)
            .clamp(i128::from(i64::MIN), i128::from(i64::MAX));
        self.adjustment = adjustment as i64;
        self.log(EventKind::Reconciled {
            before: old_tubes,
            after: actual,
        });
        Ok(())
    }

    fn log(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
    }
}

fn choose_change(tubes: [u16; 4], amount: u16) -> Option<[u16; 4]> {
    let mut greedy = [0u16; 4];
    let mut remaining = amount;

    for i in (0..4).rev() {
        let denomination = DENOMINATIONS[i];
        let take = tubes[i].min(remaining / denomination);
        greedy[i] = take;
        remaining -= take * denomination;
    }
    if remaining == 0 {
        return Some(greedy);
    }

    let mut best: Option<[u16; 4]> = None;
    let mut best_count = u32::MAX;
    let mut candidate = [0u16; 4];
    search_change(
        3,
        amount,
        &tubes,
        &mut candidate,
        &mut best,
        &mut best_count,
    );
    best
}

fn search_change(
    index: i32,
    remaining: u16,
    tubes: &[u16; 4],
    candidate: &mut [u16; 4],
    best: &mut Option<[u16; 4]>,
    best_count: &mut u32,
) {
    if index < 0 {
        if remaining == 0 {
            let count: u32 = candidate.iter().map(|&n| u32::from(n)).sum();
            let is_better = match best {
                None => true,
                Some(current) if count < *best_count => true,
                Some(current) if count == *best_count => {
                    (0..4).rev().find_map(|i| {
                        if candidate[i] != current[i] {
                            Some(candidate[i] > current[i])
                        } else {
                            None
                        }
                    }) == Some(true)
                }
                _ => false,
            };
            if is_better {
                *best = Some(*candidate);
                *best_count = count;
            }
        }
        return;
    }

    let i = index as usize;
    let denomination = DENOMINATIONS[i];
    let max_take = tubes[i].min(remaining / denomination);
    for take in 0..=max_take {
        candidate[i] = take;
        search_change(
            index - 1,
            remaining - take * denomination,
            tubes,
            candidate,
            best,
            best_count,
        );
    }
    candidate[i] = 0;
}

fn coins_descending(counts: &[u16; 4]) -> Vec<u16> {
    let mut coins = Vec::new();
    for i in (0..4).rev() {
        for _ in 0..counts[i] {
            coins.push(DENOMINATIONS[i]);
        }
    }
    coins
}