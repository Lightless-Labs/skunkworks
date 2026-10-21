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

const DENOMINATIONS: [u16; 4] = [1, 3, 4, 10];

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

        for (index, &count) in tubes.iter().enumerate() {
            if count > 100 {
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
        let Some(_) = denomination_index(coin) else {
            return Err(Error::BadCoin { coin });
        };

        if u32::from(self.credit) + u32::from(coin) > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit += coin;
        self.escrow.push(coin);
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        let Some(_) = self.products.iter().find(|product| product.id == id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };

        if self.selected.as_deref() == Some(id) {
            return Err(Error::SameSelection);
        }

        self.selected = Some(id.to_owned());
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let Some(selected_id) = self.selected.as_deref() else {
            return Err(Error::NoSelection);
        };

        let product = self
            .products
            .iter()
            .find(|product| product.id == selected_id)
            .expect("selected product must be in catalog");
        let price = product.price;
        let id = product.id.clone();

        if self.credit < price {
            return Err(Error::Underpaid {
                price,
                credit: self.credit,
            });
        }

        let amount = self.credit - price;
        let change_counts = choose_change(amount, &self.tubes)
            .ok_or(Error::NoChange { amount })?;

        let mut proposed_tubes = self.tubes;
        for index in 0..4 {
            proposed_tubes[index] -= change_counts[index];
        }
        for &coin in &self.escrow {
            let index = denomination_index(coin).expect("escrow coins are valid");
            proposed_tubes[index] += 1;
        }

        for (index, &count) in proposed_tubes.iter().enumerate() {
            if count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[index],
                });
            }
        }

        let mut change = Vec::new();
        for index in (0..4).rev() {
            for _ in 0..change_counts[index] {
                change.push(DENOMINATIONS[index]);
            }
        }

        self.tubes = proposed_tubes;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        Ok(Receipt { id, price, change })
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        let coins = self.escrow.iter().rev().copied().collect();
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        Ok(coins)
    }
}

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_lowercase())
}

fn denomination_index(coin: u16) -> Option<usize> {
    DENOMINATIONS.iter().position(|&denomination| denomination == coin)
}

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<[u16; 4]> {
    let mut greedy = [0u16; 4];
    let mut remaining = amount;

    for index in (0..4).rev() {
        let coin = DENOMINATIONS[index];
        let taken = tubes[index].min(remaining / coin);
        greedy[index] = taken;
        remaining -= taken * coin;
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
    tubes: &[u16; 4],
    candidate: &mut [u16; 4],
    best: &mut Option<[u16; 4]>,
) {
    if remaining == 0 {
        if best
            .as_ref()
            .is_none_or(|current| is_better(candidate, current))
        {
            *best = Some(*candidate);
        }
        return;
    }
    if index < 0 {
        return;
    }

    let i = index as usize;
    let coin = DENOMINATIONS[i];
    let max_count = tubes[i].min(remaining / coin);
    for count in (0..=max_count).rev() {
        candidate[i] = count;
        let next_remaining = remaining - count * coin;

        if let Some(current_best) = best.as_ref() {
            let current_total: u16 = current_best.iter().sum();
            let candidate_total: u16 = candidate.iter().sum();
            let minimum_possible = candidate_total + next_remaining.div_ceil(coin);
            if minimum_possible > current_total {
                continue;
            }
        }

        search_change(index - 1, next_remaining, tubes, candidate, best);
    }
    candidate[i] = 0;
}

fn is_better(candidate: &[u16; 4], current: &[u16; 4]) -> bool {
    let candidate_total: u16 = candidate.iter().sum();
    let current_total: u16 = current.iter().sum();
    if candidate_total != current_total {
        return candidate_total < current_total;
    }

    for index in (0..4).rev() {
        if candidate[index] != current[index] {
            return candidate[index] > current[index];
        }
    }
    false
}