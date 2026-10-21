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
    TenderMismatch {
        index: usize,
        expected: u16,
        found: u16,
    },
}

pub struct Machine {
    products: Vec<Product>,
    credit: u16,
    selected: Option<usize>,
    escrow: Vec<u16>,
}

impl Machine {
    pub fn new(products: Vec<Product>) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut seen: Vec<String> = Vec::new();
        for product in &products {
            let valid_id = !product.id.is_empty()
                && product.id.len() <= 8
                && product
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase());
            if !valid_id {
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
        }

        Ok(Self {
            products,
            credit: 0,
            selected: None,
            escrow: Vec::new(),
        })
    }

    pub fn state(&self) -> State {
        State {
            credit: self.credit,
            selected: self
                .selected
                .map(|index| self.products[index].id.clone()),
            escrow: self.escrow.clone(),
        }
    }

    pub fn insert(&mut self, coin: u16) -> Result<(), Error> {
        if !matches!(coin, 1 | 3 | 4 | 10) {
            return Err(Error::BadCoin { coin });
        }
        if u32::from(self.credit) + u32::from(coin) > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit += coin;
        self.escrow.push(coin);
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        let index = self
            .products
            .iter()
            .position(|product| product.id == id)
            .ok_or_else(|| Error::UnknownProduct { id: id.to_owned() })?;

        if self.selected == Some(index) {
            return Err(Error::SameSelection);
        }

        self.selected = Some(index);
        Ok(())
    }

    pub fn vend(&mut self) -> Result<Receipt, Error> {
        let index = self.selected.ok_or(Error::NoSelection)?;
        let product = &self.products[index];

        if self.credit < product.price {
            return Err(Error::Underpaid {
                price: product.price,
                credit: self.credit,
            });
        }
        if self.credit > product.price {
            return Err(Error::ExactPayment {
                price: product.price,
                credit: self.credit,
            });
        }

        let mut canonical = Vec::new();
        let mut remaining = product.price;
        for denomination in [10_u16, 4, 3, 1] {
            while remaining >= denomination {
                canonical.push(denomination);
                remaining -= denomination;
            }
        }

        let mut tender = self.escrow.clone();
        tender.sort_unstable_by(|a, b| b.cmp(a));

        for offset in 0..canonical.len().max(tender.len()) {
            let expected = canonical[offset];
            let found = tender[offset];
            if expected != found {
                return Err(Error::TenderMismatch {
                    index: offset,
                    expected,
                    found,
                });
            }
        }

        let receipt = Receipt {
            id: product.id.clone(),
            price: product.price,
            change: Vec::new(),
        };
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        Ok(receipt)
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        let returned = self.escrow.iter().rev().copied().collect();
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        Ok(returned)
    }
}