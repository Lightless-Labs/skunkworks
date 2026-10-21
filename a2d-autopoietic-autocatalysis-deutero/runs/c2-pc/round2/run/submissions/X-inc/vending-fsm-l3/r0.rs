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
    tubes: [u16; 4],
    credit: u16,
    selected: Option<String>,
    escrow: Vec<u16>,
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
            if !seen.insert(product.id.clone()) {
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

        let denominations = [1, 3, 4, 10];
        for i in 0..4 {
            if tubes[i] > 100 {
                return Err(Error::TubeLimit {
                    coin: denominations[i],
                });
            }
        }

        Ok(Self {
            products,
            tubes,
            credit: 0,
            selected: None,
            escrow: Vec::new(),
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
        if ![1, 3, 4, 10].contains(&coin) {
            return Err(Error::BadCoin { coin });
        }
        let new_credit = self.credit + coin;
        if new_credit > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit = new_credit;
        self.escrow.push(coin);
        self.log(EventKind::Inserted {
            coin,
            credit: new_credit,
        });
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        let Some(_) = self.products.iter().find(|p| p.id == id) else {
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
        let product_index = self.products.iter().position(|p| &p.id == id).unwrap();
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

        let price = product.price;
        let amount = self.credit - price;
        let change_counts =
            match Self::choose_change(amount, &self.tubes) {
                Some(counts) => counts,
                None => return Err(Error::NoChange { amount }),
            };

        let mut proposed = self.tubes;
        for i in 0..4 {
            proposed[i] -= change_counts[i];
            proposed[i] += self.escrow.iter().filter(|&&c| c == [1, 3, 4, 10][i]).count() as u16;
            if proposed[i] > 100 {
                return Err(Error::TubeLimit {
                    coin: [1, 3, 4, 10][i],
                });
            }
        }

        let mut change = Vec::new();
        for i in (0..4).rev() {
            for _ in 0..change_counts[i] {
                change.push([1, 3, 4, 10][i]);
            }
        }

        let receipt = Receipt {
            id: id.clone(),
            price,
            change: change.clone(),
        };
        self.products[product_index].stock -= 1;
        self.tubes = proposed;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.log(EventKind::Vended {
            id: receipt.id.clone(),
            price,
            change,
        });
        Ok(receipt)
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
        products.sort_by(|a, b| a.id.cmp(&b.id));
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

    fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<[u16; 4]> {
        let coins = [1u16, 3, 4, 10];
        let mut greedy = [0u16; 4];
        let mut remaining = amount;

        for i in (0..4).rev() {
            let take = tubes[i].min(remaining / coins[i]);
            greedy[i] = take;
            remaining -= take * coins[i];
        }
        if remaining == 0 {
            return Some(greedy);
        }

        let mut best: Option<[u16; 4]> = None;
        let mut best_count = u32::MAX;

        fn search(
            i: usize,
            remaining: u16,
            coins: &[u16; 4],
            tubes: &[u16; 4],
            current: &mut [u16; 4],
            best: &mut Option<[u16; 4]>,
            best_count: &mut u32,
        ) {
            if i == 4 {
                if remaining == 0 {
                    let count = current.iter().map(|&n| n as u32).sum();
                    let better_tie = best
                        .as_ref()
                        .map(|b| {
                            for j in (0..4).rev() {
                                if current[j] != b[j] {
                                    return current[j] > b[j];
                                }
                            }
                            false
                        })
                        .unwrap_or(false);
                    if count < *best_count || (count == *best_count && better_tie) {
                        *best_count = count;
                        *best = Some(*current);
                    }
                }
                return;
            }

            let max_take = tubes[i].min(remaining / coins[i]);
            for take in 0..=max_take {
                current[i] = take;
                search(
                    i + 1,
                    remaining - take * coins[i],
                    coins,
                    tubes,
                    current,
                    best,
                    best_count,
                );
            }
            current[i] = 0;
        }

        search(
            0,
            amount,
            &coins,
            tubes,
            &mut [0; 4],
            &mut best,
            &mut best_count,
        );
        best
    }
}