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
        if denomination_index(coin).is_none() {
            return Err(Error::BadCoin { coin });
        }

        let new_credit = self.credit as u32 + coin as u32;
        if new_credit > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit = new_credit as u16;
        self.escrow.push(coin);
        self.log(EventKind::Inserted {
            coin,
            credit: self.credit,
        });
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        if self.product_index(id).is_none() {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        }
        if self.selected.as_deref() == Some(id) {
            return Err(Error::SameSelection);
        }

        self.selected = Some(id.to_owned());
        self.log(EventKind::Selected { id: id.to_owned() });
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let id = self.selected.clone().ok_or(Error::NoSelection)?;
        let index = self
            .product_index(&id)
            .expect("selected product must belong to the catalog");
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
        let change = choose_change(amount, &self.tubes).ok_or(Error::NoChange { amount })?;

        let mut proposed_tubes = self.tubes;
        for coin in &change {
            let tube_index =
                denomination_index(*coin).expect("change contains only supported denominations");
            proposed_tubes[tube_index] -= 1;
        }
        for coin in &self.escrow {
            let tube_index =
                denomination_index(*coin).expect("escrow contains only accepted coins");
            proposed_tubes[tube_index] += 1;
        }
        for (tube_index, count) in proposed_tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[tube_index],
                });
            }
        }

        self.tubes = proposed_tubes;
        self.products[index].stock -= 1;
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
        let index = self
            .product_index(id)
            .ok_or_else(|| Error::UnknownProduct { id: id.to_owned() })?;
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
        let index = self
            .product_index(id)
            .ok_or_else(|| Error::UnknownProduct { id: id.to_owned() })?;
        if count == 0 {
            return Err(Error::ZeroRestock);
        }

        let new_stock = self.products[index].stock as u32 + count as u32;
        if new_stock > 100 {
            return Err(Error::StockLimit { id: id.to_owned() });
        }

        self.products[index].stock = new_stock as u16;
        self.log(EventKind::Restocked {
            id: id.to_owned(),
            added: count,
            stock: self.products[index].stock,
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
        self.events.push(Event {
            seq: self.events.len(),
            kind,
        });
    }
}

const DENOMINATIONS: [u16; 4] = [1, 3, 4, 10];

fn denomination_index(coin: u16) -> Option<usize> {
    DENOMINATIONS
        .iter()
        .position(|denomination| *denomination == coin)
}

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_lowercase())
}

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    let mut remaining = amount as usize;
    let mut greedy_counts = [0usize; 4];
    for index in (0..4).rev() {
        let denomination = DENOMINATIONS[index] as usize;
        let take = (tubes[index] as usize).min(remaining / denomination);
        greedy_counts[index] = take;
        remaining -= take * denomination;
    }

    if remaining == 0 {
        return Some(counts_to_descending(&greedy_counts));
    }

    let target = amount as usize;
    let mut best: Vec<Option<[usize; 4]>> = vec![None; target + 1];
    best[0] = Some([0; 4]);

    for index in (0..4).rev() {
        let denomination = DENOMINATIONS[index] as usize;
        let available = tubes[index] as usize;
        let mut next: Vec<Option<[usize; 4]>> = vec![None; target + 1];

        for value in 0..=target {
            if let Some(current) = best[value] {
                let max_take = available.min((target - value) / denomination);
                for take in 0..=max_take {
                    let new_value = value + take * denomination;
                    let mut candidate = current;
                    candidate[index] += take;

                    let replace = match &next[new_value] {
                        None => true,
                        Some(existing) => {
                            let candidate_count: usize = candidate.iter().sum();
                            let existing_count: usize = existing.iter().sum();
                            candidate_count < existing_count
                                || (candidate_count == existing_count
                                    && lex_greater_descending(&candidate, existing))
                        }
                    };
                    if replace {
                        next[new_value] = Some(candidate);
                    }
                }
            }
        }
        best = next;
    }

    best[target].as_ref().map(counts_to_descending)
}

fn lex_greater_descending(left: &[usize; 4], right: &[usize; 4]) -> bool {
    for index in (0..4).rev() {
        if left[index] != right[index] {
            return left[index] > right[index];
        }
    }
    false
}

fn counts_to_descending(counts: &[usize; 4]) -> Vec<u16> {
    let mut coins = Vec::new();
    for index in (0..4).rev() {
        for _ in 0..counts[index] {
            coins.push(DENOMINATIONS[index]);
        }
    }
    coins
}