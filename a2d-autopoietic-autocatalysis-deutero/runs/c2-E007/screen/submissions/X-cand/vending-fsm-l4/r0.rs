use std::collections::HashSet;

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
    pub maintenance: bool,
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
    MaintenanceActive,
    MaintenanceRequired,
    AlreadyMode,
    ReconcileMismatch {
        coin: u16,
        expected: u16,
        found: u16,
    },
    ValueDrift { before: u64, after: u64 },
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
    Maintenance { enabled: bool },
    Reconciled { before: [u16; 4], after: [u16; 4] },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub seq: usize,
    pub kind: EventKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Audit {
    pub inserted: u64,
    pub refunded: u64,
    pub sales: u64,
    pub change: u64,
    pub adjustment: i64,
    pub cash: u64,
}

pub struct Machine {
    products: Vec<Product>,
    tubes: [u16; 4],
    credit: u16,
    selected: Option<String>,
    escrow: Vec<u16>,
    maintenance: bool,
    events: Vec<Event>,
    inserted: u64,
    refunded: u64,
    sales: u64,
    change: u64,
    adjustment: i64,
}

const DENOMINATIONS: [u16; 4] = [1, 3, 4, 10];

impl Machine {
    pub fn new(products: Vec<Product>, tubes: [u16; 4]) -> Result<Self, Error> {
        if products.is_empty() {
            return Err(Error::EmptyCatalog);
        }

        let mut seen = HashSet::new();
        for product in &products {
            if !valid_id(&product.id) {
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

        for (i, count) in tubes.iter().enumerate() {
            if *count > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
                });
            }
        }

        Ok(Self {
            products,
            tubes,
            credit: 0,
            selected: None,
            escrow: Vec::new(),
            maintenance: false,
            events: Vec::new(),
            inserted: 0,
            refunded: 0,
            sales: 0,
            change: 0,
            adjustment: 0,
        })
    }

    pub fn state(&self) -> State {
        State {
            credit: self.credit,
            selected: self.selected.clone(),
            escrow: self.escrow.clone(),
            tubes: self.tubes,
            maintenance: self.maintenance,
        }
    }

    pub fn insert(&mut self, coin: u16) -> Result<(), Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }
        if !DENOMINATIONS.contains(&coin) {
            return Err(Error::BadCoin { coin });
        }
        if u32::from(self.credit) + u32::from(coin) > 100 {
            return Err(Error::CreditLimit);
        }

        self.credit += coin;
        self.escrow.push(coin);
        self.inserted = self.inserted.saturating_add(u64::from(coin));
        self.log(EventKind::Inserted {
            coin,
            credit: self.credit,
        });
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }
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
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }

        let id = match &self.selected {
            Some(id) => id.clone(),
            None => return Err(Error::NoSelection),
        };
        let index = self.product_index(&id).expect("selected product exists");
        let product = &self.products[index];

        if product.stock == 0 {
            return Err(Error::SoldOut { id });
        }
        if self.credit < product.price {
            return Err(Error::Underpaid {
                price: product.price,
                credit: self.credit,
            });
        }

        let price = product.price;
        let amount = self.credit - price;
        let change = choose_change(amount, &self.tubes)
            .ok_or(Error::NoChange { amount })?;
        let proposed_tubes = proposed_tubes(&self.tubes, &change, &self.escrow)?;

        self.tubes = proposed_tubes;
        self.products[index].stock -= 1;
        self.credit = 0;
        self.selected = None;
        self.escrow.clear();
        self.sales = self.sales.saturating_add(u64::from(price));
        let change_value = coin_vector_value(&change);
        self.change = self.change.saturating_add(change_value);
        self.log(EventKind::Vended {
            id: id.clone(),
            price,
            change: change.clone(),
        });

        Ok(Receipt { id, price, change })
    }

    pub fn refund(&mut self) -> Result<Vec<u16>, Error> {
        if self.maintenance {
            return Err(Error::MaintenanceActive);
        }

        let coins: Vec<u16> = self.escrow.iter().rev().copied().collect();
        self.refunded = self.refunded.saturating_add(u64::from(self.credit));
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
        if !self.maintenance {
            return Err(Error::MaintenanceRequired);
        }
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
        if !self.maintenance {
            return Err(Error::MaintenanceRequired);
        }
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

    pub fn set_maintenance(&mut self, enabled: bool) -> Result<(), Error> {
        if enabled == self.maintenance {
            return Err(Error::AlreadyMode);
        }
        if self.credit != 0 {
            return Err(Error::Busy);
        }

        self.maintenance = enabled;
        self.selected = None;
        self.log(EventKind::Maintenance { enabled });
        Ok(())
    }

    pub fn audit(&self) -> Audit {
        Audit {
            inserted: self.inserted,
            refunded: self.refunded,
            sales: self.sales,
            change: self.change,
            adjustment: self.adjustment,
            cash: tube_value(&self.tubes).saturating_add(u64::from(self.credit)),
        }
    }

    pub fn reconcile(
        &mut self,
        expected: [u16; 4],
        actual: [u16; 4],
    ) -> Result<(), Error> {
        if !self.maintenance {
            return Err(Error::MaintenanceRequired);
        }

        for i in 0..4 {
            if expected[i] != self.tubes[i] {
                return Err(Error::ReconcileMismatch {
                    coin: DENOMINATIONS[i],
                    expected: expected[i],
                    found: self.tubes[i],
                });
            }
        }
        for i in 0..4 {
            if actual[i] > 100 {
                return Err(Error::TubeLimit {
                    coin: DENOMINATIONS[i],
                });
            }
        }

        let before_value = tube_value(&self.tubes);
        let after_value = tube_value(&actual);
        let drift = before_value.abs_diff(after_value);
        if drift > 20 {
            return Err(Error::ValueDrift {
                before: before_value,
                after: after_value,
            });
        }

        let signed_difference = i128::from(after_value) - i128::from(before_value);
        let adjustment = (i128::from(self.adjustment) + signed_difference)
            .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;

        let before = self.tubes;
        self.tubes = actual;
        self.adjustment = adjustment;
        self.log(EventKind::Reconciled {
            before,
            after: actual,
        });
        Ok(())
    }

    fn product_index(&self, id: &str) -> Option<usize> {
        self.products.iter().position(|product| product.id == id)
    }

    fn log(&mut self, kind: EventKind) {
        let seq = self.events.len();
        self.events.push(Event { seq, kind });
    }
}

