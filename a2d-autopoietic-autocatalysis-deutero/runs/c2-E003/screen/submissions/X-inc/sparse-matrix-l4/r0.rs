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

        let mut sums: Vec<(usize, usize, f64)> = Vec::new();
        let mut positions: Vec<(usize, usize, usize)> = Vec::new();

        for (i, entry) in entries.iter().enumerate() {
            let position = positions
                .iter()
                .find(|&&(r, c, _)| r == entry.row && c == entry.col)
                .map(|&(_, _, p)| p);

            if let Some(p) = position {
                let sum = sums[p].2 + entry.value;
                if !sum.is_finite() {
                    return Err(Error::SumOverflow { entry: i });
                }
                sums[p].2 = sum;
            } else {
                let p = sums.len();
                sums.push((entry.row, entry.col, 0.0 + entry.value));
                positions.push((entry.row, entry.col, p));
            }
        }

        sums.retain(|item| item.2 != 0.0);
        sums.sort_by_key(|item| (item.0, item.1));

        Ok(Self {
            rows,
            cols,
            entries: sums
                .into_iter()
                .map(|(row, col, value)| Entry { row, col, value })
                .collect(),
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

        Csr {
            rows: self.rows,
            cols: self.cols,
            row_ptr,
            col_idx: self.entries.iter().map(|e| e.col).collect(),
            values: self.entries.iter().map(|e| e.value).collect(),
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
            let mut previous = None;
            for slot in row_ptr[row]..row_ptr[row + 1] {
                let column = col_idx[slot];
                if column >= cols {
                    return Err(Error::ColumnBounds {
                        slot,
                        column,
                        limit: cols,
                    });
                }
                if previous.is_some_and(|p| column <= p) {
                    return Err(Error::ColumnOrder { slot });
                }
                previous = Some(column);
                if !values[slot].is_finite() {
                    return Err(Error::NonFiniteValue { slot });
                }
                if values[slot] == 0.0 {
                    return Err(Error::ExplicitZero { slot });
                }
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
        for (index, &value) in x.iter().enumerate() {
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
                let target = next[target_row];
                col_idx[target] = row;
                values[target] = self.values[slot];
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
                for right_slot in rhs.row_ptr[inner]..rhs.row_ptr[inner + 1] {
                    let column = rhs.col_idx[right_slot];
                    let product = self.values[left_slot] * rhs.values[right_slot];
                    if !product.is_finite() {
                        return Err(Error::MultiplyOverflow {
                            row,
                            inner,
                            column,
                        });
                    }

                    let position = accumulators
                        .iter()
                        .position(|&(c, _)| c == column)
                        .unwrap_or_else(|| {
                            accumulators.push((column, 0.0));
                            accumulators.len() - 1
                        });
                    let sum = accumulators[position].1 + product;
                    if !sum.is_finite() {
                        return Err(Error::MultiplySumOverflow {
                            row,
                            inner,
                            column,
                        });
                    }
                    accumulators[position].1 = sum;
                }
            }

            accumulators.retain(|&(_, value)| value != 0.0);
            accumulators.sort_by_key(|&(column, _)| column);
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
                if self.values[slot].abs() > tolerance {
                    col_idx.push(self.col_idx[slot]);
                    values.push(self.values[slot]);
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
        let mut text = format!("SM1 {} {} {}\n", self.rows, self.cols, self.values.len());
        for row in 0..self.rows {
            for slot in self.row_ptr[row]..self.row_ptr[row + 1] {
                text.push_str(&format!(
                    "{} {} {:016x}\n",
                    row,
                    self.col_idx[slot],
                    self.values[slot].to_bits()
                ));
            }
        }
        text
    }

    pub fn parse_text(text: &str) -> Result<Csr, Error> {
        let fail = |line, kind| Error::Text { line, kind };

        if !text.ends_with('\n') {
            return Err(fail(1 + text.bytes().filter(|&b| b == b'\n').count(), TextError::Newline));
        }

        let body = &text[..text.len() - 1];
        let lines: Vec<&str> = body.split('\n').collect();
        let header = lines[0].split(' ').collect::<Vec<_>>();
        if header.len() != 4
            || header.iter().any(|field| field.is_empty())
            || header[0] != "SM1"
        {
            return Err(fail(1, TextError::Header));
        }

        fn parse_decimal(token: &str) -> Option<usize> {
            if token.is_empty()
                || !token.bytes().all(|b| b.is_ascii_digit())
                || (token.len() > 1 && token.starts_with('0'))
            {
                return None;
            }
            token.parse::<usize>().ok()
        }

        let rows = parse_decimal(header[1]).ok_or_else(|| fail(1, TextError::Decimal))?;
        let cols = parse_decimal(header[2]).ok_or_else(|| fail(1, TextError::Decimal))?;
        let nnz = parse_decimal(header[3]).ok_or_else(|| fail(1, TextError::Decimal))?;

        if rows > MAX_DIM || cols > MAX_DIM {
            return Err(fail(1, TextError::Dimension));
        }
        if nnz != lines.len() - 1 {
            return Err(fail(1, TextError::Count));
        }

        let mut row_ptr = vec![0usize; rows + 1];
        let mut col_idx = Vec::new();
        let mut values = Vec::new();
        let mut previous: Option<(usize, usize)> = None;

        for (line_index, line) in lines.iter().enumerate().skip(1) {
            let line_number = line_index + 1;
            let fields = line.split(' ').collect::<Vec<_>>();
            if fields.len() != 3 || fields.iter().any(|field| field.is_empty()) {
                return Err(fail(line_number, TextError::Record));
            }

            let row = parse_decimal(fields[0])
                .ok_or_else(|| fail(line_number, TextError::Decimal))?;
            let column = parse_decimal(fields[1])
                .ok_or_else(|| fail(line_number, TextError::Decimal))?;
            let bits_text = fields[2];
            if bits_text.len() != 16
                || !bits_text
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(fail(line_number, TextError::Hex));
            }
            let bits = u64::from_str_radix(bits_text, 16)
                .map_err(|_| fail(line_number, TextError::Hex))?;
            let value = f64::from_bits(bits);

            if row >= rows {
                return Err(fail(line_number, TextError::RowBounds));
            }
            if column >= cols {
                return Err(fail(line_number, TextError::ColumnBounds));
            }
            if !value.is_finite() {
                return Err(fail(line_number, TextError::NonFinite));
            }
            if value == 0.0 {
                return Err(fail(line_number, TextError::Zero));
            }
            if previous.is_some_and(|coord| (row, column) <= coord) {
                return Err(fail(line_number, TextError::Order));
            }
            previous = Some((row, column));

            col_idx.push(column);
            values.push(value);
            row_ptr[row + 1] += 1;
        }

        for row in 0..rows {
            row_ptr[row + 1] += row_ptr[row];
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