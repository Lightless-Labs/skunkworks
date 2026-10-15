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

fn denomination_index(coin: u16) -> Option<usize> {
    DENOMINATIONS.iter().position(|&d| d == coin)
}

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase())
}

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut seen: Vec<&str> = Vec::new();

        for product in &products {
            if !valid_id(&product.id) {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }

            if seen.iter().any(|&id| id == product.id) {
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
        if denomination_index(coin).is_none() {
            return Err(Error::BadCoin { coin });
        }

        if self.credit as u32 + coin as u32 > 100 {
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

        let product = self
            .products
            .iter()
            .find(|product| product.id == selected_id)
            .expect("selected product must be in the catalog");

        let price = product.price;
        if self.credit < price {
            return Err(Error::Underpaid {
                price,
                credit: self.credit,
            });
        }

        let amount = self.credit - price;
        let change = match choose_change(amount, self.tubes) {
            Some(change) => change,
            None => return Err(Error::NoChange { amount }),
        };

        let mut proposed_tubes = self.tubes;
        for (index, &coin) in DENOMINATIONS.iter().enumerate() {
            let count = change.iter().filter(|&&c| c == coin).count() as u16;
            proposed_tubes[index] -= count;
        }

        for &coin in &self.escrow {
            let index = denomination_index(coin).expect("escrow only contains accepted coins");
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

fn choose_change(amount: u16, tubes: [u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    let mut remaining = amount;
    let mut greedy_counts = [0u16; 4];

    for index in (0..4).rev() {
        let coin = DENOMINATIONS[index];
        let take = tubes[index].min(remaining / coin);
        greedy_counts[index] = take;
        remaining -= take * coin;
    }

    if remaining == 0 {
        return Some(counts_to_descending_coins(greedy_counts));
    }

    let mut best: Option<[u16; 4]> = None;
    let mut best_count = u16::MAX;
    let max_tens = tubes[3].min(amount / 10);

    for tens in 0..=max_tens {
        let after_tens = amount - tens * 10;
        let max_fours = tubes[2].min(after_tens / 4);

        for fours in 0..=max_fours {
            let after_fours = after_tens - fours * 4;
            let max_threes = tubes[1].min(after_fours / 3);

            for threes in 0..=max_threes {
                let after_threes = after_fours - threes * 3;
                if after_threes > tubes[0] {
                    continue;
                }

                let candidate = [after_threes, threes, fours, tens];
                let count = candidate.iter().copied().sum::<u16>();

                let replace = match best {
                    None => true,
                    Some(current) => {
                        count < best_count
                            || (count == best_count
                                && lexicographically_greater_descending(candidate, current))
                    }
                };

                if replace {
                    best = Some(candidate);
                    best_count = count;
                }
            }
        }
    }

    best.map(counts_to_descending_coins)
}

fn lexicographically_greater_descending(a: [u16; 4], b: [u16; 4]) -> bool {
    for index in (0..4).rev() {
        if a[index] != b[index] {
            return a[index] > b[index];
        }
    }
    false
}

fn counts_to_descending_coins(counts: [u16; 4]) -> Vec<u16> {
    let mut coins = Vec::new();
    for index in (0..4).rev() {
        for _ in 0..counts[index] {
            coins.push(DENOMINATIONS[index]);
        }
    }
    coins
}