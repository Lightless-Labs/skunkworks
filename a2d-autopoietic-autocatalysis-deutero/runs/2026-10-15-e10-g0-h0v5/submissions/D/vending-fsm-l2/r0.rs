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

        let mut seen = Vec::<String>::new();
        for product in &products {
            let valid_id = !product.id.is_empty()
                && product.id.len() <= 8
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

        let denominations = [1u16, 3, 4, 10];
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
        match coin {
            1 | 3 | 4 | 10 => {}
            _ => return Err(Error::BadCoin { coin }),
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
        let selected_id = match self.selected.as_ref() {
            Some(id) => id,
            None => return Err(Error::NoSelection),
        };

        let product = match self.products.iter().find(|product| product.id == *selected_id) {
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
        let original_tubes = self.tubes;
        let denominations = [1u16, 3, 4, 10];

        let mut change_counts = [0u16; 4];
        let mut remainder = amount;
        for index in (0..4).rev() {
            let coin = denominations[index];
            let take = original_tubes[index].min(remainder / coin);
            change_counts[index] = take;
            remainder -= take * coin;
        }

        if remainder != 0 {
            fn search(
                index: usize,
                remaining: u16,
                counts: &mut [u16; 4],
                tubes: &[u16; 4],
                denominations: &[u16; 4],
                best: &mut Option<[u16; 4]>,
                best_total: &mut u32,
            ) {
                if index == 4 {
                    if remaining != 0 {
                        return;
                    }

                    let total: u32 = counts.iter().map(|&count| u32::from(count)).sum();
                    let replace = match best {
                        None => true,
                        Some(current) if total < *best_total => true,
                        Some(current) if total > *best_total => false,
                        Some(current) => {
                            let mut greater = false;
                            for i in (0..4).rev() {
                                if counts[i] != current[i] {
                                    greater = counts[i] > current[i];
                                    break;
                                }
                            }
                            greater
                        }
                    };

                    if replace {
                        *best = Some(*counts);
                        *best_total = total;
                    }
                    return;
                }

                let coin = denominations[index];
                let maximum = tubes[index].min(remaining / coin);
                for count in 0..=maximum {
                    counts[index] = count;
                    search(
                        index + 1,
                        remaining - count * coin,
                        counts,
                        tubes,
                        denominations,
                        best,
                        best_total,
                    );
                }
                counts[index] = 0;
            }

            let mut best = None;
            let mut best_total = u32::MAX;
            search(
                0,
                amount,
                &mut [0; 4],
                &original_tubes,
                &denominations,
                &mut best,
                &mut best_total,
            );

            match best {
                Some(counts) => change_counts = counts,
                None => return Err(Error::NoChange { amount }),
            }
        }

        let mut proposed_tubes = original_tubes;
        for index in 0..4 {
            proposed_tubes[index] -= change_counts[index];
        }

        for &coin in &self.escrow {
            let index = match coin {
                1 => 0,
                3 => 1,
                4 => 2,
                10 => 3,
                _ => continue,
            };
            if proposed_tubes[index] == 100 {
                return Err(Error::TubeLimit { coin });
            }
            proposed_tubes[index] += 1;
        }

        let mut change = Vec::new();
        for index in (0..4).rev() {
            for _ in 0..change_counts[index] {
                change.push(denominations[index]);
            }
        }

        self.tubes = proposed_tubes;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        Ok(Receipt { id, price, change })
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        let mut returned = self.escrow.clone();
        returned.reverse();

        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        Ok(returned)
    }
}