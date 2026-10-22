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

        let mut seen: Vec<String> = Vec::new();
        for product in &products {
            let valid_id = (1..=8).contains(&product.id.len())
                && product.id.bytes().all(|b| b.is_ascii_lowercase());
            if !valid_id {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }
            if seen.iter().any(|id| id == &product.id) {
                return Err(Error::DuplicateId {
                    id: product.id.clone(),
                });
            }
            seen.push(product.id.clone());

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
        if !DENOMINATIONS.contains(&coin) {
            return Err(Error::BadCoin { coin });
        }
        if self.credit + coin > 100 {
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
        let product = match self.products.iter().find(|product| product.id == selected_id) {
            Some(product) => product,
            None => return Err(Error::NoSelection),
        };
        let price = product.price;
        let id = product.id.clone();

        if self.credit < price {
            return Err(Error::Underpaid {
                price,
                credit: self.credit,
            });
        }

        let amount = self.credit - price;
        let change = match Self::choose_change(amount, self.tubes) {
            Some(coins) => coins,
            None => return Err(Error::NoChange { amount }),
        };

        let mut proposed_tubes = self.tubes;
        for &coin in &change {
            let index = DENOMINATIONS.iter().position(|&d| d == coin).unwrap();
            proposed_tubes[index] -= 1;
        }
        for &coin in &self.escrow {
            let index = DENOMINATIONS.iter().position(|&d| d == coin).unwrap();
            proposed_tubes[index] += 1;
        }

        for (index, &count) in proposed_tubes.iter().enumerate() {
            if count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[index],
                });
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

    fn choose_change(amount: u16, tubes: [u16; 4]) -> Option<Vec<u16>> {
        if amount == 0 {
            return Some(Vec::new());
        }

        let mut remaining = amount;
        let mut counts = [0_u16; 4];
        for index in (0..4).rev() {
            let denomination = DENOMINATIONS[index];
            let take = tubes[index].min(remaining / denomination);
            counts[index] = take;
            remaining -= take * denomination;
        }

        if remaining == 0 {
            return Some(Self::counts_to_descending(counts));
        }

        let mut best: Option<[u16; 4]> = None;
        Self::search_change(
            3,
            amount,
            &tubes,
            &mut counts,
            &mut best,
        );
        best.map(Self::counts_to_descending)
    }

    fn search_change(
        index: isize,
        remaining: u16,
        tubes: &[u16; 4],
        counts: &mut [u16; 4],
        best: &mut Option<[u16; 4]>,
    ) {
        if index < 0 {
            if remaining == 0 {
                let candidate = *counts;
                let replace = match best {
                    None => true,
                    Some(current) => {
                        let candidate_total: u32 = candidate.iter().map(|&n| n as u32).sum();
                        let current_total: u32 = current.iter().map(|&n| n as u32).sum();
                        candidate_total < current_total
                            || (candidate_total == current_total
                                && (0..4).rev().find_map(|i| {
                                    if candidate[i] != current[i] {
                                        Some(candidate[i] > current[i])
                                    } else {
                                        None
                                    }
                                }).unwrap_or(false))
                    }
                };
                if replace {
                    *best = Some(candidate);
                }
            }
            return;
        }

        let i = index as usize;
        let denomination = DENOMINATIONS[i];
        let max_take = tubes[i].min(remaining / denomination);
        for take in 0..=max_take {
            counts[i] = take;
            Self::search_change(
                index - 1,
                remaining - take * denomination,
                tubes,
                counts,
                best,
            );
        }
        counts[i] = 0;
    }

    fn counts_to_descending(counts: [u16; 4]) -> Vec<u16> {
        let capacity: usize = counts.iter().map(|&n| n as usize).sum();
        let mut coins = Vec::with_capacity(capacity);
        for index in (0..4).rev() {
            for _ in 0..counts[index] {
                coins.push(DENOMINATIONS[index]);
            }
        }
        coins
    }
}