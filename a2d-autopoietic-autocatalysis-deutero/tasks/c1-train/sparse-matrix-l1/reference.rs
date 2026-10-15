use std::collections::BTreeMap;
pub const MAX_DIM: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis { Row, Column }
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entry { pub row: usize, pub col: usize, pub value: f64 }
#[derive(Clone, Debug, PartialEq)]
pub struct Sparse { rows: usize, cols: usize, entries: Vec<Entry> }
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    Shape { axis: Axis, size: usize },
    Bounds { entry: usize, axis: Axis, index: usize, limit: usize },
    NonFinite { entry: usize },
    SumOverflow { entry: usize },

}
impl Sparse {
    pub fn from_coo(rows: usize, cols: usize, entries: &[Entry]) -> Result<Self, Error> {
        if rows > MAX_DIM { return Err(Error::Shape { axis: Axis::Row, size: rows }); }
        if cols > MAX_DIM { return Err(Error::Shape { axis: Axis::Column, size: cols }); }
        for (i, e) in entries.iter().enumerate() {
            if e.row >= rows { return Err(Error::Bounds { entry: i, axis: Axis::Row, index: e.row, limit: rows }); }
            if e.col >= cols { return Err(Error::Bounds { entry: i, axis: Axis::Column, index: e.col, limit: cols }); }
            if !e.value.is_finite() { return Err(Error::NonFinite { entry: i }); }
        }
        let mut sums = BTreeMap::new();
        for (i, e) in entries.iter().enumerate() {
            let old = *sums.get(&(e.row, e.col)).unwrap_or(&0.0);
            let next: f64 = old + e.value;
            if !next.is_finite() { return Err(Error::SumOverflow { entry: i }); }
            sums.insert((e.row, e.col), next);
        }
        let entries = sums.into_iter().filter_map(|((row, col), value)| {
            if value == 0.0 { None } else { Some(Entry { row, col, value }) }
        }).collect();
        Ok(Self { rows, cols, entries })
    }
    pub fn shape(&self) -> (usize, usize) { (self.rows, self.cols) }
    pub fn entries(&self) -> &[Entry] { &self.entries }
}
