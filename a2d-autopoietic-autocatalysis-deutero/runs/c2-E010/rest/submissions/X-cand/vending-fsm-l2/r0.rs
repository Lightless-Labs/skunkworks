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

const DENOMS: [u16; 4] = [1, 3, 4, 10];

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase())
}

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut seen = HashSet::new();
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
        }

        for i in 0..4 {
            if tubes[i] > 100 {
                return Err(Error::TubeLimit { coin: DENOMS[i] });
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
        if !DENOMS.contains(&coin) {
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
        if !self.products.iter().any(|p| p.id == id) {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        }
        if self.selected.as_deref() == Some(id) {
            return Err(Error::SameSelection);
        }
        self.selected = Some(id.to_owned());
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let selected_id = match self.selected.as_ref() {
            Some(id) => id,
            None => return Err(Error::NoSelection),
        };
        let product = self.products.iter().find(|p| p.id == *selected_id).unwrap();
        let price = product.price;
        let id = product.id.clone();

        if self.credit < price {
            return Err(Error::Underpaid {
                price,
                credit: self.credit,
            });
        }

        let amount = self.credit - price;
        let counts = choose_change(amount, self.tubes)
            .ok_or(Error::NoChange { amount })?;

        let mut proposed = self.tubes;
        for i in 0..4 {
            proposed[i] -= counts[i];
        }
        for &coin in &self.escrow {
            let idx = DENOMS.iter().position(|&d| d == coin).unwrap();
            proposed[idx] += 1;
        }
        for i in 0..4 {
            if proposed[i] > 100 {
                return Err(Error::TubeLimit { coin: DENOMS[i] });
            }
        }

        let mut change = Vec::new();
        for i in (0..4).rev() {
            for _ in 0..counts[i] {
                change.push(DENOMS[i]);
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
    if amount == 0 {
        return Some([0; 4]);
    }

    let mut greedy = [0u16; 4];
    let mut rem = amount;
    for i in (0..4).rev() {
        let take = tubes[i].min(rem / DENOMS[i]);
        greedy[i] = take;
        rem -= take * DENOMS[i];
    }
    if rem == 0 {
        return Some(greedy);
    }

    let mut best: Option<[u16; 4]> = None;
    let mut best_total = u32::MAX;

    fn search(
        i: isize,
        rem: u16,
        tubes: [u16; 4],
        current: &mut [u16; 4],
        total: u32,
        best: &mut Option<[u16; 4]>,
        best_total: &mut u32,
    ) {
        if i < 0 {
            if rem == 0
                && (total < *best_total
                    || (total == *best_total
                        && best.map_or(true, |b| {
                            for j in (0..4).rev() {
                                if current[j] != b[j] {
                                    return current[j] > b[j];
                                }
                            }
                            false
                        })))
            {
                *best_total = total;
                *best = Some(*current);
            }
            return;
        }

        let idx = i as usize;
        let denom = DENOMS[idx];
        let max = tubes[idx].min(rem / denom);
        for count in 0..=max {
            let next_total = total + u32::from(count);
            if next_total > *best_total {
                continue;
            }
            current[idx] = count;
            search(
                i - 1,
                rem - count * denom,
                tubes,
                current,
                next_total,
                best,
                best_total,
            );
        }
        current[idx] = 0;
    }

    search(
        3,
        amount,
        tubes,
        &mut [0; 4],
        0,
        &mut best,
        &mut best_total,
    );
    best
}
