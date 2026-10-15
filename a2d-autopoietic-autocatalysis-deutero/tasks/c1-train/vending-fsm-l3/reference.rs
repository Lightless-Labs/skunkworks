use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Product { pub id: String, pub price: u16, pub stock: u16 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State { pub credit: u16, pub selected: Option<String>, pub escrow: Vec<u16>, pub tubes: [u16; 4] }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt { pub id: String, pub price: u16, pub change: Vec<u16> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error { EmptyCatalog, BadId { id: String }, DuplicateId { id: String }, BadPrice { id: String }, BadCoin { coin: u16 }, CreditLimit, UnknownProduct { id: String }, SameSelection, NoSelection, Underpaid { price: u16, credit: u16 }, TubeLimit { coin: u16 }, NoChange { amount: u16 }, StockLimit { id: String }, SoldOut { id: String }, Busy, ZeroRestock }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind { Inserted { coin: u16, credit: u16 }, Selected { id: String }, Vended { id: String, price: u16, change: Vec<u16> }, Refunded { coins: Vec<u16> }, PriceChanged { id: String, old: u16, new: u16 }, Restocked { id: String, added: u16, stock: u16 } }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event { pub seq: usize, pub kind: EventKind }
pub struct Machine {
    products: BTreeMap<String, Product>,
    credit: u16,
    selected: Option<String>,
    escrow: Vec<u16>,
    tubes: [u16;4],
    events: Vec<Event>,
}
impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16;4]) -> Result<Self, Error> {
        if products.is_empty() { return Err(Error::EmptyCatalog); }
        let mut catalog = BTreeMap::new();
        for p in products {
            if p.id.is_empty() || p.id.len() > 8 || !p.id.bytes().all(|c| c.is_ascii_lowercase()) {
                return Err(Error::BadId { id: p.id });
            }
            if catalog.contains_key(&p.id) { return Err(Error::DuplicateId { id: p.id }); }
            if p.price == 0 || p.price > 100 { return Err(Error::BadPrice { id: p.id }); }
            if p.stock > 100 { return Err(Error::StockLimit { id: p.id }); }
            catalog.insert(p.id.clone(), p);
        }
        for (i, coin) in [1,3,4,10].iter().enumerate() {
            if tubes[i] > 100 { return Err(Error::TubeLimit { coin: *coin }); }
        }
        Ok(Self { products: catalog, credit: 0, selected: None, escrow: Vec::new(), tubes,events: Vec::new(), })
    }
    pub fn state(&self) -> State {
        State { credit: self.credit, selected: self.selected.clone(), escrow: self.escrow.clone(), tubes: self.tubes, }
    }
    pub fn insert(&mut self, coin: u16) -> Result<(), Error> {

        if ![1,3,4,10].contains(&coin) { return Err(Error::BadCoin { coin }); }
        if self.credit as u32 + coin as u32 > 100 { return Err(Error::CreditLimit); }
        self.credit += coin;
        self.escrow.push(coin);
        self.log(EventKind::Inserted { coin, credit: self.credit });
        Ok(())
    }
    pub fn select(&mut self, id: &str) -> Result<(), Error> {

        if !self.products.contains_key(id) { return Err(Error::UnknownProduct { id: id.into() }); }
        if self.selected.as_deref() == Some(id) { return Err(Error::SameSelection); }
        self.selected = Some(id.into());
        self.log(EventKind::Selected { id: id.into() });
        Ok(())
    }
    pub fn vend(&mut self) -> Result<Receipt, Error> {

        let id = self.selected.clone().ok_or(Error::NoSelection)?;
        let p = &self.products[&id];
        if p.stock == 0 { return Err(Error::SoldOut { id }); }
        let price = p.price;
        if self.credit < price { return Err(Error::Underpaid { price, credit: self.credit }); }
        let amount = self.credit - price;
        let change = Self::make_change(amount, self.tubes).ok_or(Error::NoChange { amount })?;
        let ds = [1,3,4,10];
        let mut proposed = self.tubes.map(u32::from);
        for c in &change { proposed[ds.iter().position(|d| d == c).unwrap()] -= 1; }
        for c in &self.escrow { proposed[ds.iter().position(|d| d == c).unwrap()] += 1; }
        for i in 0..4 { if proposed[i] > 100 { return Err(Error::TubeLimit { coin: ds[i] }); } }
        self.tubes = proposed.map(|n| n as u16);
        self.products.get_mut(&id).unwrap().stock -= 1;
        self.credit = 0;
        self.escrow.clear();
        self.selected = None;
        self.log(EventKind::Vended { id: id.clone(), price, change: change.clone() });
        Ok(Receipt { id, price, change })
    }
    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {


        let coins: Vec<u16> = self.escrow.iter().rev().copied().collect();
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.log(EventKind::Refunded { coins: coins.clone() });
        Ok(coins)
    }
    fn make_change(amount: u16, tubes: [u16; 4]) -> Option<Vec<u16>> {
        let ds = [1,3,4,10];
        let mut remaining = amount;
        let mut greedy = Vec::new();
        for i in (0..4).rev() {
            let n = tubes[i].min(remaining / ds[i]);
            greedy.extend(std::iter::repeat_n(ds[i], n as usize));
            remaining -= n * ds[i];
        }
        if remaining == 0 { return Some(greedy); }
        let mut best: Option<Vec<u16>> = None;
        for tens in 0..=tubes[3].min(amount / 10) {
            let a = amount - tens * 10;
            for fours in 0..=tubes[2].min(a / 4) {
                let b = a - fours * 4;
                for threes in 0..=tubes[1].min(b / 3) {
                    let ones = b - threes * 3;
                    if ones > tubes[0] { continue; }
                    let mut v = Vec::new();
                    for (coin,n) in [(10,tens),(4,fours),(3,threes),(1,ones)] {
                        v.extend(std::iter::repeat_n(coin,n as usize));
                    }
                    let better = best.as_ref().is_none_or(|old| v.len() < old.len() || (v.len() == old.len() && v > *old));
                    if better { best = Some(v); }
                }
            }
        }
        best
    }
    pub fn products(&self) -> Vec<Product> { self.products.values().cloned().collect() }
    pub fn events(&self) -> &[Event] { &self.events }
    fn log(&mut self, kind: EventKind) { self.events.push(Event { seq: self.events.len(), kind }); }
    pub fn set_price(&mut self, id: &str, price: u16) -> Result<(), Error> {

        if !self.products.contains_key(id) { return Err(Error::UnknownProduct { id: id.into() }); }
        if self.credit != 0 { return Err(Error::Busy); }
        if price == 0 || price > 100 { return Err(Error::BadPrice { id: id.into() }); }
        let p = self.products.get_mut(id).unwrap();
        let old = p.price;
        p.price = price;
        self.log(EventKind::PriceChanged { id: id.into(), old, new: price });
        Ok(())
    }
    pub fn restock(&mut self, id: &str, count: u16) -> Result<(), Error> {

        let p = self.products.get_mut(id).ok_or_else(|| Error::UnknownProduct { id: id.into() })?;
        if count == 0 { return Err(Error::ZeroRestock); }
        if p.stock as u32 + count as u32 > 100 { return Err(Error::StockLimit { id: id.into() }); }
        p.stock += count;
        let stock = p.stock;
        self.log(EventKind::Restocked { id: id.into(), added: count, stock });
        Ok(())
    }

}