fn valid_id(id: &str) -> bool {
    (1..=8).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase())
}

fn tube_value(tubes: &[u16; 4]) -> u64 {
    tubes
        .iter()
        .zip(DENOMINATIONS)
        .map(|(&count, denomination)| u64::from(count) * u64::from(denomination))
        .sum()
}

fn coin_vector_value(coins: &[u16]) -> u64 {
    coins.iter().map(|&coin| u64::from(coin)).sum()
}

fn proposed_tubes(
    tubes: &[u16; 4],
    change: &[u16],
    escrow: &[u16],
) -> Result<[u16; 4], Error> {
    let mut proposed = *tubes;

    for &coin in change {
        let index = denomination_index(coin).expect("change uses valid denominations");
        proposed[index] -= 1;
    }
    for &coin in escrow {
        let index = denomination_index(coin).expect("escrow uses valid denominations");
        proposed[index] += 1;
    }

    for (index, &count) in proposed.iter().enumerate() {
        if count > 100 {
            return Err(Error::TubeLimit {
                coin: DENOMINATIONS[index],
            });
        }
    }

    Ok(proposed)
}

fn denomination_index(coin: u16) -> Option<usize> {
    DENOMINATIONS.iter().position(|&d| d == coin)
}

fn choose_change(amount: u16, tubes: &[u16; 4]) -> Option<Vec<u16>> {
    if amount == 0 {
        return Some(Vec::new());
    }

    let mut remaining = amount;
    let mut greedy = [0u16; 4];

    for index in (0..4).rev() {
        let denomination = DENOMINATIONS[index];
        let take = tubes[index].min(remaining / denomination);
        greedy[index] = take;
        remaining -= take * denomination;
    }

    if remaining == 0 {
        return Some(counts_to_descending_coins(&greedy));
    }

    let target = usize::from(amount);
    let mut best: Vec<Option<[u16; 4]>> = vec![None; target + 1];
    best[0] = Some([0; 4]);

    for index in (0..4).rev() {
        let denomination = usize::from(DENOMINATIONS[index]);
        let max_count = usize::from(tubes[index]).min(target / denomination);
        let previous = best.clone();

        for subtotal in 0..=target {
            let Some(base) = previous[subtotal] else {
                continue;
            };

            for count in 0..=max_count {
                let next = subtotal + count * denomination;
                if next > target {
                    break;
                }

                let mut candidate = base;
                candidate[index] += count as u16;
                match best[next] {
                    None => best[next] = Some(candidate),
                    Some(existing) if better_combination(&candidate, &existing) => {
                        best[next] = Some(candidate);
                    }
                    _ => {}
                }
            }
        }
    }

    best[target].map(|counts| counts_to_descending_coins(&counts))
}

fn better_combination(candidate: &[u16; 4], existing: &[u16; 4]) -> bool {
    let candidate_total: u32 = candidate.iter().map(|&n| u32::from(n)).sum();
    let existing_total: u32 = existing.iter().map(|&n| u32::from(n)).sum();

    if candidate_total != existing_total {
        return candidate_total < existing_total;
    }

    for index in (0..4).rev() {
        if candidate[index] != existing[index] {
            return candidate[index] > existing[index];
        }
    }

    false
}

fn counts_to_descending_coins(counts: &[u16; 4]) -> Vec<u16> {
    let capacity: usize = counts.iter().map(|&n| usize::from(n)).sum();
    let mut coins = Vec::with_capacity(capacity);

    for index in (0..4).rev() {
        for _ in 0..counts[index] {
            coins.push(DENOMINATIONS[index]);
        }
    }

    coins
}