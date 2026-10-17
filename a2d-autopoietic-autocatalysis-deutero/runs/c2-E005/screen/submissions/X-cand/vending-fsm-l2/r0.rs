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

        let mut seen: Vec<&str> = Vec::new();
        for product in &products {
            let valid_id = !product.id.is_empty()
                && product.id.len() <= 8
                && product.id.bytes().all(|b| b.is_ascii_lowercase());
            if !valid_id {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }
            if seen.contains(&product.id.as_str()) {
                return Err(Error::DuplicateId {
                    id: product.id.clone(),
                });
            }
            seen.push(&product.id);

            if !(1..=100).contains(&product.price) {
                return Err(Error::BadPrice {
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
        if !DENOMINATIONS.contains(&coin) {
            return Err(Error::BadCoin { coin });
        }

        let new_credit = self.credit.checked_add(coin).ok_or(Error::CreditLimit)?;
        if new_credit > 100 {
            return Err(Error::CreditLimit);
        }

        self.escrow.push(coin);
        self.credit = new_credit;
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
            .expect("selected product must belong to catalog");
        let price = product.price;
        let id = product.id.clone();

        if self.credit < price {
            return Err(Error::Underpaid {
                price,
                credit: self.credit,
            });
        }

        let amount = self.credit - price;
        let mut change_counts = [0u16; 4];
        let mut remainder = amount;

        for index in (0..4).rev() {
            let coin = DENOMINATIONS[index];
            let count = self.tubes[index].min(remainder / coin);
            change_counts[index] = count;
            remainder -= count * coin;
        }

        if remainder != 0 {
            change_counts = match find_best_change(self.tubes, amount) {
                Some(counts) => counts,
                None => return Err(Error::NoChange { amount }),
            };
        }

        let mut proposed_tubes = self.tubes;
        for index in 0..4 {
            proposed_tubes[index] -= change_counts[index];
        }
        for &coin in &self.escrow {
            let index = DENOMINATIONS
                .iter()
                .position(|&denomination| denomination == coin)
                .expect("escrow contains only accepted coins");
            proposed_tubes[index] += 1;
        }
        for index in 0..4 {
            if proposed_tubes[index] > 100 {
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
        let refunded = self.escrow.iter().rev().copied().collect();
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        Ok(refunded)
    }
}

fn find_best_change(tubes: [u16; 4], amount: u16) -> Option<[u16; 4]> {
    fn search(
        index: isize,
        remaining: u16,
        tubes: &[u16; 4],
        counts: &mut [u16; 4],
        best: &mut Option<[u16; 4]>,
        best_total: &mut u16,
    ) {
        if index < 0 {
            if remaining != 0 {
                return;
            }
            let total: u16 = counts.iter().copied().sum();
            let replace = match best {
                None => true,
                Some(current) => {
                    total < *best_total
                        || (total == *best_total && descending_lex_greater(counts, current))
                }
            };
            if replace {
                *best = Some(*counts);
                *best_total = total;
            }
            return;
        }

        let i = index as usize;
        let coin = DENOMINATIONS[i];
        let max_count = tubes[i].min(remaining / coin);
        for count in 0..=max_count {
            counts[i] = count;
            search(
                index - 1,
                remaining - count * coin,
                tubes,
                counts,
                best,
                best_total,
            );
        }
        counts[i] = 0;
    }

    let mut best = None;
    let mut best_total = u16::MAX;
    let mut counts = [0; 4];
    search(
        3,
        amount,
        &tubes,
        &mut counts,
        &mut best,
        &mut best_total,
    );
    best
}

fn descending_lex_greater(left: &[u16; 4], right: &[u16; 4]) -> bool {
    for index in (0..4).rev() {
        if left[index] != right[index] {
            return left[index] > right[index];
        }
    }
    false
}