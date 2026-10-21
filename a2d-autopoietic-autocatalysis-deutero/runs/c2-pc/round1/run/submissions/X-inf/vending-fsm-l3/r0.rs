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
            selected: self.selected.map(|index| self.products[index].id.clone()),
            escrow: self.escrow.clone(),
            tubes: self.tubes,
        }
    }

    pub fn insert(&mut self, coin: u16) -> Result<(), Error> {
        if !DENOMINATIONS.contains(&coin) {
            return Err(Error::BadCoin { coin });
        }
        if self.credit + coin > 100 {
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
        let index = self
            .products
            .iter()
            .position(|product| product.id == id)
            .ok_or_else(|| Error::UnknownProduct { id: id.to_owned() })?;

        if self.selected == Some(index) {
            return Err(Error::SameSelection);
        }

        self.selected = Some(index);
        self.push_event(EventKind::Selected {
            id: id.to_owned(),
        });
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let index = self.selected.ok_or(Error::NoSelection)?;
        let product = &self.products[index];

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
        let change = choose_change(amount, self.tubes)
            .ok_or(Error::NoChange { amount })?;
        let mut proposed_tubes = self.tubes;

        for coin in &change {
            let slot = denomination_index(*coin).expect("chosen denomination");
            proposed_tubes[slot] -= 1;
        }
        for coin in &self.escrow {
            let slot = denomination_index(*coin).expect("accepted denomination");
            proposed_tubes[slot] += 1;
        }
        for (index, count) in proposed_tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[index],
                });
            }
        }

        self.tubes = proposed_tubes;
        self.products[index].stock -= 1;
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
        let index = self
            .products
            .iter()
            .position(|product| product.id == id)
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
        let index = self
            .products
            .iter()
            .position(|product| product.id == id)
            .ok_or_else(|| Error::UnknownProduct { id: id.to_owned() })?;

        if count == 0 {
            return Err(Error::ZeroRestock);
        }
        if self.products[index].stock as u32 + count as u32 > 100 {
            return Err(Error::StockLimit { id: id.to_owned() });
        }

        self.products[index].stock += count;
        self.push_event(EventKind::Restocked {
            id: id.to_owned(),
            added: count,
            stock: self.products[index].stock,
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
    DENOMINATIONS.iter().position(|&denomination| denomination == coin)
}

fn choose_change(amount: u16, tubes: [u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    let mut remaining = amount as usize;
    let mut greedy_counts = [0u16; 4];
    for index in (0..4).rev() {
        let denomination = DENOMINATIONS[index] as usize;
        let take = (tubes[index] as usize).min(remaining / denomination);
        greedy_counts[index] = take as u16;
        remaining -= take * denomination;
    }

    if remaining == 0 {
        return Some(counts_to_descending(greedy_counts));
    }

    let target = amount as usize;
    let mut best: Option<[u16; 4]> = None;
    let mut best_count = usize::MAX;

    fn search(
        index: isize,
        remaining: usize,
        tubes: [u16; 4],
        counts: &mut [u16; 4],
        best: &mut Option<[u16; 4]>,
        best_count: &mut usize,
    ) {
        if index < 0 {
            if remaining == 0 {
                let total: usize = counts.iter().map(|&count| count as usize).sum();
                let is_better = total < *best_count
                    || (total == *best_count
                        && best
                            .as_ref()
                            .map_or(true, |current| descending_lex_greater(counts, current)));
                if is_better {
                    *best = Some(*counts);
                    *best_count = total;
                }
            }
            return;
        }

        let i = index as usize;
        let denomination = DENOMINATIONS[i] as usize;
        let max_take = (tubes[i] as usize).min(remaining / denomination);
        for take in 0..=max_take {
            counts[i] = take as u16;
            search(
                index - 1,
                remaining - take * denomination,
                tubes,
                counts,
                best,
                best_count,
            );
        }
        counts[i] = 0;
    }

    fn descending_lex_greater(a: &[u16; 4], b: &[u16; 4]) -> bool {
        for index in (0..4).rev() {
            if a[index] != b[index] {
                return a[index] > b[index];
            }
        }
        false
    }

    search(
        3,
        target,
        tubes,
        &mut [0; 4],
        &mut best,
        &mut best_count,
    );
    best.map(counts_to_descending)
}

fn counts_to_descending(counts: [u16; 4]) -> Vec<u16> {
    let mut coins = Vec::new();
    for index in (0..4).rev() {
        for _ in 0..counts[index] {
            coins.push(DENOMINATIONS[index]);
        }
    }
    coins
}