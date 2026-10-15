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

        let mut seen: Vec<String> = Vec::new();
        for product in &products {
            if product.id.is_empty()
                || product.id.len() > 8
                || !product.id.bytes().all(|byte| byte.is_ascii_lowercase())
            {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }

            if seen.iter().any(|id| id == &product.id) {
                return Err(Error::DuplicateId {
                    id: product.id.clone(),
                });
            }

            if !(1..=100).contains(&product.price) {
                return Err(Error::BadPrice {
                    id: product.id.clone(),
                });
            }

            seen.push(product.id.clone());
        }

        let denominations = [1, 3, 4, 10];
        for (index, count) in tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: denominations[index],
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
        let selected_id = match &self.selected {
            Some(id) => id.clone(),
            None => return Err(Error::NoSelection),
        };

        let price = self
            .products
            .iter()
            .find(|product| product.id == selected_id)
            .map(|product| product.price)
            .ok_or_else(|| Error::UnknownProduct {
                id: selected_id.clone(),
            })?;

        if self.credit < price {
            return Err(Error::Underpaid {
                price,
                credit: self.credit,
            });
        }

        let amount = self.credit - price;
        let denominations = [1u16, 3, 4, 10];
        let descending = [3usize, 2, 1, 0];
        let mut change_counts = [0u16; 4];
        let mut remainder = amount;

        for &index in &descending {
            let coin = denominations[index];
            let count = self.tubes[index].min(remainder / coin);
            change_counts[index] = count;
            remainder -= count * coin;
        }

        if remainder != 0 {
            fn search(
                position: usize,
                remaining: u16,
                tubes: &[u16; 4],
                denominations: &[u16; 4],
                counts: &mut [u16; 4],
                best: &mut Option<[u16; 4]>,
                best_count: &mut u32,
            ) {
                if position == 4 {
                    if remaining == 0 {
                        let total: u32 = counts.iter().map(|&count| u32::from(count)).sum();
                        let better = match best {
                            None => true,
                            Some(current) if total < *best_count => true,
                            Some(current) if total == *best_count => [3usize, 2, 1, 0]
                                .iter()
                                .find_map(|&index| {
                                    (counts[index] != current[index])
                                        .then_some(counts[index] > current[index])
                                })
                                .unwrap_or(false),
                            _ => false,
                        };

                        if better {
                            *best = Some(*counts);
                            *best_count = total;
                        }
                    }
                    return;
                }

                let index = [3usize, 2, 1, 0][position];
                let coin = denominations[index];
                let max_count = tubes[index].min(remaining / coin);

                for count in 0..=max_count {
                    counts[index] = count;
                    search(
                        position + 1,
                        remaining - count * coin,
                        tubes,
                        denominations,
                        counts,
                        best,
                        best_count,
                    );
                }
                counts[index] = 0;
            }

            let mut best = None;
            let mut best_count = u32::MAX;
            search(
                0,
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
        for index in 0..4 {
            proposed_tubes[index] -= change_counts[index];
        }

        for &coin in &self.escrow {
            let index = match coin {
                1 => 0,
                3 => 1,
                4 => 2,
                10 => 3,
                _ => return Err(Error::BadCoin { coin }),
            };
            proposed_tubes[index] += 1;
        }

        for (index, count) in proposed_tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: denominations[index],
                });
            }
        }

        let mut change = Vec::new();
        for &index in &descending {
            for _ in 0..change_counts[index] {
                change.push(denominations[index]);
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
        let coins = self.escrow.iter().rev().copied().collect();
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        Ok(coins)
    }
}

fn main() {}