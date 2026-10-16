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

        for (index, count) in tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: Self::denomination(index),
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
        let Some(index) = Self::denomination_index(coin) else {
            return Err(Error::BadCoin { coin });
        };
        let new_credit = u32::from(self.credit) + u32::from(coin);
        if new_credit > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit = new_credit as u16;
        self.escrow.push(coin);
        self.log(EventKind::Inserted {
            coin,
            credit: self.credit,
        });
        let _ = index;
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
            .expect("selected product must remain in the fixed catalog");

        if self.products[index].stock == 0 {
            return Err(Error::SoldOut { id });
        }

        let price = self.products[index].price;
        if self.credit < price {
            return Err(Error::Underpaid {
                price,
                credit: self.credit,
            });
        }

        let amount = self.credit - price;
        let change_counts = Self::choose_change(amount, self.tubes)
            .ok_or(Error::NoChange { amount })?;

        let mut proposed_tubes = self.tubes;
        for i in 0..4 {
            proposed_tubes[i] -= change_counts[i];
        }
        for &coin in &self.escrow {
            let i = Self::denomination_index(coin)
                .expect("escrow only contains accepted denominations");
            proposed_tubes[i] += 1;
        }
        for (i, count) in proposed_tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: Self::denomination(i),
                });
            }
        }

        let change = Self::expand_change(change_counts);
        self.tubes = proposed_tubes;
        self.products[index].stock -= 1;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        let receipt = Receipt {
            id: id.clone(),
            price,
            change: change.clone(),
        };
        self.log(EventKind::Vended {
            id,
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
        let stock = u32::from(self.products[index].stock) + u32::from(count);
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

    fn product_index(&self, id: &str) -> Option<usize> {
        self.products.iter().position(|product| product.id == id)
    }

    fn log(&mut self, kind: EventKind) {
        self.events.push(Event {
            seq: self.events.len(),
            kind,
        });
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

    fn denomination(index: usize) -> u16 {
        [1, 3, 4, 10][index]
    }

    fn choose_change(amount: u16, tubes: [u16; 4]) -> Option<[u16; 4]> {
        let mut greedy = [0u16; 4];
        let mut remaining = amount;

        for index in (0..4).rev() {
            let coin = Self::denomination(index);
            let take = tubes[index].min(remaining / coin);
            greedy[index] = take;
            remaining -= take * coin;
        }

        if remaining == 0 {
            return Some(greedy);
        }

        let mut best: Option<[u16; 4]> = None;
        let mut best_count = u32::MAX;

        for c10 in 0..=tubes[3].min(amount / 10) {
            let rem10 = amount - c10 * 10;
            for c4 in 0..=tubes[2].min(rem10 / 4) {
                let rem4 = rem10 - c4 * 4;
                for c3 in 0..=tubes[1].min(rem4 / 3) {
                    let rem3 = rem4 - c3 * 3;
                    if rem3 > tubes[0] {
                        continue;
                    }
                    let candidate = [rem3, c3, c4, c10];
                    let count = candidate.iter().map(|&n| u32::from(n)).sum();
                    let better = match best {
                        None => true,
                        Some(current) => {
                            count < best_count
                                || (count == best_count
                                    && Self::descending_lex_greater(candidate, current))
                        }
                    };
                    if better {
                        best = Some(candidate);
                        best_count = count;
                    }
                }
            }
        }

        best
    }

    fn descending_lex_greater(a: [u16; 4], b: [u16; 4]) -> bool {
        for index in (0..4).rev() {
            if a[index] != b[index] {
                return a[index] > b[index];
            }
        }
        false
    }

    fn expand_change(counts: [u16; 4]) -> Vec<u16> {
        let mut coins = Vec::new();
        for index in (0..4).rev() {
            for _ in 0..counts[index] {
                coins.push(Self::denomination(index));
            }
        }
        coins
    }
}