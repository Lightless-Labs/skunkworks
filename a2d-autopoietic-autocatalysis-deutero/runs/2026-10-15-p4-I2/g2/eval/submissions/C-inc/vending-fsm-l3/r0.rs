#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Product {
    pub id: String,
    pub price: u16,
    pub stock: u16,
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
    StockLimit { id: String },
    SoldOut { id: String },
    Busy,
    ZeroRestock,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    Inserted { coin: u16, credit: u16 },
    Selected { id: String },
    Vended { id: String, price: u16, change: Vec<u16> },
    Refunded { coins: Vec<u16> },
    PriceChanged { id: String, old: u16, new: u16 },
    Restocked { id: String, added: u16, stock: u16 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub seq: usize,
    pub kind: EventKind,
}

pub struct Machine {
    products: Vec<Product>,
    credit: u16,
    selected: Option<String>,
    escrow: Vec<u16>,
    tubes: [u16; 4],
    events: Vec<Event>,
}

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut seen = Vec::<String>::new();
        for product in &products {
            if !valid_id(&product.id) {
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
            if product.stock > 100 {
                return Err(Error::StockLimit {
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
            events: Vec::new(),
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
        if !is_denomination(coin) {
            return Err(Error::BadCoin { coin });
        }
        let new_credit = self.credit as u32 + coin as u32;
        if new_credit > 100 {
            return Err(Error::CreditLimit);
        }
        let credit = new_credit as u16;
        self.escrow.push(coin);
        self.credit = credit;
        self.log(EventKind::Inserted { coin, credit });
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        let Some(index) = self.product_index(id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if self.selected.as_deref() == Some(self.products[index].id.as_str()) {
            return Err(Error::SameSelection);
        }
        let selected_id = self.products[index].id.clone();
        self.selected = Some(selected_id.clone());
        self.log(EventKind::Selected { id: selected_id });
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let Some(id) = self.selected.clone() else {
            return Err(Error::NoSelection);
        };
        let index = self
            .product_index(&id)
            .expect("selected product is always in the catalog");
        let product = &self.products[index];

        if product.stock == 0 {
            return Err(Error::SoldOut { id });
        }
        if self.credit < product.price {
            return Err(Error::Underpaid {
                price: product.price,
                credit: self.credit,
            });
        }

        let price = product.price;
        let amount = self.credit - price;
        let change = match choose_change(amount, &self.tubes) {
            Some(change) => change,
            None => return Err(Error::NoChange { amount }),
        };

        let mut proposed_tubes = self.tubes;
        for (i, count) in change.counts.iter().enumerate() {
            proposed_tubes[i] -= count;
        }
        for &coin in &self.escrow {
            let i = denomination_index(coin).expect("escrow contains accepted denominations");
            let proposed = proposed_tubes[i] as u32 + 1;
            if proposed > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
                });
            }
            proposed_tubes[i] = proposed as u16;
        }

        let change_coins = change.coins;
        self.tubes = proposed_tubes;
        self.products[index].stock -= 1;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.log(EventKind::Vended {
            id: id.clone(),
            price,
            change: change_coins.clone(),
        });

        Ok(Receipt {
            id,
            price,
            change: change_coins,
        })
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        let coins: Vec<u16> = self.escrow.iter().rev().copied().collect();
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.log(EventKind::Refunded {
            coins: coins.clone(),
        });
        Ok(coins)
    }

    pub fn products(&self) -> Vec<Product> {
        let mut products = self.products.clone();
        products.sort_by(|a, b| a.id.as_bytes().cmp(b.id.as_bytes()));
        products
    }

    pub fn set_price(&mut self, id: &str, price: u16) -> Result<(), Error> {
        let Some(index) = self.product_index(id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if self.credit != 0 {
            return Err(Error::Busy);
        }
        if !(1..=100).contains(&price) {
            return Err(Error::BadPrice {
                id: self.products[index].id.clone(),
            });
        }

        let product_id = self.products[index].id.clone();
        let old = self.products[index].price;
        self.products[index].price = price;
        self.log(EventKind::PriceChanged {
            id: product_id,
            old,
            new: price,
        });
        Ok(())
    }

    pub fn restock(&mut self, id: &str, count: u16) -> Result<(), Error> {
        let Some(index) = self.product_index(id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if count == 0 {
            return Err(Error::ZeroRestock);
        }

        let stock = self.products[index].stock as u32 + count as u32;
        if stock > 100 {
            return Err(Error::StockLimit {
                id: self.products[index].id.clone(),
            });
        }

        let stock = stock as u16;
        let product_id = self.products[index].id.clone();
        self.products[index].stock = stock;
        self.log(EventKind::Restocked {
            id: product_id,
            added: count,
            stock,
        });
        Ok(())
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    fn product_index(&self, id: &str) -> Option<usize> {
        self.products.iter().position(|product| product.id == id)
    }

    fn log(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
    }
}

const DENOMINATIONS: [u16; 4] = [1, 3, 4, 10];

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_lowercase())
}

fn is_denomination(coin: u16) -> bool {
    denomination_index(coin).is_some()
}

fn denomination_index(coin: u16) -> Option<usize> {
    DENOMINATIONS.iter().position(|&value| value == coin)
}

struct Change {
    counts: [u16; 4],
    coins: Vec<u16>,
}

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<Change> {
    if amount == 0 {
        return Some(Change {
            counts: [0; 4],
            coins: Vec::new(),
        });
    }

    let mut greedy = [0u16; 4];
    let mut remaining = amount as u32;
    for index in (0..4).rev() {
        let denomination = DENOMINATIONS[index] as u32;
        let take = (tubes[index] as u32).min(remaining / denomination);
        greedy[index] = take as u16;
        remaining -= take * denomination;
    }

    if remaining == 0 {
        return Some(Change {
            coins: counts_to_descending_coins(&greedy),
            counts: greedy,
        });
    }

    let mut best: Option<[u16; 4]> = None;
    let mut candidate = [0u16; 4];
    search_change(
        3,
        amount as i32,
        tubes,
        &mut candidate,
        &mut best,
    )?;

    let counts = best?;
    Some(Change {
        coins: counts_to_descending_coins(&counts),
        counts,
    })
}

fn search_change(
    index: isize,
    remaining: i32,
    tubes: &[u16; 4],
    candidate: &mut [u16; 4],
    best: &mut Option<[u16; 4]>,
) -> Option<()> {
    if index < 0 {
        if remaining == 0 && is_better(candidate, best.as_ref()) {
            *best = Some(*candidate);
        }
        return Some(());
    }

    let i = index as usize;
    let denomination = DENOMINATIONS[i] as i32;
    let max_take = (tubes[i] as i32).min(remaining / denomination);
    for take in 0..=max_take {
        candidate[i] = take as u16;
        let next_remaining = remaining - take * denomination;
        if let Some(current_best) = best.as_ref() {
            let lower_bound = min_coins_for_remainder(next_remaining, i);
            let current_total: u32 = candidate.iter().map(|&count| count as u32).sum();
            let best_total: u32 = current_best.iter().map(|&count| count as u32).sum();
            if lower_bound.is_some_and(|bound| current_total + bound > best_total) {
                continue;
            }
        }
        search_change(index - 1, next_remaining, tubes, candidate, best)?;
    }
    candidate[i] = 0;
    Some(())
}

fn min_coins_for_remainder(remaining: i32, next_index: usize) -> Option<u32> {
    if remaining < 0 {
        return None;
    }
    if remaining == 0 {
        return Some(0);
    }

    let mut dp = vec![u32::MAX; remaining as usize + 1];
    dp[0] = 0;
    for value in 1..=remaining as usize {
        for i in 0..=next_index {
            let denomination = DENOMINATIONS[i] as usize;
            if denomination <= value && dp[value - denomination] != u32::MAX {
                dp[value] = dp[value].min(dp[value - denomination] + 1);
            }
        }
    }
    (dp[remaining as usize] != u32::MAX).then_some(dp[remaining as usize])
}

fn is_better(candidate: &[u16; 4], current: Option<&[u16; 4]>) -> bool {
    let Some(current) = current else {
        return true;
    };
    let candidate_total: u32 = candidate.iter().map(|&count| count as u32).sum();
    let current_total: u32 = current.iter().map(|&count| count as u32).sum();
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
    let total: usize = counts.iter().map(|&count| count as usize).sum();
    let mut coins = Vec::with_capacity(total);
    for index in (0..4).rev() {
        coins.extend(std::iter::repeat_n(DENOMINATIONS[index], counts[index] as usize));
    }
    coins
}

fn main() {}