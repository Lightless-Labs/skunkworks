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
    Vended { id: String, price: u16, change: Vec<u16> },
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
    index: HashMap<String, usize>,
    credit: u16,
    selected: Option<String>,
    escrow: Vec<u16>,
    tubes: [u16; 4],
    events: Vec<Event>,
}

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut seen = HashMap::<String, ()>::new();
        let mut index = HashMap::<String, usize>::new();
        for (i, product) in products.iter().enumerate() {
            let valid_id = (1..=8).contains(&product.id.len())
                && product.id.bytes().all(|b| b.is_ascii_lowercase());
            if !valid_id {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }
            if seen.insert(product.id.clone(), ()).is_some() {
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
            index.insert(product.id.clone(), i);
        }

        for (i, count) in tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: [1, 3, 4, 10][i],
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

    fn log(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
    }

    pub fn insert(&mut self, coin: u16) -> Result<(), Error> {
        let denomination_index = match coin {
            1 => 0,
            3 => 1,
            4 => 2,
            10 => 3,
            _ => return Err(Error::BadCoin { coin }),
        };
        let _ = denomination_index;
        if u32::from(self.credit) + u32::from(coin) > 100 {
            return Err(Error::CreditLimit);
        }
        self.credit += coin;
        self.escrow.push(coin);
        self.log(EventKind::Inserted {
            coin,
            credit: self.credit,
        });
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
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
        let id = match self.selected.as_deref() {
            Some(id) => id.to_owned(),
            None => return Err(Error::NoSelection),
        };
        let product_index = self.index[&id];
        let product = &self.products[product_index];
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
        let change = choose_change(amount, &self.tubes)
            .ok_or(Error::NoChange { amount })?;
        let mut proposed = self.tubes;
        for (i, coin) in [1u16, 3, 4, 10].iter().enumerate() {
            let count = change.iter().filter(|c| **c == *coin).count() as u16;
            proposed[i] -= count;
        }
        for coin in &self.escrow {
            let i = match *coin {
                1 => 0,
                3 => 1,
                4 => 2,
                10 => 3,
                _ => unreachable!(),
            };
            proposed[i] += 1;
        }
        for (i, count) in proposed.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: [1, 3, 4, 10][i],
                });
            }
        }

        self.products[product_index].stock -= 1;
        self.tubes = proposed;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.log(EventKind::Vended {
            id: id.clone(),
            price,
            change: change.clone(),
        });
        Ok(Receipt { id, price, change })
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
        products.sort_by(|a, b| a.id.cmp(&b.id));
        products
    }

    pub fn set_price(&mut self, id: &str, price: u16) -> Result<(), Error> {
        let index = match self.index.get(id) {
            Some(index) => *index,
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
        self.log(EventKind::PriceChanged {
            id: id.to_owned(),
            old,
            new: price,
        });
        Ok(())
    }

    pub fn restock(&mut self, id: &str, count: u16) -> Result<(), Error> {
        let index = match self.index.get(id) {
            Some(index) => *index,
            None => return Err(Error::UnknownProduct { id: id.to_owned() }),
        };
        if count == 0 {
            return Err(Error::ZeroRestock);
        }
        let stock = self.products[index].stock;
        if u32::from(stock) + u32::from(count) > 100 {
            return Err(Error::StockLimit { id: id.to_owned() });
        }
        let new_stock = stock + count;
        self.products[index].stock = new_stock;
        self.log(EventKind::Restocked {
            id: id.to_owned(),
            added: count,
            stock: new_stock,
        });
        Ok(())
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }
}

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }
    let coins = [1u16, 3, 4, 10];
    let mut remaining = amount;
    let mut greedy = Vec::new();
    for i in (0..4).rev() {
        let take = tubes[i].min(remaining / coins[i]);
        greedy.extend(std::iter::repeat(coins[i]).take(take as usize));
        remaining -= take * coins[i];
    }
    if remaining == 0 {
        return Some(greedy);
    }

    let mut best: Option<Vec<u16>> = None;
    let mut current = [0u16; 4];

    fn search(
        i: usize,
        remaining: u16,
        coins: &[u16; 4],
        tubes: &[u16; 4],
        current: &mut [u16; 4],
        best: &mut Option<Vec<u16>>,
    ) {
        if i == 4 {
            if remaining != 0 {
                return;
            }
            let mut candidate = Vec::new();
            for j in (0..4).rev() {
                candidate.extend(std::iter::repeat(coins[j]).take(current[j] as usize));
            }
            let replace = match best {
                None => true,
                Some(existing) => {
                    if candidate.len() != existing.len() {
                        candidate.len() < existing.len()
                    } else {
                        candidate > *existing
                    }
                }
            };
            if replace {
                *best = Some(candidate);
            }
            return;
        }

        let max = tubes[i].min(remaining / coins[i]);
        for count in 0..=max {
            current[i] = count;
            search(
                i + 1,
                remaining - count * coins[i],
                coins,
                tubes,
                current,
                best,
            );
        }
    }

    search(0, amount, &coins, tubes, &mut current, &mut best);
    best
}