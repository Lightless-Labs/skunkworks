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
    Vended {
        id: String,
        price: u16,
        change: Vec<u16>,
    },
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

const DENOMINATIONS: [u16; 4] = [1, 3, 4, 10];

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut seen = std::collections::HashSet::new();
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
        let denomination_index = DENOMINATIONS
            .iter()
            .position(|&denomination| denomination == coin)
            .ok_or(Error::BadCoin { coin })?;

        let new_credit = u32::from(self.credit) + u32::from(coin);
        if new_credit > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit = new_credit as u16;
        self.escrow.push(coin);
        let _ = denomination_index;
        self.log(EventKind::Inserted {
            coin,
            credit: self.credit,
        });
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
        self.log(EventKind::Selected { id: id.to_owned() });
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let id = self.selected.as_ref().ok_or(Error::NoSelection)?.clone();
        let product_index = self
            .products
            .iter()
            .position(|product| product.id == id)
            .expect("selected product must exist");
        let product = &self.products[product_index];

        if product.stock == 0 {
            return Err(Error::SoldOut { id });
        }

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
            let dispensed = change
                .iter()
                .filter(|&&coin| coin == *denomination)
                .count() as u16;
            proposed_tubes[index] -= dispensed;
        }

        for &coin in &self.escrow {
            let index = DENOMINATIONS
                .iter()
                .position(|&denomination| denomination == coin)
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

        self.products[product_index].stock -= 1;
        self.tubes = proposed_tubes;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        self.log(EventKind::Vended {
            id: id.clone(),
            price,
            change: change.clone(),
        });

        Ok(Receipt { id, price, change })
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
        let Some(index) = self.products.iter().position(|product| product.id == id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if self.credit != 0 {
            return Err(Error::Busy);
        }
        if !(1..=100).contains(&price) {
            return Err(Error::BadPrice { id: id.to_owned() });
        }

        let old = self.products[index].price;
        self.products[index].price = price;
        self.log(EventKind::PriceChanged {
            id: id.to_owned(),
            old,
            new: price,
        });
        Ok(())
    }

    pub fn restock(&mut self, id: &str, count: u16) -> Result<(), Error> {
        let Some(index) = self.products.iter().position(|product| product.id == id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if count == 0 {
            return Err(Error::ZeroRestock);
        }

        let new_stock = u32::from(self.products[index].stock) + u32::from(count);
        if new_stock > 100 {
            return Err(Error::StockLimit { id: id.to_owned() });
        }

        self.products[index].stock = new_stock as u16;
        self.log(EventKind::Restocked {
            id: id.to_owned(),
            added: count,
            stock: new_stock as u16,
        });
        Ok(())
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    fn log(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
    }
}

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_lowercase())
}

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    let descending = [3usize, 2, 1, 0];
    let mut greedy_counts = [0u16; 4];
    let mut remaining = amount;

    for &index in &descending {
        let denomination = DENOMINATIONS[index];
        let count = tubes[index].min(remaining / denomination);
        greedy_counts[index] = count;
        remaining -= count * denomination;
    }

    if remaining == 0 {
        return Some(counts_to_coins(&greedy_counts));
    }

    let mut best: Option<[u16; 4]> = None;
    let mut counts = [0u16; 4];
    search_change(
        amount,
        tubes,
        3,
        &mut counts,
        &mut best,
    );
    best.map(|counts| counts_to_coins(&counts))
}

fn search_change(
    remaining: u16,
    tubes: &[u16; 4],
    index: usize,
    counts: &mut [u16; 4],
    best: &mut Option<[u16; 4]>,
) {
    if index == 0 {
        let denomination = DENOMINATIONS[0];
        if remaining % denomination != 0 {
            return;
        }
        let count = remaining / denomination;
        if count > tubes[0] {
            return;
        }
        counts[0] = count;
        if best
            .as_ref()
            .is_none_or(|current| better_counts(counts, current))
        {
            *best = Some(*counts);
        }
        return;
    }

    let denomination = DENOMINATIONS[index];
    let max_count = tubes[index].min(remaining / denomination);
    for count in 0..=max_count {
        let next_remaining = remaining - count * denomination;
        counts[index] = count;
        search_change(
            next_remaining,
            tubes,
            index - 1,
            counts,
            best,
        );
    }
    counts[index] = 0;
}

fn better_counts(candidate: &[u16; 4], current: &[u16; 4]) -> bool {
    let candidate_total: u32 = candidate.iter().map(|&count| u32::from(count)).sum();
    let current_total: u32 = current.iter().map(|&count| u32::from(count)).sum();
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

fn counts_to_coins(counts: &[u16; 4]) -> Vec<u16> {
    let mut coins = Vec::new();
    for index in (0..4).rev() {
        coins.extend(std::iter::repeat_n(
            DENOMINATIONS[index],
            usize::from(counts[index]),
        ));
    }
    coins
}