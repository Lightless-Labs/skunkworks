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

        self.escrow.push(coin);
        self.credit += coin;
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
        let Some(selected_id) = self.selected.as_deref() else {
            return Err(Error::NoSelection);
        };

        let product = self
            .products
            .iter()
            .find(|product| product.id == selected_id)
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
        for (index, &denomination) in DENOMINATIONS.iter().enumerate() {
            let used = change
                .iter()
                .filter(|&&coin| coin == denomination)
                .count() as u16;
            proposed_tubes[index] -= used;
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

        self.tubes = proposed_tubes;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        Ok(Receipt {
            id,
            price,
            change,
        })
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        let returned = self.escrow.iter().rev().copied().collect();
        self.escrow.clear();
        self.credit = 0;
        self.selected = None;
        Ok(returned)
    }
}

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_lowercase())
}

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    let mut greedy_counts = [0u16; 4];
    let mut remaining = amount;

    for index in (0..4).rev() {
        let denomination = DENOMINATIONS[index];
        let count = tubes[index].min(remaining / denomination);
        greedy_counts[index] = count;
        remaining -= count * denomination;
    }

    if remaining == 0 {
        return Some(counts_to_descending_coins(&greedy_counts));
    }

    let mut best: Option<[u16; 4]> = None;
    let mut candidate = [0u16; 4];
    search_change(3, amount, tubes, &mut candidate, &mut best);

    best.map(|counts| counts_to_descending_coins(&counts))
}

fn search_change(
    index: isize,
    remaining: u16,
    tubes: &[u16; 4],
    candidate: &mut [u16; 4],
    best: &mut Option<[u16; 4]>,
) {
    if index < 0 {
        if remaining == 0 && is_better(candidate, best.as_ref()) {
            *best = Some(*candidate);
        }
        return;
    }

    let index = index as usize;
    let denomination = DENOMINATIONS[index];
    let max_count = tubes[index].min(remaining / denomination);

    for count in 0..=max_count {
        candidate[index] = count;
        let next_remaining = remaining - count * denomination;

        if let Some(current_best) = best.as_ref() {
            let used: u32 = candidate.iter().map(|&value| u32::from(value)).sum();
            let lower_bound = u32::from(next_remaining.div_ceil(10));
            let best_total: u32 = current_best.iter().map(|&value| u32::from(value)).sum();
            if used + lower_bound > best_total {
                continue;
            }
        }

        search_change(index as isize - 1, next_remaining, tubes, candidate, best);
    }

    candidate[index] = 0;
}

fn is_better(candidate: &[u16; 4], best: Option<&[u16; 4]>) -> bool {
    let Some(best) = best else {
        return true;
    };

    let candidate_total: u32 = candidate.iter().map(|&count| u32::from(count)).sum();
    let best_total: u32 = best.iter().map(|&count| u32::from(count)).sum();

    if candidate_total != best_total {
        return candidate_total < best_total;
    }

    for index in (0..4).rev() {
        if candidate[index] != best[index] {
            return candidate[index] > best[index];
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