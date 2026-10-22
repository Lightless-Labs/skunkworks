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
            let valid_id = !product.id.is_empty()
                && product.id.len() <= 8
                && product.id.bytes().all(|b| b.is_ascii_lowercase());
            if !valid_id {
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
        let denomination_index = DENOMINATIONS.iter().position(|&d| d == coin);
        if denomination_index.is_none() {
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
        let id = self.selected.as_ref().ok_or(Error::NoSelection)?;
        let price = *self.products.get(id).ok_or(Error::NoSelection)?;

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
        for i in 0..4 {
            proposed_tubes[i] -= change_counts[i];
        }

        for &coin in &self.escrow {
            let i = DENOMINATIONS
                .iter()
                .position(|&d| d == coin)
                .expect("escrow contains only accepted denominations");
            proposed_tubes[i] += 1;
        }

        for i in 0..4 {
            if proposed_tubes[i] > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
                });
            }
        }

        let mut change = Vec::new();
        for i in (0..4).rev() {
            for _ in 0..change_counts[i] {
                change.push(DENOMINATIONS[i]);
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
        let refunded = self.escrow.iter().rev().copied().collect();
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        Ok(refunded)
    }
}

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<[u16; 4]> {
    let mut greedy = [0u16; 4];
    let mut remaining = amount;

    for i in (0..4).rev() {
        let denomination = DENOMINATIONS[i];
        let count = tubes[i].min(remaining / denomination);
        greedy[i] = count;
        remaining -= count * denomination;
    }

    if remaining == 0 {
        return Some(greedy);
    }

    let mut best: Option<[u16; 4]> = None;
    let mut best_count = u32::MAX;

    fn search(
        index: usize,
        remaining: u16,
        tubes: &[u16; 4],
        current: &mut [u16; 4],
        best: &mut Option<[u16; 4]>,
        best_count: &mut u32,
    ) {
        if index == 0 {
            if remaining <= tubes[0] {
                current[0] = remaining;
                let total: u32 = current.iter().map(|&count| u32::from(count)).sum();
                let is_better = total < *best_count
                    || (total == *best_count
                        && best
                            .as_ref()
                            .map_or(true, |old| (0..4).rev().find_map(|i| {
                                if current[i] != old[i] {
                                    Some(current[i] > old[i])
                                } else {
                                    None
                                }
                            }).unwrap_or(false)));
                if is_better {
                    *best = Some(*current);
                    *best_count = total;
                }
            }
            return;
        }

        let denomination = DENOMINATIONS[index];
        let max_count = tubes[index].min(remaining / denomination);
        for count in 0..=max_count {
            current[index] = count;
            search(
                index - 1,
                remaining - count * denomination,
                tubes,
                current,
                best,
                best_count,
            );
        }
        current[index] = 0;
    }

    search(
        3,
        amount,
        tubes,
        &mut [0; 4],
        &mut best,
        &mut best_count,
    );

    best
}