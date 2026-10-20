use std::collections::HashMap;

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
    products: HashMap<String, u16>,
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

        let mut catalog = HashMap::new();
        for product in products {
            let valid_id = !product.id.is_empty()
                && product.id.len() <= 8
                && product.id.bytes().all(|b| b.is_ascii_lowercase());
            if !valid_id {
                return Err(Error::BadId { id: product.id });
            }
            if catalog.contains_key(&product.id) {
                return Err(Error::DuplicateId { id: product.id });
            }
            if !(1..=100).contains(&product.price) {
                return Err(Error::BadPrice { id: product.id });
            }
            catalog.insert(product.id, product.price);
        }

        for (index, count) in tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[index],
                });
            }
        }

        Ok(Self {
            products: catalog,
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
        let Some(index) = DENOMINATIONS.iter().position(|&d| d == coin) else {
            return Err(Error::BadCoin { coin });
        };
        let _ = index;

        if self.credit + coin > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit += coin;
        self.escrow.push(coin);
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        if !self.products.contains_key(id) {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        }
        if self.selected.as_deref() == Some(id) {
            return Err(Error::SameSelection);
        }
        self.selected = Some(id.to_owned());
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let id = self.selected.as_ref().ok_or(Error::NoSelection)?;
        let price = *self.products.get(id).expect("selected product is in catalog");
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
        for &coin in &change {
            let index = denomination_index(coin).expect("change uses valid denominations");
            proposed_tubes[index] -= 1;
        }
        for &coin in &self.escrow {
            let index = denomination_index(coin).expect("escrow contains valid denominations");
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
            id: id.clone(),
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

fn denomination_index(coin: u16) -> Option<usize> {
    DENOMINATIONS.iter().position(|&d| d == coin)
}

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    let descending_indices = [3usize, 2, 1, 0];
    let mut remaining = amount;
    let mut greedy_counts = [0u16; 4];

    for &index in &descending_indices {
        let coin = DENOMINATIONS[index];
        let take = tubes[index].min(remaining / coin);
        greedy_counts[index] = take;
        remaining -= take * coin;
    }

    if remaining == 0 {
        return Some(counts_to_descending_coins(&greedy_counts));
    }

    let mut best: Option<[u16; 4]> = None;
    let mut candidate = [0u16; 4];
    search_combinations(
        amount,
        tubes,
        &descending_indices,
        0,
        &mut candidate,
        &mut best,
    );

    best.map(|counts| counts_to_descending_coins(&counts))
}

fn search_combinations(
    remaining: u16,
    tubes: &[u16; 4],
    order: &[usize; 4],
    position: usize,
    candidate: &mut [u16; 4],
    best: &mut Option<[u16; 4]>,
) {
    if position == order.len() {
        if remaining == 0 && is_better(candidate, best.as_ref()) {
            *best = Some(*candidate);
        }
        return;
    }

    let index = order[position];
    let coin = DENOMINATIONS[index];
    let max_take = tubes[index].min(remaining / coin);

    for take in (0..=max_take).rev() {
        candidate[index] = take;
        search_combinations(
            remaining - take * coin,
            tubes,
            order,
            position + 1,
            candidate,
            best,
        );
    }
    candidate[index] = 0;
}

fn is_better(candidate: &[u16; 4], best: Option<&[u16; 4]>) -> bool {
    let Some(best) = best else {
        return true;
    };

    let candidate_total: u32 = candidate.iter().map(|&n| u32::from(n)).sum();
    let best_total: u32 = best.iter().map(|&n| u32::from(n)).sum();

    if candidate_total != best_total {
        return candidate_total < best_total;
    }

    for index in [3usize, 2, 1, 0] {
        if candidate[index] != best[index] {
            return candidate[index] > best[index];
        }
    }
    false
}

fn counts_to_descending_coins(counts: &[u16; 4]) -> Vec<u16> {
    let total: usize = counts.iter().map(|&n| usize::from(n)).sum();
    let mut coins = Vec::with_capacity(total);
    for index in [3usize, 2, 1, 0] {
        coins.extend(std::iter::repeat_n(DENOMINATIONS[index], usize::from(counts[index])));
    }
    coins
}