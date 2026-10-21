use std::collections::HashMap;

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
    products: HashMap<String, u16>,
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

        let mut catalog = HashMap::new();
        for product in products {
            if product.id.is_empty()
                || product.id.len() > 8
                || !product.id.bytes().all(|b| b.is_ascii_lowercase())
            {
                return Err(Error::BadId { id: product.id });
            }
            if catalog.contains_key(&product.id) {
                return Err(Error::DuplicateId { id: product.id });
            }
            if !(1..=100).contains(&product.price) {
                return Err(Error::BadPrice { id: product.id });
            }
            catalog.insert(product.id, product.price);
        }

        for (index, count) in tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[index],
                });
            }
        }

        Ok(Self {
            products: catalog,
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
        let index = DENOMINATIONS.iter().position(|&d| d == coin);
        let Some(_) = index else {
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
        if !self.products.contains_key(id) {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        }
        if self.selected.as_deref() == Some(id) {
            return Err(Error::SameSelection);
        }
        self.selected = Some(id.to_owned());
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let Some(id) = self.selected.as_ref() else {
            return Err(Error::NoSelection);
        };
        let price = self.products[id];
        if self.credit < price {
            return Err(Error::Underpaid {
                price,
                credit: self.credit,
            });
        }

        let amount = self.credit - price;
        let (change_counts, change) = choose_change(amount, self.tubes)
            .ok_or(Error::NoChange { amount })?;

        let mut proposed_tubes = self.tubes;
        for i in 0..4 {
            proposed_tubes[i] -= change_counts[i];
        }
        for &coin in &self.escrow {
            let index = DENOMINATIONS.iter().position(|&d| d == coin).unwrap();
            proposed_tubes[index] += 1;
        }
        for (index, count) in proposed_tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[index],
                });
            }
        }

        let receipt = Receipt {
            id: id.clone(),
            price,
            change,
        };
        self.tubes = proposed_tubes;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        Ok(receipt)
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        let returned = self.escrow.iter().rev().copied().collect();
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        Ok(returned)
    }
}

fn choose_change(amount: u16, tubes: [u16; 4]) -> Option<([u16; 4], Vec<u16>)> {
    let mut greedy = [0u16; 4];
    let mut remaining = amount;

    for index in (0..4).rev() {
        let coin = DENOMINATIONS[index];
        let count = tubes[index].min(remaining / coin);
        greedy[index] = count;
        remaining -= count * coin;
    }

    let counts = if remaining == 0 {
        greedy
    } else {
        let mut best: Option<[u16; 4]> = None;
        search_change(
            3,
            amount,
            &tubes,
            &mut [0u16; 4],
            &mut best,
        );
        best?
    };

    let mut change = Vec::new();
    for index in (0..4).rev() {
        for _ in 0..counts[index] {
            change.push(DENOMINATIONS[index]);
        }
    }
    Some((counts, change))
}

fn search_change(
    index: isize,
    remaining: u16,
    tubes: &[u16; 4],
    current: &mut [u16; 4],
    best: &mut Option<[u16; 4]>,
) {
    if remaining == 0 {
        let replace = match best {
            None => true,
            Some(existing) => {
                let current_total: u32 = current.iter().map(|&n| u32::from(n)).sum();
                let existing_total: u32 = existing.iter().map(|&n| u32::from(n)).sum();
                current_total < existing_total
                    || (current_total == existing_total
                        && (3..=0).rev().any(|_| false))
                    || (current_total == existing_total
                        && lex_greater_desc(current, existing))
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
    let coin = DENOMINATIONS[i];
    let max_count = tubes[i].min(remaining / coin);
    for count in 0..=max_count {
        current[i] = count;
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

fn lex_greater_desc(left: &[u16; 4], right: &[u16; 4]) -> bool {
    for index in (0..4).rev() {
        if left[index] != right[index] {
            return left[index] > right[index];
        }
    }
    false
}