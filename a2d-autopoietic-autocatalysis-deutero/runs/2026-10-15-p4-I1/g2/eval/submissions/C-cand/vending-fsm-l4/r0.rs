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
    ReconcileMismatch { coin: u16, expected: u16, found: u16 },
    ValueDrift { before: u64, after: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    Inserted { coin: u16, credit: u16 },
    Selected { id: String },
    Vended { id: String, price: u16, change: Vec<u16> },
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

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut seen = Vec::<String>::new();
        for product in &products {
            if product.id.is_empty()
                || product.id.len() > 8
                || !product.id.bytes().all(|b| b.is_ascii_lowercase())
            {
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

    fn log(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
    }

    fn product_index(&self, id: &str) -> Option<usize> {
        self.products.iter().position(|p| p.id == id)
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
        if self.product_index(id).is_none() {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        }
        if self.selected.as_deref() == Some(id) {
            return Err(Error::SameSelection);
        }

        self.selected = Some(id.to_owned());
        self.log(EventKind::Selected { id: id.to_owned() });
        Ok(())
    }

    fn choose_change(amount: u16, tubes: [u16; 4]) -> Option<Vec<u16>> {
        if amount == 0 {
            return Some(Vec::new());
        }

        let mut remaining = amount;
        let mut greedy_counts = [0u16; 4];
        for i in (0..4).rev() {
            let take = tubes[i].min(remaining / DENOMS[i]);
            greedy_counts[i] = take;
            remaining -= take * DENOMS[i];
        }

        if remaining == 0 {
            let mut coins = Vec::new();
            for i in (0..4).rev() {
                for _ in 0..greedy_counts[i] {
                    coins.push(DENOMS[i]);
                }
            }
            return Some(coins);
        }

        fn search(
            i: usize,
            remaining: u16,
            tubes: &[u16; 4],
            counts: &mut [u16; 4],
            best: &mut Option<[u16; 4]>,
        ) {
            if i == 4 {
                if remaining != 0 {
                    return;
                }
                let total: u32 = counts.iter().map(|&n| u32::from(n)).sum();
                let replace = match best {
                    None => true,
                    Some(current) => {
                        let best_total: u32 =
                            current.iter().map(|&n| u32::from(n)).sum();
                        total < best_total
                            || (total == best_total
                                && (0..4).rev().find_map(|j| {
                                    if counts[j] != current[j] {
                                        Some(counts[j] > current[j])
                                    } else {
                                        None
                                    }
                                }) == Some(true))
                    }
                };
                if replace {
                    *best = Some(*counts);
                }
                return;
            }

            let denomination = DENOMS[i];
            let max_take = tubes[i].min(remaining / denomination);
            for take in 0..=max_take {
                counts[i] = take;
                search(
                    i + 1,
                    remaining - take * denomination,
                    tubes,
                    counts,
                    best,
                );
            }
            counts[i] = 0;
        }

        let mut best = None;
        let mut counts = [0u16; 4];
        search(0, amount, &tubes, &mut counts, &mut best);

        best.map(|counts| {
            let mut coins = Vec::new();
            for i in (0..4).rev() {
                for _ in 0..counts[i] {
                    coins.push(DENOMS[i]);
                }
            }
            coins
        })
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }
        let id = match self.selected.as_ref() {
            Some(id) => id.clone(),
            None => return Err(Error::NoSelection),
        };
        let index = self.product_index(&id).expect("selected product exists");
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
        let coins = Self::choose_change(amount, self.tubes)
            .ok_or(Error::NoChange { amount })?;

        let mut proposed = self.tubes;
        for coin in &coins {
            let i = DENOMS.iter().position(|&d| d == *coin).unwrap();
            proposed[i] -= 1;
        }
        for coin in &self.escrow {
            let i = DENOMS.iter().position(|&d| d == *coin).unwrap();
            proposed[i] += 1;
        }
        for i in 0..4 {
            if proposed[i] > 100 {
                return Err(Error::TubeLimit { coin: DENOMS[i] });
            }
        }

        let change_value: u64 = coins.iter().map(|&c| u64::from(c)).sum();
        self.tubes = proposed;
        self.products[index].stock -= 1;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.sales = self.sales.saturating_add(u64::from(price));
        self.change = self.change.saturating_add(change_value);
        self.log(EventKind::Vended {
            id: id.clone(),
            price,
            change: coins.clone(),
        });

        Ok(Receipt {
            id,
            price,
            change: coins,
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
        products.sort_by(|a, b| a.id.cmp(&b.id));
        products
    }

    pub fn set_price(&mut self, id: &str, price: u16) -> Result<(), Error> {
        if !self.maintenance {
            return Err(Error::MaintenanceRequired);
        }
        let index = self
            .product_index(id)
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
            .product_index(id)
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

        let before_value: u64 = self
            .tubes
            .iter()
            .zip(DENOMS.iter())
            .map(|(&count, &coin)| u64::from(count) * u64::from(coin))
            .sum();
        let after_value: u64 = actual
            .iter()
            .zip(DENOMS.iter())
            .map(|(&count, &coin)| u64::from(count) * u64::from(coin))
            .sum();

        if before_value.abs_diff(after_value) > 20 {
            return Err(Error::ValueDrift {
                before: before_value,
                after: after_value,
            });
        }

        let delta = i128::from(after_value) - i128::from(before_value);
        let new_adjustment =
            (i128::from(self.adjustment) + delta).clamp(i128::from(i64::MIN), i128::from(i64::MAX))
                as i64;

        let before = self.tubes;
        self.tubes = actual;
        self.adjustment = new_adjustment;
        self.log(EventKind::Reconciled {
            before,
            after: actual,
        });
        Ok(())
    }
}

fn main() {}