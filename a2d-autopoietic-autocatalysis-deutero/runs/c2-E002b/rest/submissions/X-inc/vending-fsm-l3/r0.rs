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

        let denominations = [1, 3, 4, 10];
        for (index, &count) in tubes.iter().enumerate() {
            if count > 100 {
                return Err(Error::TubeLimit {
                    coin: denominations[index],
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
        if ![1, 3, 4, 10].contains(&coin) {
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
        let Some(_) = self.products.iter().find(|p| p.id == id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if self.selected.as_deref() == Some(id) {
            return Err(Error::SameSelection);
        }

        self.selected = Some(id.to_owned());
        self.push_event(EventKind::Selected { id: id.to_owned() });
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let Some(id) = self.selected.as_ref() else {
            return Err(Error::NoSelection);
        };
        let product_index = self
            .products
            .iter()
            .position(|p| &p.id == id)
            .expect("selected products always exist");
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
        let change = choose_change(amount, &self.tubes)
            .ok_or(Error::NoChange { amount })?;
        let mut proposed_tubes = self.tubes;
        for &coin in &change {
            let index = denomination_index(coin).expect("change uses valid denominations");
            proposed_tubes[index] -= 1;
        }
        for &coin in &self.escrow {
            let index = denomination_index(coin).expect("escrow uses valid denominations");
            proposed_tubes[index] += 1;
        }
        for (index, &count) in proposed_tubes.iter().enumerate() {
            if count > 100 {
                return Err(Error::TubeLimit {
                    coin: [1, 3, 4, 10][index],
                });
            }
        }

        let id = product.id.clone();
        let price = product.price;
        self.products[product_index].stock -= 1;
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
        let Some(index) = self.products.iter().position(|p| p.id == id) else {
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
        self.push_event(EventKind::PriceChanged {
            id: id.to_owned(),
            old,
            new: price,
        });
        Ok(())
    }

    pub fn restock(&mut self, id: &str, count: u16) -> Result<(), Error> {
        let Some(index) = self.products.iter().position(|p| p.id == id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if count == 0 {
            return Err(Error::ZeroRestock);
        }
        let proposed = u32::from(self.products[index].stock) + u32::from(count);
        if proposed > 100 {
            return Err(Error::StockLimit { id: id.to_owned() });
        }

        self.products[index].stock = proposed as u16;
        self.push_event(EventKind::Restocked {
            id: id.to_owned(),
            added: count,
            stock: proposed as u16,
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

fn denomination_index(coin: u16) -> Option<usize> {
    match coin {
        1 => Some(0),
        3 => Some(1),
        4 => Some(2),
        10 => Some(3),
        _ => None,
    }
}

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    let denominations = [1u16, 3, 4, 10];
    let mut greedy = Vec::new();
    let mut remaining = amount;
    for index in (0..4).rev() {
        let coin = denominations[index];
        let count = tubes[index].min(remaining / coin);
        greedy.extend(std::iter::repeat_n(coin, usize::from(count)));
        remaining -= count * coin;
    }
    if remaining == 0 {
        return Some(greedy);
    }

    let target = usize::from(amount);
    let mut best: Vec<Option<Vec<u16>>> = vec![None; target + 1];
    best[0] = Some(Vec::new());

    for value in 1..=target {
        let mut selected: Option<Vec<u16>> = None;
        for index in (0..4).rev() {
            let coin = usize::from(denominations[index]);
            if value < coin || tubes[index] == 0 {
                continue;
            }
            if let Some(previous) = &best[value - coin] {
                let used = previous
                    .iter()
                    .filter(|&&c| c == denominations[index])
                    .count();
                if used >= usize::from(tubes[index]) {
                    continue;
                }
                let mut candidate = previous.clone();
                candidate.push(denominations[index]);
                candidate.sort_by(|a, b| b.cmp(a));
                let is_better = match &selected {
                    None => true,
                    Some(current) => {
                        candidate.len() < current.len()
                            || (candidate.len() == current.len()
                                && candidate.as_slice() > current.as_slice())
                    }
                };
                if is_better {
                    selected = Some(candidate);
                }
            }
        }
        best[value] = selected;
    }

    best[target].clone()
}