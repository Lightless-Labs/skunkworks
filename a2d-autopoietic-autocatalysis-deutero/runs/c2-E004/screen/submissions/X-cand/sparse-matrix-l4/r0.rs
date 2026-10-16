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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextError {
    Newline,
    Header,
    Decimal,
    Dimension,
    Count,
    Record,
    Hex,
    RowBounds,
    ColumnBounds,
    NonFinite,
    Zero,
    Order,
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

    DimensionMismatch {
        left_cols: usize,
        right_rows: usize,
    },
    MultiplyOverflow {
        row: usize,
        inner: usize,
        column: usize,
    },
    MultiplySumOverflow {
        row: usize,
        inner: usize,
        column: usize,
    },

    Tolerance,
    Text {
        line: usize,
        kind: TextError,
    },
}

fn check_shape(rows: usize, cols: usize) -> Result<(), Error> {
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

impl Sparse {
    pub fn from_coo(rows: usize, cols: usize, entries: &[Entry]) -> Result<Self, Error> {
        check_shape(rows, cols)?;

        for (i, entry) in entries.iter().enumerate() {
            if entry.row >= rows {
                return Err(Error::Bounds {
                    entry: i,
                    axis: Axis::Row,
                    index: entry.row,
                    limit: rows,
                });
            }
            if entry.col >= cols {
                return Err(Error::Bounds {
                    entry: i,
                    axis: Axis::Column,
                    index: entry.col,
                    limit: cols,
                });
            }
            if !entry.value.is_finite() {
                return Err(Error::NonFinite { entry: i });
            }
        }

        let mut sums: Vec<((usize, usize), f64)> = Vec::new();
        for (i, entry) in entries.iter().enumerate() {
            let key = (entry.row, entry.col);
            if let Some((_, sum)) = sums.iter_mut().find(|(coord, _)| *coord == key) {
                *sum = *sum + entry.value;
                if !sum.is_finite() {
                    return Err(Error::SumOverflow { entry: i });
                }
            } else {
                let sum = 0.0 + entry.value;
                if !sum.is_finite() {
                    return Err(Error::SumOverflow { entry: i });
                }
                sums.push((key, sum));
            }
        }

        sums.retain(|(_, value)| *value != 0.0);
        sums.sort_by_key(|((row, col), _)| (*row, *col));
        let canonical = sums
            .into_iter()
            .map(|((row, col), value)| Entry { row, col, value })
            .collect();

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
        let mut row_ptr = vec![0usize; self.rows + 1];
        for entry in &self.entries {
            row_ptr[entry.row + 1] += 1;
        }
        for row in 0..self.rows {
            row_ptr[row + 1] += row_ptr[row];
        }

        Csr {
            rows: self.rows,
            cols: self.cols,
            row_ptr,
            col_idx: self.entries.iter().map(|entry| entry.col).collect(),
            values: self.entries.iter().map(|entry| entry.value).collect(),
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
        check_shape(rows, cols)?;

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
            let mut previous: Option<usize> = None;
            for slot in start..end {
                let column = col_idx[slot];
                if column >= cols {
                    return Err(Error::ColumnBounds {
                        slot,
                        column,
                        limit: cols,
                    });
                }
                if let Some(prev) = previous {
                    if column <= prev {
                        return Err(Error::ColumnOrder { slot });
                    }
                }
                let value = values[slot];
                if !value.is_finite() {
                    return Err(Error::NonFiniteValue { slot });
                }
                if value == 0.0 {
                    return Err(Error::ExplicitZero { slot });
                }
                previous = Some(column);
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
            let mut acc = 0.0;
            for slot in self.row_ptr[row]..self.row_ptr[row + 1] {
                let column = self.col_idx[slot];
                let product = self.values[slot] * x[column];
                if !product.is_finite() {
                    return Err(Error::ProductOverflow { row, column });
                }
                acc = acc + product;
                if !acc.is_finite() {
                    return Err(Error::AccumulationOverflow { row, column });
                }
            }
            if acc == 0.0 {
                acc = 0.0;
            }
            result.push(acc);
        }
        Ok(result)
    }

    pub fn transpose(&self) -> Csr {
        let mut counts = vec![0usize; self.cols];
        for &column in &self.col_idx {
            counts[column] += 1;
        }

        let mut row_ptr = vec![0usize; self.cols + 1];
        for row in 0..self.cols {
            row_ptr[row + 1] = row_ptr[row] + counts[row];
        }

        let mut next = row_ptr[..self.cols].to_vec();
        let mut col_idx = vec![0usize; self.values.len()];
        let mut values = vec![0.0; self.values.len()];

        for row in 0..self.rows {
            for slot in self.row_ptr[row]..self.row_ptr[row + 1] {
                let target_row = self.col_idx[slot];
                let target_slot = next[target_row];
                col_idx[target_slot] = row;
                values[target_slot] = self.values[slot];
                next[target_row] += 1;
            }
        }

        Csr {
            rows: self.cols,
            cols: self.rows,
            row_ptr,
            col_idx,
            values,
        }
    }

    pub fn multiply(&self, rhs: &Csr) -> Result<Csr, Error> {
        if self.cols != rhs.rows {
            return Err(Error::DimensionMismatch {
                left_cols: self.cols,
                right_rows: rhs.rows,
            });
        }

        let mut row_ptr = Vec::with_capacity(self.rows + 1);
        let mut col_idx = Vec::new();
        let mut values = Vec::new();
        row_ptr.push(0);

        for row in 0..self.rows {
            let mut accumulators: Vec<(usize, f64)> = Vec::new();

            for left_slot in self.row_ptr[row]..self.row_ptr[row + 1] {
                let inner = self.col_idx[left_slot];
                let left_value = self.values[left_slot];

                for rhs_slot in rhs.row_ptr[inner]..rhs.row_ptr[inner + 1] {
                    let column = rhs.col_idx[rhs_slot];
                    let product = left_value * rhs.values[rhs_slot];
                    if !product.is_finite() {
                        return Err(Error::MultiplyOverflow {
                            row,
                            inner,
                            column,
                        });
                    }

                    let position = match accumulators
                        .iter()
                        .position(|(existing_column, _)| *existing_column == column)
                    {
                        Some(position) => position,
                        None => {
                            accumulators.push((column, 0.0));
                            accumulators.len() - 1
                        }
                    };

                    let accumulator = &mut accumulators[position].1;
                    *accumulator = *accumulator + product;
                    if !accumulator.is_finite() {
                        return Err(Error::MultiplySumOverflow {
                            row,
                            inner,
                            column,
                        });
                    }
                }
            }

            accumulators.retain(|(_, value)| *value != 0.0);
            accumulators.sort_by_key(|(column, _)| *column);
            for (column, value) in accumulators {
                col_idx.push(column);
                values.push(value);
            }
            row_ptr.push(values.len());
        }

        Ok(Csr {
            rows: self.rows,
            cols: rhs.cols,
            row_ptr,
            col_idx,
            values,
        })
    }

    pub fn prune(&self, tolerance: f64) -> Result<Csr, Error> {
        if !tolerance.is_finite() || tolerance < 0.0 {
            return Err(Error::Tolerance);
        }

        let mut row_ptr = Vec::with_capacity(self.rows + 1);
        let mut col_idx = Vec::new();
        let mut values = Vec::new();
        row_ptr.push(0);

        for row in 0..self.rows {
            for slot in self.row_ptr[row]..self.row_ptr[row + 1] {
                let value = self.values[slot];
                if value.abs() > tolerance {
                    col_idx.push(self.col_idx[slot]);
                    values.push(value);
                }
            }
            row_ptr.push(values.len());
        }

        Ok(Csr {
            rows: self.rows,
            cols: self.cols,
            row_ptr,
            col_idx,
            values,
        })
    }

    pub fn write_text(&self) -> String {
        let mut output = format!("SM1 {} {} {}\n", self.rows, self.cols, self.values.len());
        for row in 0..self.rows {
            for slot in self.row_ptr[row]..self.row_ptr[row + 1] {
                output.push_str(&format!(
                    "{} {} {:016x}\n",
                    row,
                    self.col_idx[slot],
                    self.values[slot].to_bits()
                ));
            }
        }
        output
    }

    pub fn parse_text(text: &str) -> Result<Csr, Error> {
        if !text.ends_with('\n') {
            let line = text.as_bytes().iter().filter(|&&byte| byte == b'\n').count() + 1;
            return Err(Error::Text {
                line,
                kind: TextError::Newline,
            });
        }

        let without_final_lf = &text[..text.len() - 1];
        let lines: Vec<&str> = without_final_lf.split('\n').collect();
        let header = lines[0];
        let fields: Vec<&str> = header.split(' ').collect();
        if fields.len() != 4 || fields.iter().any(|field| field.is_empty()) || fields[0] != "SM1" {
            return Err(Error::Text {
                line: 1,
                kind: TextError::Header,
            });
        }

        let rows = parse_decimal(fields[1]).ok_or(Error::Text {
            line: 1,
            kind: TextError::Decimal,
        })?;
        let cols = parse_decimal(fields[2]).ok_or(Error::Text {
            line: 1,
            kind: TextError::Decimal,
        })?;
        let nnz = parse_decimal(fields[3]).ok_or(Error::Text {
            line: 1,
            kind: TextError::Decimal,
        })?;

        if rows > MAX_DIM || cols > MAX_DIM {
            return Err(Error::Text {
                line: 1,
                kind: TextError::Dimension,
            });
        }

        let record_count = lines.len() - 1;
        if nnz != record_count {
            return Err(Error::Text {
                line: 1,
                kind: TextError::Count,
            });
        }

        let mut row_ptr = vec![0usize; rows + 1];
        let mut col_idx = Vec::with_capacity(nnz);
        let mut values = Vec::with_capacity(nnz);
        let mut previous: Option<(usize, usize)> = None;
        let mut current_row = 0usize;

        for (record_offset, line) in lines.iter().skip(1).enumerate() {
            let line_number = record_offset + 2;
            let record_fields: Vec<&str> = line.split(' ').collect();
            if record_fields.len() != 3
                || record_fields.iter().any(|field| field.is_empty())
            {
                return Err(Error::Text {
                    line: line_number,
                    kind: TextError::Record,
                });
            }

            let row = parse_decimal(record_fields[0]).ok_or(Error::Text {
                line: line_number,
                kind: TextError::Decimal,
            })?;
            let column = parse_decimal(record_fields[1]).ok_or(Error::Text {
                line: line_number,
                kind: TextError::Decimal,
            })?;

            let bits_token = record_fields[2];
            if bits_token.len() != 16
                || !bits_token
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(Error::Text {
                    line: line_number,
                    kind: TextError::Hex,
                });
            }
            let bits = u64::from_str_radix(bits_token, 16).map_err(|_| Error::Text {
                line: line_number,
                kind: TextError::Hex,
            })?;
            let value = f64::from_bits(bits);

            if row >= rows {
                return Err(Error::Text {
                    line: line_number,
                    kind: TextError::RowBounds,
                });
            }
            if column >= cols {
                return Err(Error::Text {
                    line: line_number,
                    kind: TextError::ColumnBounds,
                });
            }
            if !value.is_finite() {
                return Err(Error::Text {
                    line: line_number,
                    kind: TextError::NonFinite,
                });
            }
            if value == 0.0 {
                return Err(Error::Text {
                    line: line_number,
                    kind: TextError::Zero,
                });
            }
            if let Some(prev) = previous {
                if (row, column) <= prev {
                    return Err(Error::Text {
                        line: line_number,
                        kind: TextError::Order,
                    });
                }
            }

            while current_row < row {
                row_ptr[current_row + 1] = col_idx.len();
                current_row += 1;
            }
            col_idx.push(column);
            values.push(value);
            previous = Some((row, column));
        }

        while current_row < rows {
            row_ptr[current_row + 1] = col_idx.len();
            current_row += 1;
        }

        Ok(Csr {
            rows,
            cols,
            row_ptr,
            col_idx,
            values,
        })
    }
}

fn parse_decimal(token: &str) -> Option<usize> {
    if token.is_empty()
        || !token.bytes().all(|byte| byte.is_ascii_digit())
        || (token.len() > 1 && token.as_bytes()[0] == b'0')
    {
        return None;
    }
    token.parse::<usize>().ok()
}