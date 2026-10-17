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

const DENOMINATIONS: [u16; 4] = [1, 3, 4, 10];

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        for (index, product) in products.iter().enumerate() {
            if !valid_id(&product.id) {
                return Err(Error::BadId {
                    id: product.id.clone(),
                });
            }
            if products[..index].iter().any(|earlier| earlier.id == product.id) {
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

        let new_credit = self.credit as u32 + coin as u32;
        if new_credit > 100 {
            return Err(Error::CreditLimit);
        }

        let credit = new_credit as u16;
        self.escrow.push(coin);
        self.credit = credit;
        let _ = denomination_index;
        self.log(EventKind::Inserted { coin, credit });
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
        self.log(EventKind::Selected { id: id.to_owned() });
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let id = self.selected.as_ref().ok_or(Error::NoSelection)?.clone();
        let product_index = self
            .products
            .iter()
            .position(|product| product.id == id)
            .expect("selected product must belong to catalog");

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
        let change_counts = choose_change(amount, self.tubes)
            .ok_or(Error::NoChange { amount })?;

        let mut proposed_tubes = self.tubes;
        for index in 0..4 {
            proposed_tubes[index] -= change_counts[index];
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

        let change = counts_to_descending(change_counts);
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
        let index = self
            .products
            .iter()
            .position(|product| product.id == id)
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
            .products
            .iter()
            .position(|product| product.id == id)
            .ok_or_else(|| Error::UnknownProduct { id: id.to_owned() })?;

        if count == 0 {
            return Err(Error::ZeroRestock);
        }

        let stock = self.products[index].stock as u32 + count as u32;
        if stock > 100 {
            return Err(Error::StockLimit { id: id.to_owned() });
        }

        self.products[index].stock = stock as u16;
        self.log(EventKind::Restocked {
            id: id.to_owned(),
            added: count,
            stock: stock as u16,
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

fn choose_change(amount: u16, tubes: [u16; 4]) -> Option<[u16; 4]> {
    let mut greedy = [0u16; 4];
    let mut remaining = amount as u32;

    for index in (0..4).rev() {
        let denomination = DENOMINATIONS[index] as u32;
        let count = (tubes[index] as u32).min(remaining / denomination);
        greedy[index] = count as u16;
        remaining -= count * denomination;
    }

    if remaining == 0 {
        return Some(greedy);
    }

    let mut best: Option<([u16; 4], u32)> = None;
    search_change(
        3,
        amount as i32,
        tubes,
        [0; 4],
        0,
        &mut best,
    );

    best.map(|(counts, _)| counts)
}

fn search_change(
    index: i32,
    remaining: i32,
    tubes: [u16; 4],
    counts: [u16; 4],
    total: u32,
    best: &mut Option<([u16; 4], u32)>,
) {
    if remaining == 0 {
        let replace = match best {
            None => true,
            Some((best_counts, best_total)) => {
                total < *best_total
                    || (total == *best_total
                        && lex_greater_descending(counts, *best_counts))
            }
        };
        if replace {
            *best = Some((counts, total));
        }
        return;
    }

    if index < 0 || remaining < 0 {
        return;
    }

    if let Some((_, best_total)) = best {
        if total >= *best_total {
            return;
        }
    }

    let i = index as usize;
    let denomination = DENOMINATIONS[i] as i32;
    let max_count = (tubes[i] as i32).min(remaining / denomination);

    for count in (0..=max_count).rev() {
        let mut next_counts = counts;
        next_counts[i] = count as u16;
        search_change(
            index - 1,
            remaining - count * denomination,
            tubes,
            next_counts,
            total + count as u32,
            best,
        );
    }
}

fn lex_greater_descending(left: [u16; 4], right: [u16; 4]) -> bool {
    for index in (0..4).rev() {
        if left[index] != right[index] {
            return left[index] > right[index];
        }
    }
    false
}

fn counts_to_descending(counts: [u16; 4]) -> Vec<u16> {
    let mut coins = Vec::new();
    for index in (0..4).rev() {
        for _ in 0..counts[index] {
            coins.push(DENOMINATIONS[index]);
        }
    }
    coins
}