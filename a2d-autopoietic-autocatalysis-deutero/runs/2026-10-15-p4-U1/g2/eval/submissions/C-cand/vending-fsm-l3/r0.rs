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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub seq: usize,
    pub kind: EventKind,
}

pub struct Machine {
    products: Vec<Product>,
    indices: HashMap<String, usize>,
    credit: u16,
    selected: Option<String>,
    escrow: Vec<u16>,
    tubes: [u16; 4],
    events: Vec<Event>,
}

const DENOMINATIONS: [u16; 4] = [1, 3, 4, 10];

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut indices = HashMap::new();

        for (index, product) in products.iter().enumerate() {
            let valid_id = (1..=8).contains(&product.id.len())
                && product.id.bytes().all(|b| b.is_ascii_lowercase());
            if !valid_id {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }

            if indices.contains_key(&product.id) {
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

            indices.insert(product.id.clone(), index);
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
            indices,
            credit: 0,
            selected: None,
            escrow: Vec::new(),
            tubes,
            events: Vec::new(),
        })
    }

    pub fn state(&self) -> State {
        State {
            credit: self.credit,
            selected: self.selected.clone(),
            escrow: self.escrow.clone(),
            tubes: self.tubes,
        }
    }

    pub fn insert(&mut self, coin: u16) -> Result<(), Error> {
        let denomination_index = DENOMINATIONS
            .iter()
            .position(|&denomination| denomination == coin)
            .ok_or(Error::BadCoin { coin })?;

        let _ = denomination_index;
        let new_credit = self.credit as u32 + coin as u32;
        if new_credit > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit = new_credit as u16;
        self.escrow.push(coin);
        self.log(EventKind::Inserted {
            coin,
            credit: self.credit,
        });
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        if !self.indices.contains_key(id) {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        }
        if self.selected.as_deref() == Some(id) {
            return Err(Error::SameSelection);
        }

        let id = id.to_owned();
        self.selected = Some(id.clone());
        self.log(EventKind::Selected { id });
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let id = self.selected.clone().ok_or(Error::NoSelection)?;
        let index = *self.indices.get(&id).ok_or(Error::NoSelection)?;
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
        let change = choose_change(amount, &self.tubes)
            .ok_or(Error::NoChange { amount })?;

        let mut proposed_tubes = self.tubes;
        for &coin in &change {
            let coin_index = DENOMINATIONS
                .iter()
                .position(|&denomination| denomination == coin)
                .expect("change uses only supported denominations");
            proposed_tubes[coin_index] -= 1;
        }
        for &coin in &self.escrow {
            let coin_index = DENOMINATIONS
                .iter()
                .position(|&denomination| denomination == coin)
                .expect("escrow contains only supported denominations");
            proposed_tubes[coin_index] += 1;
        }

        for (index, count) in proposed_tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[index],
                });
            }
        }

        let change_descending = change;
        self.products[index].stock -= 1;
        self.tubes = proposed_tubes;
        self.credit = 0;
        self.escrow.clear();
        self.selected = None;

        let receipt = Receipt {
            id: id.clone(),
            price,
            change: change_descending.clone(),
        };
        self.log(EventKind::Vended {
            id,
            price,
            change: change_descending,
        });
        Ok(receipt)
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        let coins: Vec<u16> = self.escrow.iter().rev().copied().collect();
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
        let index = *self
            .indices
            .get(id)
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
        let index = *self
            .indices
            .get(id)
            .ok_or_else(|| Error::UnknownProduct { id: id.to_owned() })?;

        if count == 0 {
            return Err(Error::ZeroRestock);
        }

        let new_stock = self.products[index].stock as u32 + count as u32;
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

    fn log(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
    }
}

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    let descending_indices = [3usize, 2, 1, 0];
    let mut greedy_counts = [0u16; 4];
    let mut remaining = amount;

    for &index in &descending_indices {
        let denomination = DENOMINATIONS[index];
        let count = tubes[index].min(remaining / denomination);
        greedy_counts[index] = count;
        remaining -= count * denomination;
    }

    if remaining == 0 {
        return Some(counts_to_descending_coins(&greedy_counts));
    }

    let target = amount as usize;
    let mut best: Option<[u16; 4]> = None;
    let mut best_count = usize::MAX;
    let mut candidate = [0u16; 4];

    fn search(
        position: usize,
        remaining: usize,
        tubes: &[u16; 4],
        candidate: &mut [u16; 4],
        best: &mut Option<[u16; 4]>,
        best_count: &mut usize,
    ) {
        if position == 4 {
            if remaining == 0 {
                let count: usize = candidate.iter().map(|&n| n as usize).sum();
                if count < *best_count
                    || (count == *best_count
                        && best
                            .as_ref()
                            .map(|current| descending_lex_greater(candidate, current))
                            .unwrap_or(true))
                {
                    *best_count = count;
                    *best = Some(*candidate);
                }
            }
            return;
        }

        let index = [3usize, 2, 1, 0][position];
        let denomination = DENOMINATIONS[index] as usize;
        let max = tubes[index].min((remaining / denomination) as u16);

        for count in 0..=max {
            candidate[index] = count;
            search(
                position + 1,
                remaining - count as usize * denomination,
                tubes,
                candidate,
                best,
                best_count,
            );
        }
        candidate[index] = 0;
    }

    fn descending_lex_greater(a: &[u16; 4], b: &[u16; 4]) -> bool {
        for index in [3usize, 2, 1, 0] {
            if a[index] != b[index] {
                return a[index] > b[index];
            }
        }
        false
    }

    search(
        0,
        target,
        tubes,
        &mut candidate,
        &mut best,
        &mut best_count,
    );
    best.map(|counts| counts_to_descending_coins(&counts))
}

fn counts_to_descending_coins(counts: &[u16; 4]) -> Vec<u16> {
    let mut coins = Vec::new();
    for index in [3usize, 2, 1, 0] {
        coins.extend(std::iter::repeat_n(DENOMINATIONS[index], counts[index] as usize));
    }
    coins
}

fn main() {}