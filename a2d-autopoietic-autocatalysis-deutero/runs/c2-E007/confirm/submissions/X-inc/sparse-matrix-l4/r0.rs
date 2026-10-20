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
    Shape { axis: Axis, size: usize },
    Bounds {
        entry: usize,
        axis: Axis,
        index: usize,
        limit: usize,
    },
    NonFinite { entry: usize },
    SumOverflow { entry: usize },

    RowPtrLength { expected: usize, actual: usize },
    ColumnValueLength { columns: usize, values: usize },
    RowPtrStart { found: usize },
    RowPtrEnd { found: usize, expected: usize },
    RowPtrOrder { row: usize },
    ColumnBounds { slot: usize, column: usize, limit: usize },
    ColumnOrder { slot: usize },
    NonFiniteValue { slot: usize },
    ExplicitZero { slot: usize },
    VectorLength { expected: usize, actual: usize },
    NonFiniteVector { index: usize },
    ProductOverflow { row: usize, column: usize },
    AccumulationOverflow { row: usize, column: usize },

    DimensionMismatch { left_cols: usize, right_rows: usize },
    MultiplyOverflow { row: usize, inner: usize, column: usize },
    MultiplySumOverflow { row: usize, inner: usize, column: usize },

    Tolerance,
    Text { line: usize, kind: TextError },
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
        let mut positions: std::collections::BTreeMap<(usize, usize), usize> =
            std::collections::BTreeMap::new();

        for (i, entry) in entries.iter().enumerate() {
            let key = (entry.row, entry.col);
            if let Some(&position) = positions.get(&key) {
                let sum = sums[position].2 + entry.value;
                if !sum.is_finite() {
                    return Err(Error::SumOverflow { entry: i });
                }
                sums[position].2 = sum;
            } else {
                let position = sums.len();
                positions.insert(key, position);
                sums.push((entry.row, entry.col, 0.0 + entry.value));
            }
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
        let mut col_idx = Vec::with_capacity(self.entries.len());
        let mut values = Vec::with_capacity(self.entries.len());

        for entry in &self.entries {
            row_ptr[entry.row + 1] += 1;
            col_idx.push(entry.col);
            values.push(entry.value);
        }
        for row in 0..self.rows {
            row_ptr[row + 1] += row_ptr[row];
        }

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
        check_shape(rows, cols)?;

        let expected = rows + 1;
        if row_ptr.len() != expected {
            return Err(Error::RowPtrLength {
                expected,
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
            let mut previous = None;
            for slot in start..end {
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
        let mut row_ptr = vec![0; self.cols + 1];
        for &column in &self.col_idx {
            row_ptr[column + 1] += 1;
        }
        for row in 0..self.cols {
            row_ptr[row + 1] += row_ptr[row];
        }

        let mut next = row_ptr[..self.cols].to_vec();
        let mut col_idx = vec![0; self.values.len()];
        let mut values = vec![0.0; self.values.len()];
        for row in 0..self.rows {
            for slot in self.row_ptr[row]..self.row_ptr[row + 1] {
                let column = self.col_idx[slot];
                let target = next[column];
                next[column] += 1;
                col_idx[target] = row;
                values[target] = self.values[slot];
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
            let mut accumulators: std::collections::BTreeMap<usize, f64> =
                std::collections::BTreeMap::new();

            for left_slot in self.row_ptr[row]..self.row_ptr[row + 1] {
                let inner = self.col_idx[left_slot];
                let left_value = self.values[left_slot];

                for right_slot in rhs.row_ptr[inner]..rhs.row_ptr[inner + 1] {
                    let column = rhs.col_idx[right_slot];
                    let product = left_value * rhs.values[right_slot];
                    if !product.is_finite() {
                        return Err(Error::MultiplyOverflow {
                            row,
                            inner,
                            column,
                        });
                    }
                    let accumulator = accumulators.entry(column).or_insert(0.0);
                    *accumulator += product;
                    if !accumulator.is_finite() {
                        return Err(Error::MultiplySumOverflow {
                            row,
                            inner,
                            column,
                        });
                    }
                }
            }

            for (column, value) in accumulators {
                if value != 0.0 {
                    col_idx.push(column);
                    values.push(value);
                }
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
        use std::fmt::Write;

        let mut output = String::new();
        let _ = writeln!(
            output,
            "SM1 {} {} {}",
            self.rows,
            self.cols,
            self.values.len()
        );
        for row in 0..self.rows {
            for slot in self.row_ptr[row]..self.row_ptr[row + 1] {
                let _ = writeln!(
                    output,
                    "{} {} {:016x}",
                    row,
                    self.col_idx[slot],
                    self.values[slot].to_bits()
                );
            }
        }
        output
    }

    pub fn parse_text(text: &str) -> Result<Csr, Error> {
        fn text_error(line: usize, kind: TextError) -> Error {
            Error::Text { line, kind }
        }

        fn decimal(token: &str) -> Option<usize> {
            if token.is_empty()
                || !token.bytes().all(|b| b.is_ascii_digit())
                || (token.len() > 1 && token.as_bytes()[0] == b'0')
            {
                return None;
            }
            token.parse::<usize>().ok()
        }

        if !text.ends_with('\n') {
            let line = 1 + text.bytes().filter(|&byte| byte == b'\n').count();
            return Err(text_error(line, TextError::Newline));
        }

        let body = &text[..text.len() - 1];
        let lines: Vec<&str> = body.split('\n').collect();
        let header: Vec<&str> = lines[0].split(' ').collect();
        if header.len() != 4 || header.iter().any(|field| field.is_empty()) || header[0] != "SM1" {
            return Err(text_error(1, TextError::Header));
        }

        let rows = decimal(header[1]).ok_or_else(|| text_error(1, TextError::Decimal))?;
        let cols = decimal(header[2]).ok_or_else(|| text_error(1, TextError::Decimal))?;
        let nnz = decimal(header[3]).ok_or_else(|| text_error(1, TextError::Decimal))?;

        if rows > MAX_DIM || cols > MAX_DIM {
            return Err(text_error(1, TextError::Dimension));
        }
        if nnz != lines.len() - 1 {
            return Err(text_error(1, TextError::Count));
        }

        let mut entries = Vec::with_capacity(nnz);
        let mut row_ptr = vec![0; rows + 1];
        let mut col_idx = Vec::with_capacity(nnz);
        let mut values = Vec::with_capacity(nnz);
        let mut previous_coordinate = None;

        for line_index in 1..lines.len() {
            let line_number = line_index + 1;
            let fields: Vec<&str> = lines[line_index].split(' ').collect();
            if fields.len() != 3 || fields.iter().any(|field| field.is_empty()) {
                return Err(text_error(line_number, TextError::Record));
            }

            let row = decimal(fields[0])
                .ok_or_else(|| text_error(line_number, TextError::Decimal))?;
            let column = decimal(fields[1])
                .ok_or_else(|| text_error(line_number, TextError::Decimal))?;
            let bits = fields[2];
            if bits.len() != 16
                || !bits
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(text_error(line_number, TextError::Hex));
            }
            let raw = u64::from_str_radix(bits, 16)
                .map_err(|_| text_error(line_number, TextError::Hex))?;
            let value = f64::from_bits(raw);

            if row >= rows {
                return Err(text_error(line_number, TextError::RowBounds));
            }
            if column >= cols {
                return Err(text_error(line_number, TextError::ColumnBounds));
            }
            if !value.is_finite() {
                return Err(text_error(line_number, TextError::NonFinite));
            }
            if value == 0.0 {
                return Err(text_error(line_number, TextError::Zero));
            }
            if previous_coordinate.is_some_and(|previous| (row, column) <= previous) {
                return Err(text_error(line_number, TextError::Order));
            }

            previous_coordinate = Some((row, column));
            row_ptr[row + 1] += 1;
            col_idx.push(column);
            values.push(value);
            entries.push(Entry {
                row,
                col: column,
                value,
            });
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