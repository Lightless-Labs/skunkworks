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

        let mut seen = std::collections::HashSet::new();
        for product in &products {
            let valid_id = !product.id.is_empty()
                && product.id.len() <= 8
                && product.id.bytes().all(|b| b.is_ascii_lowercase());
            if !valid_id {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }
            if !seen.insert(product.id.clone()) {
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

        for (i, count) in tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: [1, 3, 4, 10][i],
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
        let Some(index) = denomination_index(coin) else {
            return Err(Error::BadCoin { coin });
        };
        let _ = index;
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
        let Some(index) = self.product_index(id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if self.selected.as_deref() == Some(id) {
            return Err(Error::SameSelection);
        }

        let selected_id = self.products[index].id.clone();
        self.selected = Some(selected_id.clone());
        self.log(EventKind::Selected { id: selected_id });
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let Some(selected_id) = self.selected.clone() else {
            return Err(Error::NoSelection);
        };
        let index = self
            .product_index(&selected_id)
            .expect("selected product is in the fixed catalog");
        let product = &self.products[index];

        if product.stock == 0 {
            return Err(Error::SoldOut {
                id: selected_id,
            });
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

        let mut proposed_tubes = self.tubes;
        for (i, count) in change_counts(&change).iter().enumerate() {
            proposed_tubes[i] -= count;
        }
        for coin in &self.escrow {
            let i = denomination_index(*coin).expect("escrow contains accepted coins");
            proposed_tubes[i] += 1;
        }
        for (i, count) in proposed_tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: [1, 3, 4, 10][i],
                });
            }
        }

        self.products[index].stock -= 1;
        self.tubes = proposed_tubes;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        let receipt = Receipt {
            id: selected_id.clone(),
            price,
            change: change.clone(),
        };
        self.log(EventKind::Vended {
            id: selected_id,
            price,
            change,
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
        let Some(index) = self.product_index(id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
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
        let Some(index) = self.product_index(id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
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

    fn product_index(&self, id: &str) -> Option<usize> {
        self.products.iter().position(|product| product.id == id)
    }

    fn log(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
    }
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

fn change_counts(coins: &[u16]) -> [u16; 4] {
    let mut counts = [0; 4];
    for &coin in coins {
        if let Some(index) = denomination_index(coin) {
            counts[index] += 1;
        }
    }
    counts
}

fn choose_change(amount: u16, tubes: [u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    let denominations = [10u16, 4, 3, 1];
    let mut remaining = amount;
    let mut greedy = Vec::new();

    for &coin in &denominations {
        let index = denomination_index(coin).expect("known denomination");
        let take = tubes[index].min(remaining / coin);
        for _ in 0..take {
            greedy.push(coin);
        }
        remaining -= take * coin;
    }

    if remaining == 0 {
        return Some(greedy);
    }

    let mut best: Option<Vec<u16>> = None;
    let mut current = Vec::new();
    search_change(
        0,
        amount,
        &denominations,
        &tubes,
        &mut current,
        &mut best,
    );
    best
}

fn search_change(
    denomination_pos: usize,
    remaining: u16,
    denominations: &[u16; 4],
    tubes: &[u16; 4],
    current: &mut Vec<u16>,
    best: &mut Option<Vec<u16>>,
) {
    if denomination_pos == denominations.len() {
        if remaining == 0 {
            let replace = match best {
                None => true,
                Some(existing) => {
                    current.len() < existing.len()
                        || (current.len() == existing.len()
                            && current.as_slice() > existing.as_slice())
                }
            };
            if replace {
                *best = Some(current.clone());
            }
        }
        return;
    }

    let coin = denominations[denomination_pos];
    let index = denomination_index(coin).expect("known denomination");
    let max = tubes[index].min(remaining / coin);

    for count in (0..=max).rev() {
        let used = count * coin;
        for _ in 0..count {
            current.push(coin);
        }
        search_change(
            denomination_pos + 1,
            remaining - used,
            denominations,
            tubes,
            current,
            best,
        );
        for _ in 0..count {
            current.pop();
        }
    }
}

fn main() {}