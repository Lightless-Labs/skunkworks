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

        let mut seen = std::collections::HashSet::new();
        for product in &products {
            let valid_id = !product.id.is_empty()
                && product.id.len() <= 8
                && product.id.bytes().all(|b| b.is_ascii_lowercase());
            if !valid_id {
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

        let denominations = [1, 3, 4, 10];
        for i in 0..4 {
            if tubes[i] > 100 {
                return Err(Error::TubeLimit {
                    coin: denominations[i],
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
        if !matches!(coin, 1 | 3 | 4 | 10) {
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
        let selected_id = match &self.selected {
            Some(id) => id.clone(),
            None => return Err(Error::NoSelection),
        };
        let price = self
            .products
            .iter()
            .find(|product| product.id == selected_id)
            .expect("selected product is always in the catalog")
            .price;

        if self.credit < price {
            return Err(Error::Underpaid {
                price,
                credit: self.credit,
            });
        }

        let amount = self.credit - price;
        let mut change_counts = [0u16; 4];
        let denominations = [1u16, 3, 4, 10];
        let mut remaining = amount;

        for i in (0..4).rev() {
            let take = self.tubes[i].min(remaining / denominations[i]);
            change_counts[i] = take;
            remaining -= take * denominations[i];
        }

        if remaining != 0 {
            let mut best: Option<[u16; 4]> = None;
            let mut best_count = u16::MAX;

            fn search(
                index: isize,
                remaining: u16,
                tubes: &[u16; 4],
                denominations: &[u16; 4],
                current: &mut [u16; 4],
                best: &mut Option<[u16; 4]>,
                best_count: &mut u16,
            ) {
                if index < 0 {
                    if remaining == 0 {
                        let count: u16 = current.iter().copied().sum();
                        let is_better = match best {
                            None => true,
                            Some(existing) => {
                                count < *best_count
                                    || (count == *best_count
                                        && (0..4).rev().find_map(|i| {
                                            if current[i] != existing[i] {
                                                Some(current[i] > existing[i])
                                            } else {
                                                None
                                            }
                                        }) == Some(true))
                            }
                        };
                        if is_better {
                            *best = Some(*current);
                            *best_count = count;
                        }
                    }
                    return;
                }

                let i = index as usize;
                let max = tubes[i].min(remaining / denominations[i]);
                for take in 0..=max {
                    current[i] = take;
                    search(
                        index - 1,
                        remaining - take * denominations[i],
                        tubes,
                        denominations,
                        current,
                        best,
                        best_count,
                    );
                }
                current[i] = 0;
            }

            search(
                3,
                amount,
                &self.tubes,
                &denominations,
                &mut [0; 4],
                &mut best,
                &mut best_count,
            );

            match best {
                Some(counts) => change_counts = counts,
                None => return Err(Error::NoChange { amount }),
            }
        }

        let mut proposed_tubes = self.tubes;
        for i in 0..4 {
            proposed_tubes[i] -= change_counts[i];
        }
        for &coin in &self.escrow {
            let i = match coin {
                1 => 0,
                3 => 1,
                4 => 2,
                10 => 3,
                _ => unreachable!(),
            };
            proposed_tubes[i] += 1;
        }

        for i in 0..4 {
            if proposed_tubes[i] > 100 {
                return Err(Error::TubeLimit {
                    coin: denominations[i],
                });
            }
        }

        let mut change = Vec::new();
        for i in (0..4).rev() {
            for _ in 0..change_counts[i] {
                change.push(denominations[i]);
            }
        }

        self.tubes = proposed_tubes;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        Ok(Receipt {
            id: selected_id,
            price,
            change,
        })
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        let refunded = self.escrow.iter().rev().copied().collect();
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        Ok(refunded)
    }
}

fn main() {}