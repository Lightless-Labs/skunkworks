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

        for (i, &count) in tubes.iter().enumerate() {
            if count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
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
        let Some(selected_id) = self.selected.as_deref() else {
            return Err(Error::NoSelection);
        };
        let product = self
            .products
            .iter()
            .find(|product| product.id == selected_id)
            .expect("selected products always belong to the catalog");

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
        for &coin in &change {
            let index = denomination_index(coin).expect("change uses valid denominations");
            proposed_tubes[index] -= 1;
        }
        for &coin in &self.escrow {
            let index = denomination_index(coin).expect("escrow uses valid denominations");
            proposed_tubes[index] += 1;
        }
        for (i, &count) in proposed_tubes.iter().enumerate() {
            if count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
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
        let coins = self.escrow.iter().rev().copied().collect();
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        Ok(coins)
    }
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

    let mut best: Option<([u16; 4], u16)> = None;
    let mut counts = [0u16; 4];
    search_change(
        3,
        amount,
        0,
        tubes,
        &mut counts,
        &mut best,
    );

    best.map(|(best_counts, _)| counts_to_descending_coins(&best_counts))
}

fn search_change(
    index: isize,
    remaining: u16,
    used: u16,
    tubes: &[u16; 4],
    counts: &mut [u16; 4],
    best: &mut Option<([u16; 4], u16)>,
) {
    if remaining == 0 {
        let replace = match best {
            None => true,
            Some((best_counts, best_used)) => {
                used < *best_used
                    || (used == *best_used
                        && (3..=0)
                            .rev()
                            .find_map(|i| {
                                let i = i as usize;
                                (counts[i] != best_counts[i])
                                    .then_some(counts[i] > best_counts[i])
                            })
                            .unwrap_or(false))
            }
        };
        if replace {
            *best = Some((*counts, used));
        }
        return;
    }

    if index < 0 {
        return;
    }

    if let Some((_, best_used)) = best {
        if used >= *best_used {
            return;
        }
    }

    let i = index as usize;
    let denomination = DENOMINATIONS[i];
    let max_count = tubes[i].min(remaining / denomination);
    for count in (0..=max_count).rev() {
        counts[i] = count;
        search_change(
            index - 1,
            remaining - count * denomination,
            used + count,
            tubes,
            counts,
            best,
        );
    }
    counts[i] = 0;
}

fn counts_to_descending_coins(counts: &[u16; 4]) -> Vec<u16> {
    let mut coins = Vec::new();
    for i in (0..4).rev() {
        for _ in 0..counts[i] {
            coins.push(DENOMINATIONS[i]);
        }
    }
    coins
}