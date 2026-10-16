use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Product {
    pub id: String,
    pub price: u16,
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
}

pub struct Machine {
    products: Vec<Product>,
    credit: u16,
    selected: Option<String>,
    escrow: Vec<u16>,
    tubes: [u16; 4],
}

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut seen = HashSet::new();
        for product in &products {
            let valid_id = !product.id.is_empty()
                && product.id.len() <= 8
                && product.id.bytes().all(|b| b.is_ascii_lowercase());
            if !valid_id {
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
        }

        let denoms = [1u16, 3, 4, 10];
        for (i, &count) in tubes.iter().enumerate() {
            if count > 100 {
                return Err(Error::TubeLimit { coin: denoms[i] });
            }
        }

        Ok(Self {
            products,
            credit: 0,
            selected: None,
            escrow: Vec::new(),
            tubes,
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
        if !matches!(coin, 1 | 3 | 4 | 10) {
            return Err(Error::BadCoin { coin });
        }
        if u32::from(self.credit) + u32::from(coin) > 100 {
            return Err(Error::CreditLimit);
        }
        self.credit += coin;
        self.escrow.push(coin);
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        if !self.products.iter().any(|product| product.id == id) {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        }
        if self.selected.as_deref() == Some(id) {
            return Err(Error::SameSelection);
        }
        self.selected = Some(id.to_owned());
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let selected_id = match self.selected.as_deref() {
            Some(id) => id,
            None => return Err(Error::NoSelection),
        };
        let product = self
            .products
            .iter()
            .find(|product| product.id == selected_id)
            .expect("selected product is always in the catalog");

        let price = product.price;
        let id = product.id.clone();
        if self.credit < price {
            return Err(Error::Underpaid {
                price,
                credit: self.credit,
            });
        }

        let amount = self.credit - price;
        let chosen = match choose_change(amount, self.tubes) {
            Some(coins) => coins,
            None => return Err(Error::NoChange { amount }),
        };

        let mut proposed = self.tubes;
        for (i, &denom) in [1u16, 3, 4, 10].iter().enumerate() {
            proposed[i] -= chosen[i];
        }
        for &coin in &self.escrow {
            let index = match coin {
                1 => 0,
                3 => 1,
                4 => 2,
                10 => 3,
                _ => unreachable!(),
            };
            proposed[index] += 1;
        }

        for (i, &count) in proposed.iter().enumerate() {
            if count > 100 {
                return Err(Error::TubeLimit {
                    coin: [1, 3, 4, 10][i],
                });
            }
        }

        let mut change = Vec::new();
        for (i, &denom) in [1u16, 3, 4, 10].iter().enumerate().rev() {
            for _ in 0..chosen[i] {
                change.push(denom);
            }
        }

        self.tubes = proposed;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        Ok(Receipt { id, price, change })
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        let returned = self.escrow.iter().rev().copied().collect();
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        Ok(returned)
    }
}

fn choose_change(amount: u16, tubes: [u16; 4]) -> Option<[u16; 4]> {
    let denoms = [1u16, 3, 4, 10];

    let mut greedy = [0u16; 4];
    let mut remaining = amount;
    for i in (0..4).rev() {
        let take = tubes[i].min(remaining / denoms[i]);
        greedy[i] = take;
        remaining -= take * denoms[i];
    }
    if remaining == 0 {
        return Some(greedy);
    }

    let mut best: Option<[u16; 4]> = None;
    let mut best_count = u32::MAX;
    search_change(
        3,
        amount,
        &tubes,
        &mut [0; 4],
        0,
        &mut best,
        &mut best_count,
    );
    best
}

fn search_change(
    index: isize,
    remaining: u16,
    tubes: &[u16; 4],
    current: &mut [u16; 4],
    count: u32,
    best: &mut Option<[u16; 4]>,
    best_count: &mut u32,
) {
    if remaining == 0 {
        let is_better = count < *best_count
            || (count == *best_count
                && best
                    .as_ref()
                    .is_some_and(|old| descending_lex_greater(current, old)));
        if best.is_none() || is_better {
            *best = Some(*current);
            *best_count = count;
        }
        return;
    }
    if index < 0 || count >= *best_count {
        return;
    }

    let i = index as usize;
    let denom = [1u16, 3, 4, 10][i];
    let max_take = tubes[i].min(remaining / denom);
    for take in (0..=max_take).rev() {
        let next_count = count + u32::from(take);
        if next_count > *best_count {
            continue;
        }
        current[i] = take;
        search_change(
            index - 1,
            remaining - take * denom,
            tubes,
            current,
            next_count,
            best,
            best_count,
        );
    }
    current[i] = 0;
}

fn descending_lex_greater(a: &[u16; 4], b: &[u16; 4]) -> bool {
    for i in (0..4).rev() {
        if a[i] != b[i] {
            return a[i] > b[i];
        }
    }
    false
}