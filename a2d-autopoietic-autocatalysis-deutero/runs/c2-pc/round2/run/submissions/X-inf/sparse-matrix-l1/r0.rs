use std::collections::BTreeMap;

pub const MAX_DIM: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Row,
    Column,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entry {
    pub row: usize,
    pub col: usize,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sparse {
    shape: (usize, usize),
    entries: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    Shape {
        axis: Axis,
        size: usize,
    },
    Bounds {
        entry: usize,
        axis: Axis,
        index: usize,
        limit: usize,
    },
    NonFinite {
        entry: usize,
    },
    SumOverflow {
        entry: usize,
    },
}

impl Sparse {
    pub fn from_coo(rows: usize, cols: usize, entries: &[Entry]) -> Result<Self, Error> {
        if rows > MAX_DIM {
            return Err(Error::Shape {
                axis: Axis::Row,
                size: rows,
            });
        }
        if cols > MAX_DIM {
            return Err(Error::Shape {
                axis: Axis::Column,
                size: cols,
            });
        }

        for (entry_index, entry) in entries.iter().enumerate() {
            if entry.row >= rows {
                return Err(Error::Bounds {
                    entry: entry_index,
                    axis: Axis::Row,
                    index: entry.row,
                    limit: rows,
                });
            }
            if entry.col >= cols {
                return Err(Error::Bounds {
                    entry: entry_index,
                    axis: Axis::Column,
                    index: entry.col,
                    limit: cols,
                });
            }
            if !entry.value.is_finite() {
                return Err(Error::NonFinite { entry: entry_index });
            }
        }

        let mut accumulators = BTreeMap::new();
        for (entry_index, entry) in entries.iter().enumerate() {
            let coordinate = (entry.row, entry.col);
            let current = accumulators.entry(coordinate).or_insert(0.0_f64);
            let next = *current + entry.value;
            if !next.is_finite() {
                return Err(Error::SumOverflow { entry: entry_index });
            }
            *current = next;
        }

        let mut canonical = Vec::with_capacity(accumulators.len());
        for ((row, col), value) in accumulators {
            if value != 0.0 {
                canonical.push(Entry { row, col, value });
            }
        }

        Ok(Self {
            shape: (rows, cols),
            entries: canonical,
        })
    }

    pub fn shape(&self) -> (usize, usize) {
        self.shape
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
}