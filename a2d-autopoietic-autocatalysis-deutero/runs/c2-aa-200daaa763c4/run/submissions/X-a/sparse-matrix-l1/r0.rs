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
    rows: usize,
    cols: usize,
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

        for (index, entry) in entries.iter().enumerate() {
            if entry.row >= rows {
                return Err(Error::Bounds {
                    entry: index,
                    axis: Axis::Row,
                    index: entry.row,
                    limit: rows,
                });
            }
            if entry.col >= cols {
                return Err(Error::Bounds {
                    entry: index,
                    axis: Axis::Column,
                    index: entry.col,
                    limit: cols,
                });
            }
            if !entry.value.is_finite() {
                return Err(Error::NonFinite { entry: index });
            }
        }

        let mut sums: Vec<((usize, usize), f64)> = Vec::new();

        for (index, entry) in entries.iter().enumerate() {
            let key = (entry.row, entry.col);
            let position = match sums.binary_search_by_key(&key, |(coord, _)| *coord) {
                Ok(position) => position,
                Err(position) => {
                    sums.insert(position, (key, 0.0));
                    position
                }
            };

            let sum = sums[position].1 + entry.value;
            if !sum.is_finite() {
                return Err(Error::SumOverflow { entry: index });
            }
            sums[position].1 = sum;
        }

        let canonical_entries = sums
            .into_iter()
            .filter_map(|((row, col), value)| {
                if value == 0.0 {
                    None
                } else {
                    Some(Entry { row, col, value })
                }
            })
            .collect();

        Ok(Self {
            rows,
            cols,
            entries: canonical_entries,
        })
    }

    pub fn shape(&self) -> (usize, usize) {
        (self.rows, self.cols)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
}