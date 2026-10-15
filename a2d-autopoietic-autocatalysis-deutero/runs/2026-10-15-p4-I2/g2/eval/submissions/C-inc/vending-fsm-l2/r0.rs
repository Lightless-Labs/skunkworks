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
    const DENOMINATIONS: [u16; 4] = [1, 3, 4, 10];

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

        for (i, count) in tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: Self::DENOMINATIONS[i],
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
        if !Self::DENOMINATIONS.contains(&coin) {
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
        let mut chosen = [0u16; 4];
        let mut remainder = amount;

        for i in (0..4).rev() {
            let denomination = Self::DENOMINATIONS[i];
            let count = self.tubes[i].min(remainder / denomination);
            chosen[i] = count;
            remainder -= count * denomination;
        }

        if remainder != 0 {
            let mut best: Option<[u16; 4]> = None;
            let mut best_count = u32::MAX;

            fn search(
                index: isize,
                remainder: u16,
                tubes: &[u16; 4],
                counts: &mut [u16; 4],
                best: &mut Option<[u16; 4]>,
                best_count: &mut u32,
            ) {
                if index < 0 {
                    if remainder == 0 {
                        let total: u32 = counts.iter().map(|&n| u32::from(n)).sum();
                        let better = match best {
                            None => true,
                            Some(current) => {
                                if total < *best_count {
                                    true
                                } else if total > *best_count {
                                    false
                                } else {
                                    let mut decided = false;
                                    let mut greater = false;
                                    for i in (0..4).rev() {
                                        if counts[i] != current[i] {
                                            greater = counts[i] > current[i];
                                            decided = true;
                                            break;
                                        }
                                    }
                                    decided && greater
                                }
                            }
                        };
                        if better {
                            *best = Some(*counts);
                            *best_count = total;
                        }
                    }
                    return;
                }

                let i = index as usize;
                let denomination = Machine::DENOMINATIONS[i];
                let max = tubes[i].min(remainder / denomination);
                for count in 0..=max {
                    counts[i] = count;
                    search(
                        index - 1,
                        remainder - count * denomination,
                        tubes,
                        counts,
                        best,
                        best_count,
                    );
                }
                counts[i] = 0;
            }

            search(
                3,
                amount,
                &self.tubes,
                &mut [0; 4],
                &mut best,
                &mut best_count,
            );

            match best {
                Some(solution) => chosen = solution,
                None => return Err(Error::NoChange { amount }),
            }
        }

        let mut proposed = self.tubes;
        for i in 0..4 {
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
        for i in 0..4 {
            if proposed[i] > 100 {
                return Err(Error::TubeLimit {
                    coin: Self::DENOMINATIONS[i],
                });
            }
        }

        let mut change = Vec::new();
        for i in (0..4).rev() {
            for _ in 0..chosen[i] {
                change.push(Self::DENOMINATIONS[i]);
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

fn main() {}