use std::collections::HashSet;

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

        let mut seen = HashSet::new();
        for product in &products {
            if !valid_id(&product.id) {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }
            if !seen.insert(product.id.as_str()) {
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
        if denomination_index(coin).is_none() {
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
        let Some(_) = self.products.iter().find(|product| product.id == id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if self.selected.as_deref() == Some(id) {
            return Err(Error::SameSelection);
        }

        self.selected = Some(id.to_owned());
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let Some(selected_id) = self.selected.as_ref() else {
            return Err(Error::NoSelection);
        };
        let product = self
            .products
            .iter()
            .find(|product| product.id == *selected_id)
            .expect("selected products are always in the catalog");

        let price = product.price;
        let id = product.id.clone();
        if self.credit < price {
            return Err(Error::Underpaid {
                price,
                credit: self.credit,
            });
        }

        let amount = self.credit - price;
        let change = choose_change(amount, &self.tubes)
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

        self.tubes = proposed_tubes;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        Ok(Receipt { id, price, change })
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        let returned = self.escrow.iter().rev().copied().collect();
        self.escrow.clear();
        self.selected = None;
        self.credit = 0;
        Ok(returned)
    }
}

const DENOMINATIONS: [u16; 4] = [1, 3, 4, 10];
const DESCENDING_DENOMINATIONS: [u16; 4] = [10, 4, 3, 1];

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_lowercase())
}

fn denomination_index(coin: u16) -> Option<usize> {
    match coin {
        1 => Some(0),
        3 => Some(1),
        4 => Some(2),
        10 => Some(3),
        _ => None,
    }
}

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    let mut remaining = amount;
    let mut greedy_counts = [0_u16; 4];
    for coin in DESCENDING_DENOMINATIONS {
        let index = denomination_index(coin).unwrap();
        let taken = tubes[index].min(remaining / coin);
        greedy_counts[index] = taken;
        remaining -= taken * coin;
    }

    if remaining == 0 {
        return Some(counts_to_descending_coins(&greedy_counts));
    }

    let mut best: Option<[u16; 4]> = None;
    search_combinations(
        0,
        amount,
        tubes,
        &mut [0_u16; 4],
        &mut best,
    );

    best.map(|counts| counts_to_descending_coins(&counts))
}

fn search_combinations(
    position: usize,
    remaining: u16,
    tubes: &[u16; 4],
    current: &mut [u16; 4],
    best: &mut Option<[u16; 4]>,
) {
    if position == DESCENDING_DENOMINATIONS.len() {
        if remaining == 0
            && best
                .as_ref()
                .map_or(true, |existing| is_better(current, existing))
        {
            *best = Some(*current);
        }
        return;
    }

    let coin = DESCENDING_DENOMINATIONS[position];
    let index = denomination_index(coin).unwrap();
    let maximum = tubes[index].min(remaining / coin);
    for count in 0..=maximum {
        current[index] = count;
        search_combinations(
            position + 1,
            remaining - count * coin,
            tubes,
            current,
            best,
        );
    }
    current[index] = 0;
}

fn is_better(candidate: &[u16; 4], existing: &[u16; 4]) -> bool {
    let candidate_total: u32 = candidate.iter().map(|count| u32::from(*count)).sum();
    let existing_total: u32 = existing.iter().map(|count| u32::from(*count)).sum();
    if candidate_total != existing_total {
        return candidate_total < existing_total;
    }

    for coin in DESCENDING_DENOMINATIONS {
        let index = denomination_index(coin).unwrap();
        if candidate[index] != existing[index] {
            return candidate[index] > existing[index];
        }
    }
    false
}

fn counts_to_descending_coins(counts: &[u16; 4]) -> Vec<u16> {
    let total: usize = counts.iter().map(|count| usize::from(*count)).sum();
    let mut coins = Vec::with_capacity(total);
    for coin in DESCENDING_DENOMINATIONS {
        let index = denomination_index(coin).unwrap();
        coins.extend(std::iter::repeat_n(coin, usize::from(counts[index])));
    }
    coins
}