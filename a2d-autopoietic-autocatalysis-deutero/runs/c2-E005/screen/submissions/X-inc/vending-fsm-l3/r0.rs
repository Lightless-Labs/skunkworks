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

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase())
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

        for (index, &count) in tubes.iter().enumerate() {
            if count > 100 {
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
        if !DENOMINATIONS.contains(&coin) {
            return Err(Error::BadCoin { coin });
        }
        if u32::from(self.credit) + u32::from(coin) > 100 {
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
        let Some(_) = self.products.iter().find(|product| product.id == id) else {
            return Err(Error::UnknownProduct { id: id.to_owned() });
        };
        if self.selected.as_deref() == Some(id) {
            return Err(Error::SameSelection);
        }

        self.selected = Some(id.to_owned());
        self.push_event(EventKind::Selected { id: id.to_owned() });
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let Some(id) = self.selected.as_ref() else {
            return Err(Error::NoSelection);
        };
        let product_index = self
            .products
            .iter()
            .position(|product| &product.id == id)
            .expect("selected catalog product must exist");

        let product = &self.products[product_index];
        if product.stock == 0 {
            return Err(Error::SoldOut {
                id: product.id.clone(),
            });
        }
        if self.credit < product.price {
            return Err(Error::Underpaid {
                price: product.price,
                credit: self.credit,
            });
        }

        let id = product.id.clone();
        let price = product.price;
        let amount = self.credit - price;
        let change_counts = choose_change(amount, &self.tubes)
            .ok_or(Error::NoChange { amount })?;

        let mut proposed_tubes = self.tubes;
        for index in 0..4 {
            proposed_tubes[index] -= change_counts[index];
        }
        for &coin in &self.escrow {
            let index = denomination_index(coin).expect("escrow contains accepted coins");
            proposed_tubes[index] += 1;
        }
        for index in 0..4 {
            if proposed_tubes[index] > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[index],
                });
            }
        }

        let change = counts_to_descending_coins(change_counts);
        self.tubes = proposed_tubes;
        self.products[product_index].stock -= 1;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.push_event(EventKind::Vended {
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
        self.push_event(EventKind::Refunded {
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
        self.push_event(EventKind::PriceChanged {
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

        let resulting_stock = u32::from(self.products[index].stock) + u32::from(count);
        if resulting_stock > 100 {
            return Err(Error::StockLimit { id: id.to_owned() });
        }

        self.products[index].stock = resulting_stock as u16;
        self.push_event(EventKind::Restocked {
            id: id.to_owned(),
            added: count,
            stock: resulting_stock as u16,
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
    DENOMINATIONS.iter().position(|&denomination| denomination == coin)
}

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<[u16; 4]> {
    if amount == 0 {
        return Some([0; 4]);
    }

    let mut greedy = [0u16; 4];
    let mut remainder = amount;
    for index in (0..4).rev() {
        let coin = DENOMINATIONS[index];
        let take = tubes[index].min(remainder / coin);
        greedy[index] = take;
        remainder -= take * coin;
    }
    if remainder == 0 {
        return Some(greedy);
    }

    let mut best: Option<[u16; 4]> = None;
    let mut best_total = u16::MAX;

    for count_10 in 0..=tubes[3].min(amount / 10) {
        let value_10 = count_10 * 10;
        for count_4 in 0..=tubes[2].min((amount - value_10) / 4) {
            let value_4 = value_10 + count_4 * 4;
            for count_3 in 0..=tubes[1].min((amount - value_4) / 3) {
                let value_3 = value_4 + count_3 * 3;
                let remaining = amount - value_3;
                if remaining > tubes[0] {
                    continue;
                }

                let candidate = [remaining, count_3, count_4, count_10];
                let total = candidate.iter().copied().sum::<u16>();
                let replace = match best {
                    None => true,
                    Some(current) => {
                        total < best_total
                            || (total == best_total
                                && (3..=0).rev().any(|_| false))
                            || (total == best_total
                                && lex_greater_descending(&candidate, &current))
                    }
                };
                if replace {
                    best = Some(candidate);
                    best_total = total;
                }
            }
        }
    }

    best
}

fn lex_greater_descending(left: &[u16; 4], right: &[u16; 4]) -> bool {
    for index in (0..4).rev() {
        if left[index] != right[index] {
            return left[index] > right[index];
        }
    }
    false
}

fn counts_to_descending_coins(counts: [u16; 4]) -> Vec<u16> {
    let capacity: usize = counts.iter().map(|&count| usize::from(count)).sum();
    let mut coins = Vec::with_capacity(capacity);
    for index in (0..4).rev() {
        coins.extend(std::iter::repeat_n(DENOMINATIONS[index], usize::from(counts[index])));
    }
    coins
}