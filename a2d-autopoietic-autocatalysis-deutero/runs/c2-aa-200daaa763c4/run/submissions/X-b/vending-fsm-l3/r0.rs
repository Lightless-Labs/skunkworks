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

#[derive(Clone, Debug)]
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

        let mut seen = std::collections::HashSet::new();
        for product in &products {
            if product.id.is_empty()
                || product.id.len() > 8
                || !product.id.bytes().all(|b| b.is_ascii_lowercase())
            {
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

        for (i, count) in tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: [1, 3, 4, 10][i],
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
        let index = match coin {
            1 => 0,
            3 => 1,
            4 => 2,
            10 => 3,
            _ => return Err(Error::BadCoin { coin }),
        };

        let _ = index;
        if self.credit as u32 + coin as u32 > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit += coin;
        self.escrow.push(coin);
        self.push_event(EventKind::Inserted {
            coin,
            credit: self.credit,
        });
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
        self.push_event(EventKind::Selected { id: id.to_owned() });
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let id = self.selected.clone().ok_or(Error::NoSelection)?;
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
        let change = make_change(amount, self.tubes)?;
        let mut proposed_tubes = self.tubes;
        for &coin in &change {
            let index = denomination_index(coin).expect("change uses valid denominations");
            proposed_tubes[index] -= 1;
        }
        for &coin in &self.escrow {
            let index = denomination_index(coin).expect("escrow uses valid denominations");
            proposed_tubes[index] += 1;
        }
        for (i, count) in proposed_tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: [1, 3, 4, 10][i],
                });
            }
        }

        self.tubes = proposed_tubes;
        self.products[product_index].stock -= 1;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        let receipt = Receipt {
            id: id.clone(),
            price,
            change: change.clone(),
        };
        self.push_event(EventKind::Vended { id, price, change });
        Ok(receipt)
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        let coins: Vec<u16> = self.escrow.iter().rev().copied().collect();
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.push_event(EventKind::Refunded {
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
        self.push_event(EventKind::PriceChanged {
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
        self.push_event(EventKind::Restocked {
            id: id.to_owned(),
            added: count,
            stock: stock as u16,
        });
        Ok(())
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    fn push_event(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
    }
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

fn make_change(amount: u16, tubes: [u16; 4]) -> Result<Vec<u16>, Error> {
    if amount == 0 {
        return Ok(Vec::new());
    }

    let denominations = [1u16, 3, 4, 10];
    let mut remaining = amount;
    let mut greedy = Vec::new();

    for index in (0..4).rev() {
        let coin = denominations[index];
        let take = tubes[index].min(remaining / coin);
        greedy.extend(std::iter::repeat_n(coin, take as usize));
        remaining -= take * coin;
    }
    if remaining == 0 {
        return Ok(greedy);
    }

    let mut best: Option<([u16; 4], u32)> = None;
    search_change(
        3,
        amount as u32,
        [0; 4],
        0,
        &tubes,
        &mut best,
    );

    match best {
        Some((counts, _)) => {
            let mut result = Vec::new();
            for index in (0..4).rev() {
                result.extend(std::iter::repeat_n(
                    denominations[index],
                    counts[index] as usize,
                ));
            }
            Ok(result)
        }
        None => Err(Error::NoChange { amount }),
    }
}

fn search_change(
    index: isize,
    remaining: u32,
    counts: [u16; 4],
    coin_count: u32,
    tubes: &[u16; 4],
    best: &mut Option<([u16; 4], u32)>,
) {
    if index < 0 {
        if remaining == 0 {
            let replace = match best {
                None => true,
                Some((best_counts, best_total)) => {
                    coin_count < *best_total
                        || (coin_count == *best_total
                            && lex_greater_desc(&counts, best_counts))
                }
            };
            if replace {
                *best = Some((counts, coin_count));
            }
        }
        return;
    }

    let i = index as usize;
    let coin = [1u32, 3, 4, 10][i];
    let maximum = tubes[i].min((remaining / coin) as u16);
    for take in 0..=maximum {
        let next_count = coin_count + take as u32;
        if let Some((_, best_total)) = best {
            if next_count > *best_total {
                continue;
            }
        }
        let mut next_counts = counts;
        next_counts[i] = take;
        search_change(
            index - 1,
            remaining - take as u32 * coin,
            next_counts,
            next_count,
            tubes,
            best,
        );
    }
}

fn lex_greater_desc(left: &[u16; 4], right: &[u16; 4]) -> bool {
    for i in (0..4).rev() {
        if left[i] != right[i] {
            return left[i] > right[i];
        }
    }
    false
}