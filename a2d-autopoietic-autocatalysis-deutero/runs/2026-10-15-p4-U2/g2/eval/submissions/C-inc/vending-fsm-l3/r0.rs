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
            if product.stock > 100 {
                return Err(Error::StockLimit {
                    id: product.id.clone(),
                });
            }
        }

        for (i, count) in tubes.iter().enumerate() {
            if *count > 100 {
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
        let index = DENOMINATIONS.iter().position(|&d| d == coin);
        let Some(_) = index else {
            return Err(Error::BadCoin { coin });
        };

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
        let Some(_) = self.products.iter().position(|p| p.id == id) else {
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
        let Some(id) = self.selected.as_ref() else {
            return Err(Error::NoSelection);
        };

        let product_index = self.products.iter().position(|p| p.id == *id).unwrap();
        let product = &self.products[product_index];

        if product.stock == 0 {
            return Err(Error::SoldOut { id: id.clone() });
        }
        if self.credit < product.price {
            return Err(Error::Underpaid {
                price: product.price,
                credit: self.credit,
            });
        }

        let id = id.clone();
        let price = product.price;
        let amount = self.credit - price;
        let change = Self::make_change(amount, self.tubes)
            .ok_or(Error::NoChange { amount })?;

        let mut proposed_tubes = self.tubes;
        for (i, denomination) in DENOMINATIONS.iter().enumerate() {
            let count = change
                .iter()
                .filter(|&&coin| coin == *denomination)
                .count() as u16;
            proposed_tubes[i] -= count;
        }
        for &coin in &self.escrow {
            let i = DENOMINATIONS.iter().position(|&d| d == coin).unwrap();
            proposed_tubes[i] += 1;
        }
        for (i, count) in proposed_tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
                });
            }
        }

        let receipt = Receipt {
            id: id.clone(),
            price,
            change: change.clone(),
        };
        self.products[product_index].stock -= 1;
        self.tubes = proposed_tubes;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.log(EventKind::Vended {
            id,
            price,
            change,
        });
        Ok(receipt)
    }

    fn make_change(amount: u16, tubes: [u16; 4]) -> Option<Vec<u16>> {
        if amount == 0 {
            return Some(Vec::new());
        }

        // Greedy order is 10, 4, 3, 1.
        let descending_indices = [3usize, 2, 1, 0];
        let mut remaining = amount;
        let mut greedy = Vec::new();
        for &i in &descending_indices {
            let coin = DENOMINATIONS[i];
            let take = tubes[i].min(remaining / coin);
            for _ in 0..take {
                greedy.push(coin);
            }
            remaining -= take * coin;
        }
        if remaining == 0 {
            return Some(greedy);
        }

        // Enumerate all bounded combinations, minimizing coin count and then
        // maximizing the descending denomination vector lexicographically.
        let mut best: Option<[u16; 4]> = None;
        let max10 = tubes[3].min(amount / 10);
        for n10 in 0..=max10 {
            let rem10 = amount - n10 * 10;
            let max4 = tubes[2].min(rem10 / 4);
            for n4 in 0..=max4 {
                let rem4 = rem10 - n4 * 4;
                let max3 = tubes[1].min(rem4 / 3);
                for n3 in 0..=max3 {
                    let rem3 = rem4 - n3 * 3;
                    if rem3 > tubes[0] {
                        continue;
                    }
                    let candidate = [rem3, n3, n4, n10];
                    let count: u32 = candidate.iter().map(|&n| n as u32).sum();
                    let better = match best {
                        None => true,
                        Some(current) => {
                            let current_count: u32 =
                                current.iter().map(|&n| n as u32).sum();
                            count < current_count
                                || (count == current_count
                                    && (candidate[3], candidate[2], candidate[1], candidate[0])
                                        > (current[3], current[2], current[1], current[0]))
                        }
                    };
                    if better {
                        best = Some(candidate);
                    }
                }
            }
        }

        best.map(|counts| {
            let mut coins = Vec::new();
            for i in (0..4).rev() {
                for _ in 0..counts[i] {
                    coins.push(DENOMINATIONS[i]);
                }
            }
            coins
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
        let Some(index) = self.products.iter().position(|p| p.id == id) else {
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
        let Some(index) = self.products.iter().position(|p| p.id == id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
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

fn main() {}