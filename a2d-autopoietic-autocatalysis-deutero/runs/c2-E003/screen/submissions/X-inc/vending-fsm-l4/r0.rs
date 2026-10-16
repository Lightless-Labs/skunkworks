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
    Reconciled {
        before: [u16; 4],
        after: [u16; 4],
    },
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

        let mut seen = Vec::<String>::new();
        for product in &products {
            if !valid_id(&product.id) {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }
            if seen.iter().any(|id| id == &product.id) {
                return Err(Error::DuplicateId {
                    id: product.id.clone(),
                });
            }
            seen.push(product.id.clone());

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
        let index = denomination_index(coin).ok_or(Error::BadCoin { coin })?;
        let new_credit = self.credit as u32 + coin as u32;
        if new_credit > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit = new_credit as u16;
        self.escrow.push(coin);
        self.inserted = self.inserted.saturating_add(coin as u64);
        self.push_event(EventKind::Inserted {
            coin,
            credit: self.credit,
        });
        let _ = index;
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }
        if self.find_product(id).is_none() {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        }
        if self.selected.as_deref() == Some(id) {
            return Err(Error::SameSelection);
        }

        self.selected = Some(id.to_owned());
        self.push_event(EventKind::Selected { id: id.to_owned() });
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }
        let id = self.selected.clone().ok_or(Error::NoSelection)?;
        let product_index = self
            .find_product(&id)
            .expect("selected products are always in the catalog");
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
        let change = choose_change(amount, self.tubes)?;
        let mut proposed = self.tubes;

        for coin in &change {
            let index = denomination_index(*coin).expect("change uses valid denominations");
            proposed[index] -= 1;
        }
        for coin in &self.escrow {
            let index = denomination_index(*coin).expect("escrow contains valid denominations");
            proposed[index] = proposed[index].saturating_add(1);
        }
        for (index, count) in proposed.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[index],
                });
            }
        }

        self.tubes = proposed;
        self.products[product_index].stock -= 1;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        let change_value = change.iter().map(|coin| *coin as u64).sum::<u64>();
        self.sales = self.sales.saturating_add(price as u64);
        self.change = self.change.saturating_add(change_value);
        self.push_event(EventKind::Vended {
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
        self.selected = None;
        self.escrow.clear();
        self.push_event(EventKind::Refunded {
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
            .find_product(id)
            .ok_or_else(|| Error::UnknownProduct { id: id.to_owned() })?;
        if self.credit != 0 {
            return Err(Error::Busy);
        }
        if !(1..=100).contains(&price) {
            return Err(Error::BadPrice { id: id.to_owned() });
        }

        let old = self.products[index].price;
        self.products[index].price = price;
        self.push_event(EventKind::PriceChanged {
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
            .find_product(id)
            .ok_or_else(|| Error::UnknownProduct { id: id.to_owned() })?;
        if count == 0 {
            return Err(Error::ZeroRestock);
        }

        let stock = self.products[index].stock as u32 + count as u32;
        if stock > 100 {
            return Err(Error::StockLimit { id: id.to_owned() });
        }

        self.products[index].stock = stock as u16;
        self.push_event(EventKind::Restocked {
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
        self.push_event(EventKind::Maintenance { enabled });
        Ok(())
    }

    pub fn audit(&self) -> Audit {
        let tube_value = tube_value(self.tubes);
        let cash = tube_value.saturating_add(self.credit as u64);
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
        let difference = before_value.abs_diff(after_value);
        if difference > 20 {
            return Err(Error::ValueDrift {
                before: before_value,
                after: after_value,
            });
        }

        let signed_delta = after_value as i128 - before_value as i128;
        self.adjustment = saturating_add_i64(self.adjustment, signed_delta);
        let before = self.tubes;
        self.tubes = actual;
        self.push_event(EventKind::Reconciled {
            before,
            after: actual,
        });
        Ok(())
    }

    fn find_product(&self, id: &str) -> Option<usize> {
        self.products.iter().position(|product| product.id == id)
    }

    fn push_event(&mut self, kind: EventKind) {
        self.events.push(Event {
            seq: self.events.len(),
            kind,
        });
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 8 && id.bytes().all(|b| b.is_ascii_lowercase())
}

fn denomination_index(coin: u16) -> Option<usize> {
    match coin {
        1 => Some(0),
        3 => Some(1),
        4 => Some(2),
        10 => Some(3),
        _ => None,
    }
}

fn tube_value(tubes: [u16; 4]) -> u64 {
    tubes
        .iter()
        .zip(DENOMINATIONS)
        .map(|(count, coin)| *count as u64 * coin as u64)
        .sum()
}

fn choose_change(amount: u16, tubes: [u16; 4]) -> Result<Vec<u16>, Error> {
    if amount == 0 {
        return Ok(Vec::new());
    }

    let mut remaining = amount;
    let mut counts = [0u16; 4];
    for index in (0..4).rev() {
        let coin = DENOMINATIONS[index];
        let take = tubes[index].min(remaining / coin);
        counts[index] = take;
        remaining -= take * coin;
    }
    if remaining == 0 {
        return Ok(counts_to_descending(counts));
    }

    let mut best: Option<[u16; 4]> = None;
    search_combinations(
        3,
        amount as i32,
        0,
        [0; 4],
        tubes,
        &mut best,
    );

    best.map(counts_to_descending)
        .ok_or(Error::NoChange { amount })
}

fn search_combinations(
    index: i32,
    remaining: i32,
    used: u16,
    counts: [u16; 4],
    available: [u16; 4],
    best: &mut Option<[u16; 4]>,
) {
    if remaining == 0 {
        let is_better = match best {
            None => true,
            Some(current) => {
                let current_used: u16 = current.iter().copied().sum();
                if used != current_used {
                    used < current_used
                } else {
                    lex_greater_descending(&counts, current)
                }
            }
        };
        if is_better {
            *best = Some(counts);
        }
        return;
    }
    if index < 0 || remaining < 0 {
        return;
    }
    if let Some(current) = best {
        let current_used: u16 = current.iter().copied().sum();
        if used >= current_used {
            return;
        }
    }

    let i = index as usize;
    let coin = DENOMINATIONS[i] as i32;
    let max_take = available[i].min((remaining / coin) as u16);
    for take in (0..=max_take).rev() {
        let next_remaining = remaining - take as i32 * coin;
        let mut next_counts = counts;
        next_counts[i] = take;
        search_combinations(
            index - 1,
            next_remaining,
            used + take,
            next_counts,
            available,
            best,
        );
    }
}

fn lex_greater_descending(left: &[u16; 4], right: &[u16; 4]) -> bool {
    for index in (0..4).rev() {
        if left[index] != right[index] {
            return left[index] > right[index];
        }
    }
    false
}

fn counts_to_descending(counts: [u16; 4]) -> Vec<u16> {
    let total: usize = counts.iter().map(|count| *count as usize).sum();
    let mut coins = Vec::with_capacity(total);
    for index in (0..4).rev() {
        coins.extend(std::iter::repeat_n(
            DENOMINATIONS[index],
            counts[index] as usize,
        ));
    }
    coins
}

fn saturating_add_i64(value: i64, delta: i128) -> i64 {
    let sum = value as i128 + delta;
    sum.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}