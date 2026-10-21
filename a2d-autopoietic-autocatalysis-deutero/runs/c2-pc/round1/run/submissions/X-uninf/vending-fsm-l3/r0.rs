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
    index: HashMap<String, usize>,
    credit: u16,
    selected: Option<usize>,
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

        let mut index = HashMap::new();
        for (position, product) in products.iter().enumerate() {
            let valid_id = !product.id.is_empty()
                && product.id.len() <= 8
                && product.id.bytes().all(|byte| byte.is_ascii_lowercase());
            if !valid_id {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }
            if index.contains_key(&product.id) {
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
            index.insert(product.id.clone(), position);
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
            selected: self.selected.map(|i| self.products[i].id.clone()),
            escrow: self.escrow.clone(),
            tubes: self.tubes,
        }
    }

    pub fn insert(&mut self, coin: u16) -> Result<(), Error> {
        let denomination_index = DENOMINATIONS.iter().position(|&d| d == coin);
        if denomination_index.is_none() {
            return Err(Error::BadCoin { coin });
        }
        if u32::from(self.credit) + u32::from(coin) > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit += coin;
        self.escrow.push(coin);
        self.push_event(EventKind::Inserted {
            coin,
            credit: self.credit,
        });
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        let Some(&position) = self.index.get(id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if self.selected == Some(position) {
            return Err(Error::SameSelection);
        }

        self.selected = Some(position);
        self.push_event(EventKind::Selected {
            id: id.to_owned(),
        });
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let Some(position) = self.selected else {
            return Err(Error::NoSelection);
        };
        let product = &self.products[position];
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

        let id = product.id.clone();
        let price = product.price;
        let amount = self.credit - price;
        let change = match choose_change(amount, self.tubes) {
            Some(coins) => coins,
            None => return Err(Error::NoChange { amount }),
        };

        let mut proposed_tubes = self.tubes;
        for &coin in &change {
            let i = DENOMINATIONS.iter().position(|&d| d == coin).unwrap();
            proposed_tubes[i] -= 1;
        }
        for &coin in &self.escrow {
            let i = DENOMINATIONS.iter().position(|&d| d == coin).unwrap();
            proposed_tubes[i] += 1;
        }
        for (i, &count) in proposed_tubes.iter().enumerate() {
            if count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
                });
            }
        }

        self.products[position].stock -= 1;
        self.tubes = proposed_tubes;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.push_event(EventKind::Vended {
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
        let Some(&position) = self.index.get(id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if self.credit != 0 {
            return Err(Error::Busy);
        }
        if !(1..=100).contains(&price) {
            return Err(Error::BadPrice { id: id.to_owned() });
        }

        let old = self.products[position].price;
        self.products[position].price = price;
        self.push_event(EventKind::PriceChanged {
            id: id.to_owned(),
            old,
            new: price,
        });
        Ok(())
    }

    pub fn restock(&mut self, id: &str, count: u16) -> Result<(), Error> {
        let Some(&position) = self.index.get(id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if count == 0 {
            return Err(Error::ZeroRestock);
        }
        let stock = self.products[position].stock;
        if u32::from(stock) + u32::from(count) > 100 {
            return Err(Error::StockLimit { id: id.to_owned() });
        }

        let new_stock = stock + count;
        self.products[position].stock = new_stock;
        self.push_event(EventKind::Restocked {
            id: id.to_owned(),
            added: count,
            stock: new_stock,
        });
        Ok(())
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    fn push_event(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
    }
}

fn choose_change(amount: u16, tubes: [u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    let mut greedy = Vec::new();
    let mut remaining = amount;
    for i in (0..4).rev() {
        let coin = DENOMINATIONS[i];
        let take = tubes[i].min(remaining / coin);
        greedy.extend(std::iter::repeat(coin).take(usize::from(take)));
        remaining -= take * coin;
    }
    if remaining == 0 {
        return Some(greedy);
    }

    let mut best: Option<[u16; 4]> = None;
    for n10 in 0..=tubes[3].min(amount / 10) {
        let left10 = amount - n10 * 10;
        for n4 in 0..=tubes[2].min(left10 / 4) {
            let left4 = left10 - n4 * 4;
            for n3 in 0..=tubes[1].min(left4 / 3) {
                let left3 = left4 - n3 * 3;
                if left3 > tubes[0] {
                    continue;
                }
                let candidate = [left3, n3, n4, n10];
                let better = match best {
                    None => true,
                    Some(current) => {
                        let candidate_total: u32 = candidate.iter().map(|&x| u32::from(x)).sum();
                        let current_total: u32 = current.iter().map(|&x| u32::from(x)).sum();
                        candidate_total < current_total
                            || (candidate_total == current_total
                                && (3..=0).rev().any(|_| false))
                            || (candidate_total == current_total
                                && descending_lex_greater(candidate, current))
                    }
                };
                if better {
                    best = Some(candidate);
                }
            }
        }
    }

    let counts = best?;
    let mut coins = Vec::new();
    for i in (0..4).rev() {
        coins.extend(std::iter::repeat(DENOMINATIONS[i]).take(usize::from(counts[i])));
    }
    Some(coins)
}

fn descending_lex_greater(left: [u16; 4], right: [u16; 4]) -> bool {
    for i in (0..4).rev() {
        if left[i] != right[i] {
            return left[i] > right[i];
        }
    }
    false
}