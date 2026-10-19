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
            if seen.iter().any(|&id| id == product.id) {
                return Err(Error::DuplicateId {
                    id: product.id.clone(),
                });
            }
            if !(1..=100).contains(&product.price) {
                return Err(Error::BadPrice {
                    id: product.id.clone(),
                });
            }
            seen.push(&product.id);
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
        let product = match self.products.iter().find(|p| p.id == selected_id) {
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
        let change = match choose_change(amount, &self.tubes) {
            Some(change) => change,
            None => return Err(Error::NoChange { amount }),
        };

        let mut proposed_tubes = self.tubes;
        for (index, &count) in change.iter().enumerate() {
            proposed_tubes[index] -= count;
        }
        for &coin in &self.escrow {
            if let Some(index) = DENOMINATIONS.iter().position(|&d| d == coin) {
                proposed_tubes[index] += 1;
            }
        }

        for (index, &count) in proposed_tubes.iter().enumerate() {
            if count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[index],
                });
            }
        }

        let mut change_coins = Vec::new();
        for index in (0..DENOMINATIONS.len()).rev() {
            for _ in 0..change[index] {
                change_coins.push(DENOMINATIONS[index]);
            }
        }

        self.tubes = proposed_tubes;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        Ok(Receipt {
            id: selected_id,
            price,
            change: change_coins,
        })
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        let returned = self.escrow.iter().rev().copied().collect();
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        Ok(returned)
    }
}

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<[u16; 4]> {
    let mut remaining = amount;
    let mut greedy = [0u16; 4];

    for index in (0..DENOMINATIONS.len()).rev() {
        let denomination = DENOMINATIONS[index];
        let count = tubes[index].min(remaining / denomination);
        greedy[index] = count;
        remaining -= count * denomination;
    }

    if remaining == 0 {
        return Some(greedy);
    }

    let mut best: Option<[u16; 4]> = None;
    let mut candidate = [0u16; 4];
    search_change(0, amount, tubes, &mut candidate, &mut best);
    best
}

fn search_change(
    index: usize,
    remaining: u16,
    tubes: &[u16; 4],
    candidate: &mut [u16; 4],
    best: &mut Option<[u16; 4]>,
) {
    if index == DENOMINATIONS.len() {
        if remaining == 0 {
            let replace = match best {
                None => true,
                Some(current) => {
                    let candidate_total: u32 = candidate.iter().map(|&n| u32::from(n)).sum();
                    let current_total: u32 = current.iter().map(|&n| u32::from(n)).sum();
                    candidate_total < current_total
                        || (candidate_total == current_total
                            && lex_greater_descending(candidate, current))
                }
            };
            if replace {
                *best = Some(*candidate);
            }
        }
        return;
    }

    let denomination = DENOMINATIONS[index];
    let max_count = tubes[index].min(remaining / denomination);
    for count in 0..=max_count {
        candidate[index] = count;
        search_change(
            index + 1,
            remaining - count * denomination,
            tubes,
            candidate,
            best,
        );
    }
    candidate[index] = 0;
}

fn lex_greater_descending(left: &[u16; 4], right: &[u16; 4]) -> bool {
    for index in (0..4).rev() {
        if left[index] != right[index] {
            return left[index] > right[index];
        }
    }
    false
}