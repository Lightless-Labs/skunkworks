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

        let product = self
            .products
            .iter()
            .find(|product| product.id == selected_id)
            .expect("selected products are always in the catalog");
        let price = product.price;

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
        for (index, denomination) in DENOMINATIONS.iter().enumerate() {
            let used = change.iter().filter(|coin| *coin == denomination).count() as u16;
            proposed_tubes[index] -= used;
        }

        for coin in &self.escrow {
            let index = DENOMINATIONS
                .iter()
                .position(|denomination| denomination == coin)
                .expect("escrow contains only accepted denominations");
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

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    let mut remaining = amount;
    let mut greedy_counts = [0u16; 4];

    for index in (0..DENOMINATIONS.len()).rev() {
        let denomination = DENOMINATIONS[index];
        let take = tubes[index].min(remaining / denomination);
        greedy_counts[index] = take;
        remaining -= take * denomination;
    }

    if remaining == 0 {
        return Some(counts_to_descending_coins(&greedy_counts));
    }

    let target = usize::from(amount);
    let mut best = vec![None::<[u16; 4]>; target + 1];
    best[0] = Some([0; 4]);

    for index in 0..DENOMINATIONS.len() {
        let denomination = usize::from(DENOMINATIONS[index]);
        let available = usize::from(tubes[index]);

        for value in 0..=target {
            let Some(counts) = best[value] else {
                continue;
            };

            for quantity in 1..=available {
                let next_value = value + quantity * denomination;
                if next_value > target {
                    break;
                }

                let mut candidate = counts;
                candidate[index] += quantity as u16;

                let replace = match best[next_value] {
                    None => true,
                    Some(current) => is_better_combination(&candidate, &current),
                };

                if replace {
                    best[next_value] = Some(candidate);
                }
            }
        }
    }

    best[target].map(|counts| counts_to_descending_coins(&counts))
}

fn is_better_combination(candidate: &[u16; 4], current: &[u16; 4]) -> bool {
    let candidate_total: u32 = candidate.iter().map(|count| u32::from(*count)).sum();
    let current_total: u32 = current.iter().map(|count| u32::from(*count)).sum();

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
    for index in (0..DENOMINATIONS.len()).rev() {
        for _ in 0..counts[index] {
            coins.push(DENOMINATIONS[index]);
        }
    }
    coins
}