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
    const DENOMINATIONS: [u16; 4] = [1, 3, 4, 10];

    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut seen = std::collections::HashSet::new();
        for product in &products {
            let valid_id = !product.id.is_empty()
                && product.id.len() <= 8
                && product.id.bytes().all(|byte| byte.is_ascii_lowercase());
            if !valid_id {
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
                    coin: Self::DENOMINATIONS[index],
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
        if !Self::DENOMINATIONS.contains(&coin) {
            return Err(Error::BadCoin { coin });
        }
        if u32::from(self.credit) + u32::from(coin) > 100 {
            return Err(Error::CreditLimit);
        }

        let credit = self.credit + coin;
        self.credit = credit;
        self.escrow.push(coin);
        self.record(EventKind::Inserted { coin, credit });
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
        self.record(EventKind::Selected { id: id.to_owned() });
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let id = self.selected.as_ref().ok_or(Error::NoSelection)?.clone();
        let index = self
            .products
            .iter()
            .position(|product| product.id == id)
            .expect("selected product must remain in the catalog");

        let product = &self.products[index];
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
        let change = Self::make_change(amount, self.tubes)
            .ok_or(Error::NoChange { amount })?;

        let mut proposed_tubes = self.tubes;
        for (tube_index, denomination) in Self::DENOMINATIONS.iter().enumerate() {
            let used = change
                .iter()
                .filter(|coin| *coin == denomination)
                .count() as u16;
            proposed_tubes[tube_index] -= used;
        }
        for coin in &self.escrow {
            let tube_index = Self::DENOMINATIONS
                .iter()
                .position(|denomination| denomination == coin)
                .expect("escrow contains only accepted denominations");
            proposed_tubes[tube_index] += 1;
        }
        for (tube_index, count) in proposed_tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: Self::DENOMINATIONS[tube_index],
                });
            }
        }

        self.products[index].stock -= 1;
        self.tubes = proposed_tubes;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        let receipt = Receipt {
            id: id.clone(),
            price,
            change: change.clone(),
        };
        self.record(EventKind::Vended { id, price, change });
        Ok(receipt)
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        let coins: Vec<u16> = self.escrow.iter().rev().copied().collect();
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.record(EventKind::Refunded {
            coins: coins.clone(),
        });
        Ok(coins)
    }

    pub fn products(&self) -> Vec<Product> {
        let mut products = self.products.clone();
        products.sort_by(|left, right| left.id.cmp(&right.id));
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
        self.record(EventKind::PriceChanged {
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
        if u32::from(self.products[index].stock) + u32::from(count) > 100 {
            return Err(Error::StockLimit { id: id.to_owned() });
        }

        let stock = self.products[index].stock + count;
        self.products[index].stock = stock;
        self.record(EventKind::Restocked {
            id: id.to_owned(),
            added: count,
            stock,
        });
        Ok(())
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    fn record(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
    }

    fn make_change(amount: u16, tubes: [u16; 4]) -> Option<Vec<u16>> {
        if amount == 0 {
            return Some(Vec::new());
        }

        let descending = [10u16, 4, 3, 1];
        let mut remaining = amount;
        let mut greedy = Vec::new();
        for denomination in descending {
            let index = Self::DENOMINATIONS
                .iter()
                .position(|coin| *coin == denomination)
                .expect("known denomination");
            let take = tubes[index].min(remaining / denomination);
            greedy.extend(std::iter::repeat_n(denomination, usize::from(take)));
            remaining -= take * denomination;
        }
        if remaining == 0 {
            return Some(greedy);
        }

        let mut best: Option<Vec<u16>> = None;
        let mut current = Vec::new();
        Self::search_change(
            0,
            amount,
            &descending,
            &tubes,
            &mut current,
            &mut best,
        );
        best
    }

    fn search_change(
        index: usize,
        remaining: u16,
        descending: &[u16; 4],
        tubes: &[u16; 4],
        current: &mut Vec<u16>,
        best: &mut Option<Vec<u16>>,
    ) {
        if remaining == 0 {
            let replace = match best {
                None => true,
                Some(existing) => {
                    current.len() < existing.len()
                        || (current.len() == existing.len() && current.as_slice() > existing.as_slice())
                }
            };
            if replace {
                *best = Some(current.clone());
            }
            return;
        }
        if index == descending.len() {
            return;
        }
        if let Some(existing) = best {
            if current.len() >= existing.len() {
                return;
            }
        }

        let denomination = descending[index];
        let tube_index = Self::DENOMINATIONS
            .iter()
            .position(|coin| *coin == denomination)
            .expect("known denomination");
        let max_take = tubes[tube_index].min(remaining / denomination);
        for take in (0..=max_take).rev() {
            let added = usize::from(take);
            if let Some(existing) = best {
                if current.len() + added > existing.len() {
                    continue;
                }
            }

            let old_len = current.len();
            current.extend(std::iter::repeat_n(denomination, added));
            Self::search_change(
                index + 1,
                remaining - take * denomination,
                descending,
                tubes,
                current,
                best,
            );
            current.truncate(old_len);
        }
    }
}