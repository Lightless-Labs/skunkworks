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
                && product
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase());

            if !valid_id {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }

            if seen.iter().any(|id| *id == product.id) {
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
        let index = match DENOMINATIONS.iter().position(|&denomination| denomination == coin) {
            Some(index) => index,
            None => return Err(Error::BadCoin { coin }),
        };

        let new_credit = u32::from(self.credit) + u32::from(coin);
        if new_credit > 100 {
            return Err(Error::CreditLimit);
        }

        self.escrow.push(DENOMINATIONS[index]);
        self.credit = new_credit as u16;
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
        if self.credit < price {
            return Err(Error::Underpaid {
                price,
                credit: self.credit,
            });
        }

        let amount = self.credit - price;
        let counts = match choose_change(amount, self.tubes) {
            Some(counts) => counts,
            None => return Err(Error::NoChange { amount }),
        };

        let mut proposed_tubes = self.tubes;
        for index in 0..4 {
            proposed_tubes[index] -= counts[index];
        }

        for &coin in &self.escrow {
            let index = DENOMINATIONS
                .iter()
                .position(|&denomination| denomination == coin)
                .expect("escrow contains only accepted denominations");
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
            for _ in 0..counts[index] {
                change.push(DENOMINATIONS[index]);
            }
        }

        let receipt = Receipt {
            id: selected_id.clone(),
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

fn choose_change(amount: u16, tubes: [u16; 4]) -> Option<[u16; 4]> {
    let mut greedy = [0u16; 4];
    let mut remainder = amount;

    for index in (0..4).rev() {
        let denomination = DENOMINATIONS[index];
        let take = tubes[index].min(remainder / denomination);
        greedy[index] = take;
        remainder -= take * denomination;
    }

    if remainder == 0 {
        return Some(greedy);
    }

    let mut best: Option<[u16; 4]> = None;
    let mut best_total = u32::MAX;

    for c10 in 0..=tubes[3].min(amount / 10) {
        let after10 = amount - c10 * 10;
        for c4 in 0..=tubes[2].min(after10 / 4) {
            let after4 = after10 - c4 * 4;
            for c3 in 0..=tubes[1].min(after4 / 3) {
                let after3 = after4 - c3 * 3;
                if after3 > tubes[0] {
                    continue;
                }

                let candidate = [after3, c3, c4, c10];
                let total = u32::from(candidate[0])
                    + u32::from(candidate[1])
                    + u32::from(candidate[2])
                    + u32::from(candidate[3]);

                let replace = match best {
                    None => true,
                    Some(current) => {
                        total < best_total
                            || (total == best_total
                                && (candidate[3], candidate[2], candidate[1], candidate[0])
                                    > (current[3], current[2], current[1], current[0]))
                    }
                };

                if replace {
                    best = Some(candidate);
                    best_total = total;
                }
            }
        }
    }

    best
}