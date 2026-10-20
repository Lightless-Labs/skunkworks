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
    by_id: HashMap<String, usize>,
    credit: u16,
    selected: Option<String>,
    escrow: Vec<u16>,
    tubes: [u16; 4],
    events: Vec<Event>,
}

const DENOMS: [u16; 4] = [1, 3, 4, 10];

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase())
}

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut seen = HashMap::<String, ()>::new();
        for p in &products {
            if !valid_id(&p.id) {
                return Err(Error::BadId { id: p.id.clone() });
            }
            if seen.contains_key(&p.id) {
                return Err(Error::DuplicateId { id: p.id.clone() });
            }
            seen.insert(p.id.clone(), ());
            if !(1..=100).contains(&p.price) {
                return Err(Error::BadPrice { id: p.id.clone() });
            }
            if p.stock > 100 {
                return Err(Error::StockLimit { id: p.id.clone() });
            }
        }

        for i in 0..4 {
            if tubes[i] > 100 {
                return Err(Error::TubeLimit { coin: DENOMS[i] });
            }
        }

        let by_id = products
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.clone(), i))
            .collect();

        Ok(Self {
            products,
            by_id,
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
        if !DENOMS.contains(&coin) {
            return Err(Error::BadCoin { coin });
        }
        let total = u32::from(self.credit) + u32::from(coin);
        if total > 100 {
            return Err(Error::CreditLimit);
        }
        self.credit = total as u16;
        self.escrow.push(coin);
        self.push_event(EventKind::Inserted {
            coin,
            credit: self.credit,
        });
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        if !self.by_id.contains_key(id) {
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
        let id = self.selected.clone().ok_or(Error::NoSelection)?;
        let index = *self.by_id.get(&id).expect("selected catalog product");
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
        let change = choose_change(amount, &self.tubes).ok_or(Error::NoChange { amount })?;

        let mut proposed = self.tubes;
        for &coin in &change {
            let i = DENOMS.iter().position(|&d| d == coin).expect("valid change coin");
            proposed[i] -= 1;
        }
        for &coin in &self.escrow {
            let i = DENOMS.iter().position(|&d| d == coin).expect("valid escrow coin");
            proposed[i] += 1;
        }
        for i in 0..4 {
            if proposed[i] > 100 {
                return Err(Error::TubeLimit { coin: DENOMS[i] });
            }
        }

        self.products[index].stock -= 1;
        self.tubes = proposed;
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
        let mut result = self.products.clone();
        result.sort_by(|a, b| a.id.as_bytes().cmp(b.id.as_bytes()));
        result
    }

    pub fn set_price(&mut self, id: &str, price: u16) -> Result<(), Error> {
        let index = *self
            .by_id
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
        self.push_event(EventKind::PriceChanged {
            id: id.to_owned(),
            old,
            new: price,
        });
        Ok(())
    }

    pub fn restock(&mut self, id: &str, count: u16) -> Result<(), Error> {
        let index = *self
            .by_id
            .get(id)
            .ok_or_else(|| Error::UnknownProduct { id: id.to_owned() })?;
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

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    let mut greedy_counts = [0u16; 4];
    let mut remaining = amount;
    for i in (0..4).rev() {
        let take = tubes[i].min(remaining / DENOMS[i]);
        greedy_counts[i] = take;
        remaining -= take * DENOMS[i];
    }
    if remaining == 0 {
        return Some(counts_to_descending(&greedy_counts));
    }

    let mut best: Option<[u16; 4]> = None;
    search_change(0, amount, tubes, &mut [0; 4], &mut best);
    best.map(|counts| counts_to_descending(&counts))
}

fn search_change(
    index: usize,
    remaining: u16,
    tubes: &[u16; 4],
    counts: &mut [u16; 4],
    best: &mut Option<[u16; 4]>,
) {
    if index == 4 {
        if remaining != 0 {
            return;
        }
        let replace = match best {
            None => true,
            Some(old) => {
                let total_new: u32 = counts.iter().map(|&x| u32::from(x)).sum();
                let total_old: u32 = old.iter().map(|&x| u32::from(x)).sum();
                total_new < total_old
                    || (total_new == total_old
                        && (0..4).rev().find_map(|i| {
                            if counts[i] != old[i] {
                                Some(counts[i] > old[i])
                            } else {
                                None
                            }
                        }) == Some(true))
            }
        };
        if replace {
            *best = Some(*counts);
        }
        return;
    }

    let coin = DENOMS[index];
    let max_take = tubes[index].min(remaining / coin);
    for take in 0..=max_take {
        counts[index] = take;
        search_change(index + 1, remaining - take * coin, tubes, counts, best);
    }
    counts[index] = 0;
}

fn counts_to_descending(counts: &[u16; 4]) -> Vec<u16> {
    let mut result = Vec::new();
    for i in (0..4).rev() {
        for _ in 0..counts[i] {
            result.push(DENOMS[i]);
        }
    }
    result
}
