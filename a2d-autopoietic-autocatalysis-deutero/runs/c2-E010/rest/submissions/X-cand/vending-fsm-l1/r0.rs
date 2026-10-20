#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Product {
    pub id: String,
    pub price: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub credit: u16,
    pub selected: Option<String>,
    pub escrow: Vec<u16>,
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
    ExactPayment { price: u16, credit: u16 },
    TenderMismatch { index: usize, expected: u16, found: u16 },
}

pub struct Machine {
    prices: Vec<(String, u16)>,
    balance: u16,
    choice: Option<usize>,
    coins: Vec<u16>,
}

impl Machine {
    pub fn new(products: Vec<Product>) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut prices: Vec<(String, u16)> = Vec::with_capacity(products.len());
        for product in products {
            let id = product.id;
            let id_ok = !id.is_empty()
                && id.len() <= 8
                && id.as_bytes().iter().all(u8::is_ascii_lowercase);
            if !id_ok {
                return Err(Error::BadId { id });
            }
            if prices.iter().any(|(existing, _)| existing == &id) {
                return Err(Error::DuplicateId { id });
            }
            if product.price == 0 || product.price > 100 {
                return Err(Error::BadPrice { id });
            }
            prices.push((id, product.price));
        }

        Ok(Self {
            prices,
            balance: 0,
            choice: None,
            coins: Vec::new(),
        })
    }

    pub fn state(&self) -> State {
        State {
            credit: self.balance,
            selected: self.choice.map(|index| self.prices[index].0.clone()),
            escrow: self.coins.clone(),
        }
    }

    pub fn insert(&mut self, coin: u16) -> Result<(), Error> {
        if coin != 1 && coin != 3 && coin != 4 && coin != 10 {
            return Err(Error::BadCoin { coin });
        }
        if self.balance > 100 - coin {
            return Err(Error::CreditLimit);
        }
        self.balance += coin;
        self.coins.push(coin);
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        let index = match self.prices.iter().position(|(product_id, _)| product_id == id) {
            Some(index) => index,
            None => return Err(Error::UnknownProduct { id: id.to_owned() }),
        };
        if self.choice == Some(index) {
            return Err(Error::SameSelection);
        }
        self.choice = Some(index);
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let index = match self.choice {
            Some(index) => index,
            None => return Err(Error::NoSelection),
        };
        let (id, price) = &self.prices[index];

        if self.balance < *price {
            return Err(Error::Underpaid {
                price: *price,
                credit: self.balance,
            });
        }
        if self.balance > *price {
            return Err(Error::ExactPayment {
                price: *price,
                credit: self.balance,
            });
        }

        let mut canonical = Vec::new();
        let mut left = *price;
        for coin in [10u16, 4, 3, 1] {
            let count = left / coin;
            for _ in 0..count {
                canonical.push(coin);
            }
            left %= coin;
        }

        let mut tender = self.coins.clone();
        tender.sort_unstable_by(|a, b| b.cmp(a));
        for (position, (&expected, &found)) in canonical.iter().zip(tender.iter()).enumerate() {
            if expected != found {
                return Err(Error::TenderMismatch {
                    index: position,
                    expected,
                    found,
                });
            }
        }

        let receipt = Receipt {
            id: id.clone(),
            price: *price,
            change: Vec::new(),
        };
        self.balance = 0;
        self.choice = None;
        self.coins.clear();
        Ok(receipt)
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        let mut returned = Vec::with_capacity(self.coins.len());
        for &coin in self.coins.iter().rev() {
            returned.push(coin);
        }
        self.balance = 0;
        self.choice = None;
        self.coins.clear();
        Ok(returned)
    }
}
