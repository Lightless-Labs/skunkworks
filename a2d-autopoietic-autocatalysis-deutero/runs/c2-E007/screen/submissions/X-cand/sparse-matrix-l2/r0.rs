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
pub struct Csr {
    rows: usize,
    cols: usize,
    row_ptr: Vec<usize>,
    col_idx: Vec<usize>,
    values: Vec<f64>,
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

    RowPtrLength {
        expected: usize,
        actual: usize,
    },
    ColumnValueLength {
        columns: usize,
        values: usize,
    },
    RowPtrStart {
        found: usize,
    },
    RowPtrEnd {
        found: usize,
        expected: usize,
    },
    RowPtrOrder {
        row: usize,
    },
    ColumnBounds {
        slot: usize,
        column: usize,
        limit: usize,
    },
    ColumnOrder {
        slot: usize,
    },
    NonFiniteValue {
        slot: usize,
    },
    ExplicitZero {
        slot: usize,
    },
    VectorLength {
        expected: usize,
        actual: usize,
    },
    NonFiniteVector {
        index: usize,
    },
    ProductOverflow {
        row: usize,
        column: usize,
    },
    AccumulationOverflow {
        row: usize,
        column: usize,
    },
}

impl Sparse {
    pub fn from_coo(rows: usize, cols: usize, entries: &[Entry]) -> Result<Self, Error> {
        validate_shape(rows, cols)?;

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

        let mut sums: Vec<(usize, usize, f64)> = Vec::new();
        let mut positions: Vec<usize> = Vec::new();

        for (index, entry) in entries.iter().enumerate() {
            let position = positions
                .iter()
                .copied()
                .find(|&position| {
                    sums[position].0 == entry.row && sums[position].1 == entry.col
                });

            let sum_index = match position {
                Some(position) => position,
                None => {
                    sums.push((entry.row, entry.col, 0.0));
                    positions.push(sums.len() - 1);
                    sums.len() - 1
                }
            };

            let next = sums[sum_index].2 + entry.value;
            if !next.is_finite() {
                return Err(Error::SumOverflow { entry: index });
            }
            sums[sum_index].2 = next;
        }

        let mut canonical: Vec<Entry> = sums
            .into_iter()
            .filter_map(|(row, col, value)| {
                if value == 0.0 {
                    None
                } else {
                    Some(Entry { row, col, value })
                }
            })
            .collect();

        canonical.sort_by_key(|entry| (entry.row, entry.col));

        Ok(Self {
            rows,
            cols,
            entries: canonical,
        })
    }

    pub fn shape(&self) -> (usize, usize) {
        (self.rows, self.cols)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn to_csr(&self) -> Csr {
        let mut row_ptr = vec![0; self.rows + 1];
        for entry in &self.entries {
            row_ptr[entry.row + 1] += 1;
        }
        for row in 0..self.rows {
            row_ptr[row + 1] += row_ptr[row];
        }

        let col_idx = self.entries.iter().map(|entry| entry.col).collect();
        let values = self.entries.iter().map(|entry| entry.value).collect();

        Csr {
            rows: self.rows,
            cols: self.cols,
            row_ptr,
            col_idx,
            values,
        }
    }
}

impl Csr {
    pub fn from_parts(
        rows: usize,
        cols: usize,
        row_ptr: Vec<usize>,
        col_idx: Vec<usize>,
        values: Vec<f64>,
    ) -> Result<Self, Error> {
        validate_shape(rows, cols)?;

        let expected_ptr_len = rows + 1;
        if row_ptr.len() != expected_ptr_len {
            return Err(Error::RowPtrLength {
                expected: expected_ptr_len,
                actual: row_ptr.len(),
            });
        }
        if col_idx.len() != values.len() {
            return Err(Error::ColumnValueLength {
                columns: col_idx.len(),
                values: values.len(),
            });
        }
        if row_ptr[0] != 0 {
            return Err(Error::RowPtrStart { found: row_ptr[0] });
        }
        if row_ptr[rows] != values.len() {
            return Err(Error::RowPtrEnd {
                found: row_ptr[rows],
                expected: values.len(),
            });
        }

        for row in 0..rows {
            if row_ptr[row] > row_ptr[row + 1] {
                return Err(Error::RowPtrOrder { row });
            }
        }

        for row in 0..rows {
            let start = row_ptr[row];
            let end = row_ptr[row + 1];
            let mut previous_column: Option<usize> = None;

            for slot in start..end {
                let column = col_idx[slot];
                if column >= cols {
                    return Err(Error::ColumnBounds {
                        slot,
                        column,
                        limit: cols,
                    });
                }
                if previous_column.is_some_and(|previous| column <= previous) {
                    return Err(Error::ColumnOrder { slot });
                }
                if !values[slot].is_finite() {
                    return Err(Error::NonFiniteValue { slot });
                }
                if values[slot] == 0.0 {
                    return Err(Error::ExplicitZero { slot });
                }
                previous_column = Some(column);
            }
        }

        Ok(Self {
            rows,
            cols,
            row_ptr,
            col_idx,
            values,
        })
    }

    pub fn shape(&self) -> (usize, usize) {
        (self.rows, self.cols)
    }

    pub fn row_ptr(&self) -> &[usize] {
        &self.row_ptr
    }

    pub fn col_indices(&self) -> &[usize] {
        &self.col_idx
    }

    pub fn values(&self) -> &[f64] {
        &self.values
    }

    pub fn to_coo(&self) -> Sparse {
        let mut entries = Vec::with_capacity(self.values.len());
        for row in 0..self.rows {
            for slot in self.row_ptr[row]..self.row_ptr[row + 1] {
                entries.push(Entry {
                    row,
                    col: self.col_idx[slot],
                    value: self.values[slot],
                });
            }
        }

        Sparse {
            rows: self.rows,
            cols: self.cols,
            entries,
        }
    }

    pub fn mul_vector(&self, x: &[f64]) -> Result<Vec<f64>, Error> {
        if x.len() != self.cols {
            return Err(Error::VectorLength {
                expected: self.cols,
                actual: x.len(),
            });
        }
        for (index, value) in x.iter().enumerate() {
            if !value.is_finite() {
                return Err(Error::NonFiniteVector { index });
            }
        }

        let mut result = Vec::with_capacity(self.rows);
        for row in 0..self.rows {
            let mut accumulator = 0.0;
            for slot in self.row_ptr[row]..self.row_ptr[row + 1] {
                let column = self.col_idx[slot];
                let product = self.values[slot] * x[column];
                if !product.is_finite() {
                    return Err(Error::ProductOverflow { row, column });
                }

                accumulator += product;
                if !accumulator.is_finite() {
                    return Err(Error::AccumulationOverflow { row, column });
                }
            }
            if accumulator == 0.0 {
                accumulator = 0.0;
            }
            result.push(accumulator);
        }

        Ok(result)
    }
}

fn validate_shape(rows: usize, cols: usize) -> Result<(), Error> {
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
    Ok(())
}