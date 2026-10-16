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
            if !(1..=8).contains(&product.id.len())
                || !product.id.bytes().all(|b| b.is_ascii_lowercase())
            {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }
            if !seen.insert(product.id.as_str()) {
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
        let index = match DENOMINATIONS.iter().position(|&d| d == coin) {
            Some(index) => index,
            None => return Err(Error::BadCoin { coin }),
        };
        let _ = index;
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
        if !self.products.iter().any(|p| p.id == id) {
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
        let id = match self.selected.as_deref() {
            Some(id) => id,
            None => return Err(Error::NoSelection),
        };
        let product_index = match self.products.iter().position(|p| p.id == id) {
            Some(index) => index,
            None => return Err(Error::NoSelection),
        };
        let product = &self.products[product_index];

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

        let amount = self.credit - product.price;
        let change = match choose_change(self.tubes, amount) {
            Some(change) => change,
            None => return Err(Error::NoChange { amount }),
        };

        let mut proposed_tubes = self.tubes;
        for (i, count) in change.iter().enumerate() {
            proposed_tubes[i] -= count;
        }
        for &coin in &self.escrow {
            let index = DENOMINATIONS.iter().position(|&d| d == coin).unwrap();
            let total = u32::from(proposed_tubes[index]) + 1;
            if total > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[index],
                });
            }
            proposed_tubes[index] = total as u16;
        }

        let receipt = Receipt {
            id: product.id.clone(),
            price: product.price,
            change: coins_descending(&change),
        };

        self.products[product_index].stock -= 1;
        self.tubes = proposed_tubes;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.push_event(EventKind::Vended {
            id: receipt.id.clone(),
            price: receipt.price,
            change: receipt.change.clone(),
        });
        Ok(receipt)
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
        self.push_event(EventKind::PriceChanged {
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

    fn push_event(&mut self, kind: EventKind) {
        self.events.push(Event {
            seq: self.events.len(),
            kind,
        });
    }
}

fn choose_change(tubes: [u16; 4], amount: u16) -> Option<[u16; 4]> {
    let mut greedy = [0u16; 4];
    let mut remaining = amount;

    for index in (0..4).rev() {
        let denomination = DENOMINATIONS[index];
        let take = tubes[index].min(remaining / denomination);
        greedy[index] = take;
        remaining -= take * denomination;
    }
    if remaining == 0 {
        return Some(greedy);
    }

    let mut best: Option<[u16; 4]> = None;
    let mut candidate = [0u16; 4];
    search_change(3, amount, tubes, &mut candidate, &mut best);
    best
}

fn search_change(
    index: isize,
    remaining: u16,
    tubes: [u16; 4],
    candidate: &mut [u16; 4],
    best: &mut Option<[u16; 4]>,
) {
    if remaining == 0 {
        let replace = match best {
            None => true,
            Some(current) => {
                let candidate_count: u32 = candidate.iter().map(|&n| u32::from(n)).sum();
                let current_count: u32 = current.iter().map(|&n| u32::from(n)).sum();
                candidate_count < current_count
                    || (candidate_count == current_count
                        && lex_greater_desc(candidate, current))
            }
        };
        if replace {
            *best = Some(*candidate);
        }
        return;
    }
    if index < 0 {
        return;
    }

    let i = index as usize;
    let denomination = DENOMINATIONS[i];
    let max_take = tubes[i].min(remaining / denomination);
    for take in (0..=max_take).rev() {
        candidate[i] = take;
        search_change(
            index - 1,
            remaining - take * denomination,
            tubes,
            candidate,
            best,
        );
    }
    candidate[i] = 0;
}

fn lex_greater_desc(left: &[u16; 4], right: &[u16; 4]) -> bool {
    for index in (0..4).rev() {
        if left[index] != right[index] {
            return left[index] > right[index];
        }
    }
    false
}

fn coins_descending(counts: &[u16; 4]) -> Vec<u16> {
    let capacity: usize = counts.iter().map(|&n| usize::from(n)).sum();
    let mut coins = Vec::with_capacity(capacity);
    for index in (0..4).rev() {
        coins.extend(std::iter::repeat_n(DENOMINATIONS[index], counts[index] as usize));
    }
    coins
}