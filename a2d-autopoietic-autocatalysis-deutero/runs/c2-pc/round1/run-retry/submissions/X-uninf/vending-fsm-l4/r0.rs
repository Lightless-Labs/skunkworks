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

fn tube_value(tubes: &[u16; 4]) -> u64 {
    tubes
        .iter()
        .zip(DENOMINATIONS)
        .map(|(&count, coin)| u64::from(count) * u64::from(coin))
        .sum()
}

fn amount_value(coins: &[u16]) -> u64 {
    coins.iter().map(|&coin| u64::from(coin)).sum()
}

fn exact_change(tubes: &[u16; 4], amount: u16) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    // Greedy is mandatory whenever it produces an exact result.
    let mut greedy_remaining = amount;
    let mut greedy_counts = [0u16; 4];
    for i in (0..4).rev() {
        let coin = DENOMINATIONS[i];
        let take = tubes[i].min(greedy_remaining / coin);
        greedy_counts[i] = take;
        greedy_remaining -= take * coin;
    }

    if greedy_remaining == 0 {
        return Some(counts_to_descending_coins(greedy_counts));
    }

    // Enumerate bounded combinations, retaining minimum coin count and then
    // the lexicographically greatest descending coin vector.
    let mut best: Option<([u16; 4], u32)> = None;

    for n10 in 0..=tubes[3].min(amount / 10) {
        let v10 = n10 * 10;
        if v10 > amount {
            break;
        }
        for n4 in 0..=tubes[2].min((amount - v10) / 4) {
            let v4 = n4 * 4;
            if v10 + v4 > amount {
                break;
            }
            for n3 in 0..=tubes[1].min((amount - v10 - v4) / 3) {
                let v3 = n3 * 3;
                if v10 + v4 + v3 > amount {
                    break;
                }
                let remainder = amount - v10 - v4 - v3;
                if remainder > tubes[0] {
                    continue;
                }
                let counts = [remainder, n3, n4, n10];
                let total = u32::from(remainder)
                    + u32::from(n3)
                    + u32::from(n4)
                    + u32::from(n10);

                let replace = match best {
                    None => true,
                    Some((best_counts, best_total)) => {
                        total < best_total
                            || (total == best_total
                                && descending_counts_lex_greater(counts, best_counts))
                    }
                };
                if replace {
                    best = Some((counts, total));
                }
            }
        }
    }

    best.map(|(counts, _)| counts_to_descending_coins(counts))
}

fn counts_to_descending_coins(counts: [u16; 4]) -> Vec<u16> {
    let mut coins = Vec::new();
    for i in (0..4).rev() {
        coins.extend(std::iter::repeat_n(DENOMINATIONS[i], usize::from(counts[i])));
    }
    coins
}

fn descending_counts_lex_greater(a: [u16; 4], b: [u16; 4]) -> bool {
    for i in (0..4).rev() {
        if a[i] != b[i] {
            return a[i] > b[i];
        }
    }
    false
}

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
            if seen.insert(product.id.as_str(), ()).is_some() {
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

        for (i, &count) in tubes.iter().enumerate() {
            if count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
                });
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

        let id = match self.selected.as_ref() {
            Some(id) => id.clone(),
            None => return Err(Error::NoSelection),
        };
        let index = self
            .products
            .iter()
            .position(|product| product.id == id)
            .expect("selected products are always in the catalog");

        if self.products[index].stock == 0 {
            return Err(Error::SoldOut { id });
        }

        let price = self.products[index].price;
        if self.credit < price {
            return Err(Error::Underpaid {
                price,
                credit: self.credit,
            });
        }

        let amount = self.credit - price;
        let change_coins = exact_change(&self.tubes, amount)
            .ok_or(Error::NoChange { amount })?;

        let mut proposed_tubes = self.tubes;
        for &coin in &change_coins {
            let i = coin_index(coin).expect("change only contains valid denominations");
            proposed_tubes[i] -= 1;
        }
        for &coin in &self.escrow {
            let i = coin_index(coin).expect("escrow only contains valid denominations");
            proposed_tubes[i] += 1;
        }
        for (i, &count) in proposed_tubes.iter().enumerate() {
            if count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
                });
            }
        }

        self.tubes = proposed_tubes;
        self.products[index].stock -= 1;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.sales = self.sales.saturating_add(u64::from(price));
        let change_value = amount_value(&change_coins);
        self.change = self.change.saturating_add(change_value);
        self.log(EventKind::Vended {
            id: id.clone(),
            price,
            change: change_coins.clone(),
        });

        Ok(Receipt {
            id,
            price,
            change: change_coins,
        })
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
        let total = u32::from(self.products[index].stock) + u32::from(count);
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
        Audit {
            inserted: self.inserted,
            refunded: self.refunded,
            sales: self.sales,
            change: self.change,
            adjustment: self.adjustment,
            cash: tube_value(&self.tubes) + u64::from(self.credit),
        }
    }

    pub fn reconcile(&mut self, expected: [u16; 4], actual: [u16; 4]) -> Result<(), Error> {
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

        let before_value = tube_value(&self.tubes);
        let after_value = tube_value(&actual);
        if before_value.abs_diff(after_value) > 20 {
            return Err(Error::ValueDrift {
                before: before_value,
                after: after_value,
            });
        }

        self.tubes = actual;
        let difference = i128::from(after_value) - i128::from(before_value);
        let adjustment = i128::from(self.adjustment) + difference;
        self.adjustment = adjustment.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
        self.log(EventKind::Reconciled {
            before: expected,
            after: actual,
        });
        Ok(())
    }

    fn log(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
    }
}