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

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase())
}

fn coin_index(coin: u16) -> Option<usize> {
    DENOMINATIONS.iter().position(|&d| d == coin)
}

fn exact_change(tubes: &[u16; 4], amount: u16) -> Option<[u16; 4]> {
    fn search(
        i: usize,
        remaining: u16,
        tubes: &[u16; 4],
        counts: &mut [u16; 4],
        best: &mut Option<[u16; 4]>,
        best_total: &mut u16,
    ) {
        if i == 4 {
            if remaining == 0 {
                let total: u16 = counts.iter().copied().sum();
                let better = match best {
                    None => true,
                    Some(old) if total < *best_total => true,
                    Some(old) if total == *best_total => {
                        // Compare from largest denomination to smallest.
                        let mut greater = false;
                        for j in (0..4).rev() {
                            if counts[j] != old[j] {
                                greater = counts[j] > old[j];
                                break;
                            }
                        }
                        greater
                    }
                    _ => false,
                };
                if better {
                    *best = Some(*counts);
                    *best_total = total;
                }
            }
            return;
        }

        let coin = DENOMINATIONS[i];
        let max = tubes[i].min(remaining / coin);
        for n in 0..=max {
            counts[i] = n;
            search(i + 1, remaining - n * coin, tubes, counts, best, best_total);
        }
        counts[i] = 0;
    }

    let mut counts = [0; 4];
    let mut best = None;
    let mut best_total = u16::MAX;
    search(0, amount, tubes, &mut counts, &mut best, &mut best_total);
    best
}

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut seen = std::collections::HashSet::new();
        for product in &products {
            if !valid_id(&product.id) {
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

        for (i, &count) in tubes.iter().enumerate() {
            if count > 100 {
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
        if coin_index(coin).is_none() {
            return Err(Error::BadCoin { coin });
        }
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
        let price = product.price;
        if self.credit < price {
            return Err(Error::Underpaid {
                price,
                credit: self.credit,
            });
        }

        let amount = self.credit - price;
        let mut chosen = [0u16; 4];
        if amount != 0 {
            let mut remainder = amount;
            for i in (0..4).rev() {
                let coin = DENOMINATIONS[i];
                let take = self.tubes[i].min(remainder / coin);
                chosen[i] = take;
                remainder -= take * coin;
            }
            if remainder != 0 {
                chosen = exact_change(&self.tubes, amount)
                    .ok_or(Error::NoChange { amount })?;
            }
        }

        let mut proposed = self.tubes;
        for i in 0..4 {
            proposed[i] -= chosen[i];
        }
        for &coin in &self.escrow {
            let i = coin_index(coin).unwrap();
            proposed[i] += 1;
        }
        for i in 0..4 {
            if proposed[i] > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
                });
            }
        }

        let mut change = Vec::new();
        for i in (0..4).rev() {
            for _ in 0..chosen[i] {
                change.push(DENOMINATIONS[i]);
            }
        }

        self.products[product_index].stock -= 1;
        self.tubes = proposed;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        let receipt = Receipt {
            id: id.clone(),
            price,
            change: change.clone(),
        };
        self.log(EventKind::Vended {
            id,
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
        products.sort_by(|a, b| a.id.cmp(&b.id));
        products
    }

    pub fn set_price(&mut self, id: &str, price: u16) -> Result<(), Error> {
        let index = self
            .products
            .iter()
            .position(|p| p.id == id)
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
            .products
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| Error::UnknownProduct { id: id.to_owned() })?;

        if count == 0 {
            return Err(Error::ZeroRestock);
        }
        let stock = u32::from(self.products[index].stock) + u32::from(count);
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

    fn log(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
    }
}