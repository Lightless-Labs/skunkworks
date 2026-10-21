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

const DENOMS: [u16; 4] = [1, 3, 4, 10];

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut seen = HashMap::new();
        for product in &products {
            if !valid_id(&product.id) {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }
            if seen.contains_key(&product.id) {
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
            seen.insert(product.id.clone(), ());
        }

        for (i, count) in tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit { coin: DENOMS[i] });
            }
        }

        let mut index = HashMap::new();
        for (i, product) in products.iter().enumerate() {
            index.insert(product.id.clone(), i);
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
        if !self.index.contains_key(id) {
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
        let product_index = *self
            .index
            .get(&id)
            .expect("selected product is always in the catalog");
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
        let change = choose_change(amount, self.tubes)
            .ok_or(Error::NoChange { amount })?;

        let mut proposed = self.tubes;
        for coin in &change {
            let i = denom_index(*coin).expect("change uses accepted denominations");
            proposed[i] -= 1;
        }
        for coin in &self.escrow {
            let i = denom_index(*coin).expect("escrow uses accepted denominations");
            proposed[i] += 1;
        }
        for (i, count) in proposed.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit { coin: DENOMS[i] });
            }
        }

        self.tubes = proposed;
        self.products[product_index].stock -= 1;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.sales = self.sales.saturating_add(u64::from(price));
        let change_value = amount as u64;
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
        let Some(&i) = self.index.get(id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if self.credit != 0 {
            return Err(Error::Busy);
        }
        if !(1..=100).contains(&price) {
            return Err(Error::BadPrice { id: id.to_owned() });
        }

        let old = self.products[i].price;
        self.products[i].price = price;
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
        let Some(&i) = self.index.get(id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if count == 0 {
            return Err(Error::ZeroRestock);
        }
        let stock = u32::from(self.products[i].stock) + u32::from(count);
        if stock > 100 {
            return Err(Error::StockLimit { id: id.to_owned() });
        }

        self.products[i].stock = stock as u16;
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
        if self.maintenance == enabled {
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
            .zip(DENOMS.iter())
            .map(|(&count, &coin)| u64::from(count) * u64::from(coin))
            .sum();
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

        let delta = i128::from(after_value) - i128::from(before_value);
        let delta = delta as i64;
        self.adjustment = self.adjustment.saturating_add(delta);
        let before = self.tubes;
        self.tubes = actual;
        self.log(EventKind::Reconciled {
            before,
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
}

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase())
}

fn denom_index(coin: u16) -> Option<usize> {
    DENOMS.iter().position(|&d| d == coin)
}

fn tube_value(tubes: [u16; 4]) -> u64 {
    tubes
        .iter()
        .zip(DENOMS.iter())
        .map(|(&count, &coin)| u64::from(count) * u64::from(coin))
        .sum()
}

fn choose_change(amount: u16, tubes: [u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    let mut remaining = amount;
    let mut greedy = [0u16; 4];
    for i in (0..4).rev() {
        let take = tubes[i].min(remaining / DENOMS[i]);
        greedy[i] = take;
        remaining -= take * DENOMS[i];
    }
    if remaining == 0 {
        return Some(expand_change(greedy));
    }

    let mut best: Option<[u16; 4]> = None;
    let mut best_count = u32::MAX;
    search_change(
        3,
        amount,
        tubes,
        [0; 4],
        0,
        &mut best,
        &mut best_count,
    );
    best.map(expand_change)
}

fn search_change(
    i: isize,
    remaining: u16,
    tubes: [u16; 4],
    current: [u16; 4],
    count: u32,
    best: &mut Option<[u16; 4]>,
    best_count: &mut u32,
) {
    if i < 0 {
        if remaining == 0
            && (count < *best_count
                || (count == *best_count
                    && best.as_ref().is_none_or(|b| descending_lex_greater(&current, b))))
        {
            *best = Some(current);
            *best_count = count;
        }
        return;
    }

    let idx = i as usize;
    let coin = DENOMS[idx];
    let max_take = tubes[idx].min(remaining / coin);
    for take in 0..=max_take {
        let next_count = count + u32::from(take);
        if next_count > *best_count {
            continue;
        }
        let next_remaining = remaining - take * coin;
        let mut next = current;
        next[idx] = take;
        search_change(
            i - 1,
            next_remaining,
            tubes,
            next,
            next_count,
            best,
            best_count,
        );
    }
}

fn descending_lex_greater(a: &[u16; 4], b: &[u16; 4]) -> bool {
    for i in (0..4).rev() {
        if a[i] != b[i] {
            return a[i] > b[i];
        }
    }
    false
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