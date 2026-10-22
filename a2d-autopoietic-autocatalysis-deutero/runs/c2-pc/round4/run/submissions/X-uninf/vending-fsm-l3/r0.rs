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

const DENOMINATIONS: [u16; 4] = [1, 3, 4, 10];

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut seen = std::collections::HashSet::new();
        for product in &products {
            if product.id.is_empty()
                || product.id.len() > 8
                || !product.id.bytes().all(|b| b.is_ascii_lowercase())
            {
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
                    coin: DENOMINATIONS[i],
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
        let index = match coin {
            1 => 0,
            3 => 1,
            4 => 2,
            10 => 3,
            _ => return Err(Error::BadCoin { coin }),
        };
        let new_credit = self.credit as u32 + coin as u32;
        if new_credit > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit = new_credit as u16;
        self.escrow.push(coin);
        let _ = index;
        self.log(EventKind::Inserted {
            coin,
            credit: self.credit,
        });
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        if !self.products.iter().any(|p| p.id == id) {
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
        let id = match &self.selected {
            Some(id) => id.clone(),
            None => return Err(Error::NoSelection),
        };
        let product_index = self.products.iter().position(|p| p.id == id).unwrap();
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
        let change = match choose_change(amount, &self.tubes) {
            Some(change) => change,
            None => return Err(Error::NoChange { amount }),
        };

        let mut proposed = self.tubes;
        for i in 0..4 {
            proposed[i] -= change[i];
        }
        for &coin in &self.escrow {
            let index = denomination_index(coin).unwrap();
            proposed[index] += 1;
        }
        for i in 0..4 {
            if proposed[i] > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
                });
            }
        }

        let change_coins = coins_descending(&change);
        self.tubes = proposed;
        self.products[product_index].stock -= 1;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

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
        let index = match self.products.iter().position(|p| p.id == id) {
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
        self.log(EventKind::PriceChanged {
            id: id.to_owned(),
            old,
            new: price,
        });
        Ok(())
    }

    pub fn restock(&mut self, id: &str, count: u16) -> Result<(), Error> {
        let index = match self.products.iter().position(|p| p.id == id) {
            Some(index) => index,
            None => return Err(Error::UnknownProduct { id: id.to_owned() }),
        };
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

fn denomination_index(coin: u16) -> Option<usize> {
    match coin {
        1 => Some(0),
        3 => Some(1),
        4 => Some(2),
        10 => Some(3),
        _ => None,
    }
}

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<[u16; 4]> {
    let mut greedy = [0u16; 4];
    let mut remaining = amount;

    for i in (0..4).rev() {
        let coin = DENOMINATIONS[i];
        let count = (remaining / coin).min(tubes[i]);
        greedy[i] = count;
        remaining -= count * coin;
    }

    if remaining == 0 {
        return Some(greedy);
    }

    let mut best: Option<[u16; 4]> = None;
    search_change(
        3,
        amount as u32,
        tubes,
        &mut [0u16; 4],
        &mut best,
    );
    best
}

fn search_change(
    index: isize,
    remaining: u32,
    tubes: &[u16; 4],
    current: &mut [u16; 4],
    best: &mut Option<[u16; 4]>,
) {
    if remaining == 0 {
        let replace = match best {
            None => true,
            Some(existing) => {
                let current_total: u32 = current.iter().map(|&n| n as u32).sum();
                let existing_total: u32 = existing.iter().map(|&n| n as u32).sum();
                current_total < existing_total
                    || (current_total == existing_total
                        && (0..4).rev().find_map(|i| {
                            if current[i] != existing[i] {
                                Some(current[i] > existing[i])
                            } else {
                                None
                            }
                        }) == Some(true))
            }
        };
        if replace {
            *best = Some(*current);
        }
        return;
    }
    if index < 0 {
        return;
    }

    let i = index as usize;
    let coin = DENOMINATIONS[i] as u32;
    let max_count = (remaining / coin).min(tubes[i] as u32);
    for count in 0..=max_count {
        current[i] = count as u16;
        search_change(
            index - 1,
            remaining - count * coin,
            tubes,
            current,
            best,
        );
    }
    current[i] = 0;
}

fn coins_descending(counts: &[u16; 4]) -> Vec<u16> {
    let mut coins = Vec::new();
    for i in (0..4).rev() {
        for _ in 0..counts[i] {
            coins.push(DENOMINATIONS[i]);
        }
    }
    coins
}