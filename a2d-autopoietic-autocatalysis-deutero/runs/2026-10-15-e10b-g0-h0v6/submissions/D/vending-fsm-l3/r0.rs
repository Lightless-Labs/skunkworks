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

        let mut seen = HashMap::<String, ()>::new();
        for product in &products {
            if !valid_id(&product.id) {
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
        let index = denomination_index(coin).ok_or(Error::BadCoin { coin })?;
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
        let _ = index;
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
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

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let id = self.selected.clone().ok_or(Error::NoSelection)?;
        let index = self
            .product_index(&id)
            .expect("selected product remains in the fixed catalog");
        let product = &self.products[index];

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

        let mut proposed_tubes = self.tubes;
        for (i, count) in change.iter().enumerate() {
            proposed_tubes[i] -= count;
        }
        for &coin in &self.escrow {
            let i = denomination_index(coin).expect("escrow contains accepted coins");
            proposed_tubes[i] += 1;
        }
        for (i, count) in proposed_tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
                });
            }
        }

        self.products[index].stock -= 1;
        self.tubes = proposed_tubes;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        let receipt = Receipt {
            id: id.clone(),
            price,
            change: coins_from_counts(&change),
        };
        self.log(EventKind::Vended {
            id,
            price,
            change: receipt.change.clone(),
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
        let index = self
            .product_index(id)
            .ok_or_else(|| Error::UnknownProduct { id: id.to_owned() })?;
        if count == 0 {
            return Err(Error::ZeroRestock);
        }
        let stock = self.products[index].stock as u32 + count as u32;
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

    fn product_index(&self, id: &str) -> Option<usize> {
        self.products.iter().position(|product| product.id == id)
    }

    fn log(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
    }
}

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase())
}

fn denomination_index(coin: u16) -> Option<usize> {
    DENOMINATIONS.iter().position(|&d| d == coin)
}

fn coins_from_counts(counts: &[u16; 4]) -> Vec<u16> {
    let mut coins = Vec::new();
    for index in (0..4).rev() {
        for _ in 0..counts[index] {
            coins.push(DENOMINATIONS[index]);
        }
    }
    coins
}

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<[u16; 4]> {
    if amount == 0 {
        return Some([0; 4]);
    }

    let mut greedy = [0u16; 4];
    let mut remaining = amount;
    for index in (0..4).rev() {
        let take = tubes[index].min(remaining / DENOMINATIONS[index]);
        greedy[index] = take;
        remaining -= take * DENOMINATIONS[index];
    }
    if remaining == 0 {
        return Some(greedy);
    }

    let mut best: Option<[u16; 4]> = None;
    let mut best_total = u32::MAX;
    search_change(
        3,
        amount as u32,
        tubes,
        &mut [0; 4],
        &mut best,
        &mut best_total,
    );
    best
}

fn search_change(
    index: isize,
    remaining: u32,
    tubes: &[u16; 4],
    current: &mut [u16; 4],
    best: &mut Option<[u16; 4]>,
    best_total: &mut u32,
) {
    if index < 0 {
        if remaining == 0 {
            let total: u32 = current.iter().map(|&n| n as u32).sum();
            let is_better = match best {
                None => true,
                Some(existing) if total < *best_total => true,
                Some(existing) if total == *best_total => {
                    (0..4).rev().find_map(|i| {
                        if current[i] != existing[i] {
                            Some(current[i] > existing[i])
                        } else {
                            None
                        }
                    }).unwrap_or(false)
                }
                _ => false,
            };
            if is_better {
                *best = Some(*current);
                *best_total = total;
            }
        }
        return;
    }

    let i = index as usize;
    let denomination = DENOMINATIONS[i] as u32;
    let max_take = tubes[i].min((remaining / denomination) as u16);
    for take in 0..=max_take {
        let next_remaining = remaining - take as u32 * denomination;
        let next_total: u32 = current.iter().map(|&n| n as u32).sum::<u32>() + take as u32;
        if next_total <= *best_total {
            current[i] = take;
            search_change(
                index - 1,
                next_remaining,
                tubes,
                current,
                best,
                best_total,
            );
        }
    }
    current[i] = 0;
}