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

        let mut seen: Vec<&str> = Vec::new();
        for product in &products {
            let valid_id = !product.id.is_empty()
                && product.id.len() <= 8
                && product.id.bytes().all(|b| b.is_ascii_lowercase());
            if !valid_id {
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

        for (index, count) in tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMS[index],
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
        let index = match DENOMS.iter().position(|d| *d == coin) {
            Some(index) => index,
            None => return Err(Error::BadCoin { coin }),
        };
        let new_credit = u32::from(self.credit) + u32::from(coin);
        if new_credit > 100 {
            return Err(Error::CreditLimit);
        }

        let new_credit = new_credit as u16;
        self.escrow.push(coin);
        self.credit = new_credit;
        self.inserted = self.inserted.saturating_add(u64::from(coin));
        let _ = index;
        self.push_event(EventKind::Inserted {
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
        self.push_event(EventKind::Selected { id: id.to_owned() });
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }
        let id = match self.selected.as_deref() {
            Some(id) => id.to_owned(),
            None => return Err(Error::NoSelection),
        };
        let product_index = self
            .products
            .iter()
            .position(|product| product.id == id)
            .expect("selected product is always in the catalog");

        if self.products[product_index].stock == 0 {
            return Err(Error::SoldOut { id });
        }
        let price = self.products[product_index].price;
        if self.credit < price {
            return Err(Error::Underpaid {
                price,
                credit: self.credit,
            });
        }

        let amount = self.credit - price;
        let chosen = Self::choose_change(amount, self.tubes)
            .ok_or(Error::NoChange { amount })?;

        let mut proposed = self.tubes;
        for index in 0..4 {
            proposed[index] -= chosen[index];
        }
        for coin in &self.escrow {
            let index = DENOMS
                .iter()
                .position(|denom| denom == coin)
                .expect("escrow contains only accepted denominations");
            proposed[index] += 1;
        }
        for index in 0..4 {
            if proposed[index] > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMS[index],
                });
            }
        }

        let mut change_coins = Vec::new();
        for index in (0..4).rev() {
            for _ in 0..chosen[index] {
                change_coins.push(DENOMS[index]);
            }
        }

        self.tubes = proposed;
        self.products[product_index].stock -= 1;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.sales = self.sales.saturating_add(u64::from(price));
        self.change = self.change.saturating_add(u64::from(amount));
        self.push_event(EventKind::Vended {
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
        let refunded = u64::from(self.credit);
        self.escrow.clear();
        self.credit = 0;
        self.selected = None;
        self.refunded = self.refunded.saturating_add(refunded);
        self.push_event(EventKind::Refunded {
            coins: coins.clone(),
        });
        Ok(coins)
    }

    pub fn products(&self) -> Vec<Product> {
        let mut result = self.products.clone();
        result.sort_by(|a, b| a.id.as_bytes().cmp(b.id.as_bytes()));
        result
    }

    pub fn set_price(&mut self, id: &str, price: u16) -> Result<(), Error> {
        if !self.maintenance {
            return Err(Error::MaintenanceRequired);
        }
        let index = match self.products.iter().position(|product| product.id == id) {
            Some(index) => index,
            None => return Err(Error::UnknownProduct { id: id.to_owned() }),
        };
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
        let index = match self.products.iter().position(|product| product.id == id) {
            Some(index) => index,
            None => return Err(Error::UnknownProduct { id: id.to_owned() }),
        };
        if count == 0 {
            return Err(Error::ZeroRestock);
        }
        let total = u32::from(self.products[index].stock) + u32::from(count);
        if total > 100 {
            return Err(Error::StockLimit { id: id.to_owned() });
        }

        self.products[index].stock = total as u16;
        self.push_event(EventKind::Restocked {
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
        self.push_event(EventKind::Maintenance { enabled });
        Ok(())
    }

    pub fn audit(&self) -> Audit {
        let tubes_value = self.tube_value(self.tubes);
        Audit {
            inserted: self.inserted,
            refunded: self.refunded,
            sales: self.sales,
            change: self.change,
            adjustment: self.adjustment,
            cash: tubes_value.saturating_add(u64::from(self.credit)),
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
                    coin: DENOMS[index],
                    expected: expected[index],
                    found: self.tubes[index],
                });
            }
        }
        for index in 0..4 {
            if actual[index] > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMS[index],
                });
            }
        }

        let before_value = self.tube_value(self.tubes);
        let after_value = self.tube_value(actual);
        if before_value.abs_diff(after_value) > 20 {
            return Err(Error::ValueDrift {
                before: before_value,
                after: after_value,
            });
        }

        let delta = i128::from(after_value) - i128::from(before_value);
        self.adjustment = (i128::from(self.adjustment) + delta)
            .clamp(i128::from(i64::MIN), i128::from(i64::MAX))
            as i64;
        let before = self.tubes;
        self.tubes = actual;
        self.push_event(EventKind::Reconciled {
            before,
            after: actual,
        });
        Ok(())
    }
}

impl Machine {
    fn push_event(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
    }

    fn tube_value(&self, tubes: [u16; 4]) -> u64 {
        tubes
            .iter()
            .zip(DENOMS)
            .map(|(count, denom)| u64::from(*count) * u64::from(denom))
            .sum()
    }

    fn choose_change(amount: u16, tubes: [u16; 4]) -> Option<[u16; 4]> {
        if amount == 0 {
            return Some([0; 4]);
        }

        let mut greedy = [0u16; 4];
        let mut remainder = amount;
        for index in (0..4).rev() {
            let denomination = DENOMS[index];
            let take = tubes[index].min(remainder / denomination);
            greedy[index] = take;
            remainder -= take * denomination;
        }
        if remainder == 0 {
            return Some(greedy);
        }

        let target = usize::from(amount);
        let mut best: Vec<Option<[u16; 4]>> = vec![None; target + 1];
        best[0] = Some([0; 4]);

        for index in 0..4 {
            let denomination = usize::from(DENOMS[index]);
            for _ in 0..tubes[index] {
                for value in (denomination..=target).rev() {
                    if let Some(previous) = best[value - denomination] {
                        let mut candidate = previous;
                        candidate[index] += 1;
                        let replace = match best[value] {
                            None => true,
                            Some(current) => {
                                let candidate_count: u32 =
                                    candidate.iter().map(|x| u32::from(*x)).sum();
                                let current_count: u32 =
                                    current.iter().map(|x| u32::from(*x)).sum();
                                if candidate_count < current_count {
                                    true
                                } else if candidate_count > current_count {
                                    false
                                } else {
                                    Self::descending_lex_greater(candidate, current)
                                }
                            }
                        };
                        if replace {
                            best[value] = Some(candidate);
                        }
                    }
                }
            }
        }
        best[target]
    }

    fn descending_lex_greater(left: [u16; 4], right: [u16; 4]) -> bool {
        for index in (0..4).rev() {
            if left[index] != right[index] {
                return left[index] > right[index];
            }
        }
        false
    }
}