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
            if !valid_id(&product.id) {
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
        let Some(product) = self.products.iter().find(|product| product.id == *selected_id) else {
            return Err(Error::NoSelection);
        };

        if self.credit < product.price {
            return Err(Error::Underpaid {
                price: product.price,
                credit: self.credit,
            });
        }

        let amount = self.credit - product.price;
        let change = make_change(amount, &self.tubes)
            .ok_or(Error::NoChange { amount })?;

        let mut proposed_tubes = self.tubes;
        for coin in &change {
            let index = denomination_index(*coin).expect("change uses valid denominations");
            proposed_tubes[index] -= 1;
        }
        for coin in &self.escrow {
            let index = denomination_index(*coin).expect("escrow uses valid denominations");
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
        let refunded = self.escrow.iter().rev().copied().collect();
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        Ok(refunded)
    }
}

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_lowercase())
}

fn denomination_index(coin: u16) -> Option<usize> {
    DENOMINATIONS.iter().position(|denomination| *denomination == coin)
}

fn make_change(amount: u16, tubes: &[u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    let descending_indices = [3usize, 2, 1, 0];
    let mut greedy_counts = [0u16; 4];
    let mut remainder = amount;

    for index in descending_indices {
        let coin = DENOMINATIONS[index];
        let count = tubes[index].min(remainder / coin);
        greedy_counts[index] = count;
        remainder -= count * coin;
    }

    if remainder == 0 {
        return Some(counts_to_descending_coins(&greedy_counts));
    }

    let mut best: Option<[u16; 4]> = None;
    search_combinations(
        amount,
        tubes,
        0,
        &mut [0u16; 4],
        0,
        &mut best,
    );

    best.map(|counts| counts_to_descending_coins(&counts))
}

fn search_combinations(
    remaining: u16,
    tubes: &[u16; 4],
    index: usize,
    counts: &mut [u16; 4],
    total_coins: u16,
    best: &mut Option<[u16; 4]>,
) {
    if index == 4 {
        if remaining == 0 && is_better(counts, total_coins, best) {
            *best = Some(*counts);
        }
        return;
    }

    let coin = DENOMINATIONS[index];
    let max_count = tubes[index].min(remaining / coin);
    for count in 0..=max_count {
        let next_total = total_coins + count;
        if let Some(best_counts) = best {
            let best_total: u16 = best_counts.iter().sum();
            if next_total > best_total {
                continue;
            }
        }
        counts[index] = count;
        search_combinations(
            remaining - count * coin,
            tubes,
            index + 1,
            counts,
            next_total,
            best,
        );
    }
    counts[index] = 0;
}

fn is_better(candidate: &[u16; 4], total_coins: u16, best: &Option<[u16; 4]>) -> bool {
    let Some(best_counts) = best else {
        return true;
    };

    let best_total: u16 = best_counts.iter().sum();
    if total_coins != best_total {
        return total_coins < best_total;
    }

    for index in [3usize, 2, 1, 0] {
        if candidate[index] != best_counts[index] {
            return candidate[index] > best_counts[index];
        }
    }
    false
}

fn counts_to_descending_coins(counts: &[u16; 4]) -> Vec<u16> {
    let mut coins = Vec::new();
    for index in [3usize, 2, 1, 0] {
        coins.extend(std::iter::repeat_n(DENOMINATIONS[index], counts[index] as usize));
    }
    coins
}

fn main() {}