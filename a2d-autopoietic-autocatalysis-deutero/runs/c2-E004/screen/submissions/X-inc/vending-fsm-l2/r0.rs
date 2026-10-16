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
        if self.credit + coin > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit += coin;
        self.escrow.push(coin);
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        let Some(product) = self.products.iter().find(|product| product.id == id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };

        if self.selected.as_deref() == Some(product.id.as_str()) {
            return Err(Error::SameSelection);
        }

        self.selected = Some(product.id.clone());
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let Some(selected_id) = self.selected.as_ref() else {
            return Err(Error::NoSelection);
        };
        let Some(product) = self.products.iter().find(|p| p.id == *selected_id) else {
            return Err(Error::NoSelection);
        };

        if self.credit < product.price {
            return Err(Error::Underpaid {
                price: product.price,
                credit: self.credit,
            });
        }

        let amount = self.credit - product.price;
        let change = match choose_change(amount, &self.tubes) {
            Some(coins) => coins,
            None => return Err(Error::NoChange { amount }),
        };

        let mut proposed_tubes = self.tubes;
        for coin in &change {
            let index = denomination_index(*coin).expect("change coin is a denomination");
            proposed_tubes[index] -= 1;
        }
        for coin in &self.escrow {
            let index = denomination_index(*coin).expect("escrow coin is a denomination");
            proposed_tubes[index] += 1;
        }

        for (index, count) in proposed_tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[index],
                });
            }
        }

        let receipt = Receipt {
            id: product.id.clone(),
            price: product.price,
            change,
        };

        self.tubes = proposed_tubes;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        Ok(receipt)
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        let coins = self.escrow.iter().rev().copied().collect();
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        Ok(coins)
    }
}

fn denomination_index(coin: u16) -> Option<usize> {
    DENOMINATIONS.iter().position(|&denomination| denomination == coin)
}

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<Vec<u16>> {
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
        return Some(counts_to_descending_coins(&greedy_counts));
    }

    let target = amount as usize;
    let mut best: Option<[u16; 4]> = None;
    let mut counts = [0u16; 4];
    search_change(0, target, tubes, &mut counts, &mut best);
    best.map(|chosen| counts_to_descending_coins(&chosen))
}

fn search_change(
    index: usize,
    remaining: usize,
    tubes: &[u16; 4],
    counts: &mut [u16; 4],
    best: &mut Option<[u16; 4]>,
) {
    if index == 4 {
        if remaining == 0 && best.as_ref().is_none_or(|current| is_better(counts, current)) {
            *best = Some(*counts);
        }
        return;
    }

    let coin = DENOMINATIONS[index] as usize;
    let max_take = tubes[index].min((remaining / coin) as u16);
    for take in 0..=max_take {
        counts[index] = take;
        search_change(
            index + 1,
            remaining - take as usize * coin,
            tubes,
            counts,
            best,
        );
    }
    counts[index] = 0;
}

fn is_better(candidate: &[u16; 4], current: &[u16; 4]) -> bool {
    let candidate_total: u32 = candidate.iter().map(|&n| n as u32).sum();
    let current_total: u32 = current.iter().map(|&n| n as u32).sum();

    if candidate_total != current_total {
        return candidate_total < current_total;
    }

    for index in (0..4).rev() {
        if candidate[index] != current[index] {
            return candidate[index] > current[index];
        }
    }
    false
}

fn counts_to_descending_coins(counts: &[u16; 4]) -> Vec<u16> {
    let mut coins = Vec::new();
    for index in (0..4).rev() {
        for _ in 0..counts[index] {
            coins.push(DENOMINATIONS[index]);
        }
    }
    coins
}