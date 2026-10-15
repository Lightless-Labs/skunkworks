use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Product { pub id: String, pub price: u16 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State { pub credit: u16, pub selected: Option<String>, pub escrow: Vec<u16> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt { pub id: String, pub price: u16, pub change: Vec<u16> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error { EmptyCatalog, BadId { id: String }, DuplicateId { id: String }, BadPrice { id: String }, BadCoin { coin: u16 }, CreditLimit, UnknownProduct { id: String }, SameSelection, NoSelection, Underpaid { price: u16, credit: u16 }, ExactPayment { price: u16, credit: u16 }, TenderMismatch { index: usize, expected: u16, found: u16 } }
pub struct Machine {
    products: BTreeMap<String, Product>,
    credit: u16,
    selected: Option<String>,
    escrow: Vec<u16>,

}
impl Machine {
    pub fn new(products: Vec<Product>) -> Result<Self, Error> {
        if products.is_empty() { return Err(Error::EmptyCatalog); }
        let mut catalog = BTreeMap::new();
        for p in products {
            if p.id.is_empty() || p.id.len() > 8 || !p.id.bytes().all(|c| c.is_ascii_lowercase()) {
                return Err(Error::BadId { id: p.id });
            }
            if catalog.contains_key(&p.id) { return Err(Error::DuplicateId { id: p.id }); }
            if p.price == 0 || p.price > 100 { return Err(Error::BadPrice { id: p.id }); }

            catalog.insert(p.id.clone(), p);
        }

        Ok(Self { products: catalog, credit: 0, selected: None, escrow: Vec::new(),  })
    }
    pub fn state(&self) -> State {
        State { credit: self.credit, selected: self.selected.clone(), escrow: self.escrow.clone(),  }
    }
    pub fn insert(&mut self, coin: u16) -> Result<(), Error> {

        if ![1,3,4,10].contains(&coin) { return Err(Error::BadCoin { coin }); }
        if self.credit as u32 + coin as u32 > 100 { return Err(Error::CreditLimit); }
        self.credit += coin;
        self.escrow.push(coin);

        Ok(())
    }
    pub fn select(&mut self, id: &str) -> Result<(), Error> {

        if !self.products.contains_key(id) { return Err(Error::UnknownProduct { id: id.into() }); }
        if self.selected.as_deref() == Some(id) { return Err(Error::SameSelection); }
        self.selected = Some(id.into());

        Ok(())
    }
    pub fn vend(&mut self) -> Result<Receipt, Error> {

        let id = self.selected.clone().ok_or(Error::NoSelection)?;
        let p = &self.products[&id];

        let price = p.price;
        if self.credit < price { return Err(Error::Underpaid { price, credit: self.credit }); }
        if self.credit > price { return Err(Error::ExactPayment { price, credit: self.credit }); }
        let mut canonical = Vec::new();
        let mut remaining = price;
        for coin in [10,4,3,1] {
            while remaining >= coin { canonical.push(coin); remaining -= coin; }
        }
        let mut tender = self.escrow.clone();
        tender.sort_by(|a,b| b.cmp(a));
        for (index, (&expected, &found)) in canonical.iter().zip(tender.iter()).enumerate() {
            if expected != found { return Err(Error::TenderMismatch { index, expected, found }); }
        }
        let change = Vec::new();

        self.credit = 0;
        self.escrow.clear();
        self.selected = None;

        Ok(Receipt { id, price, change })
    }
    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {


        let coins: Vec<u16> = self.escrow.iter().rev().copied().collect();
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();

        Ok(coins)
    }

}
