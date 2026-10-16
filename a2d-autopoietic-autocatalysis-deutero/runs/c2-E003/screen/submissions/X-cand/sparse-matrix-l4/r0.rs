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
        for (i, entry) in entries.iter().enumerate() {
            let pos = sums.binary_search_by_key(&(entry.row, entry.col), |&(r, c, _)| (r, c));
            match pos {
                Ok(index) => {
                    sums[index].2 += entry.value;
                    if !sums[index].2.is_finite() {
                        return Err(Error::SumOverflow { entry: i });
                    }
                }
                Err(index) => sums.insert(index, (entry.row, entry.col, entry.value)),
            }
        }

        let canonical = sums
            .into_iter()
            .filter_map(|(row, col, value)| {
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
                if let Some(prev) = previous {
                    if column <= prev {
                        return Err(Error::ColumnOrder { slot });
                    }
                }
                if !values[slot].is_finite() {
                    return Err(Error::NonFiniteValue { slot });
                }
                if values[slot] == 0.0 {
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
                col_idx[target] = row;
                values[target] = self.values[slot];
                next[column] += 1;
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

                    match accumulators.binary_search_by_key(&column, |&(col, _)| col) {
                        Ok(index) => {
                            accumulators[index].1 += product;
                            if !accumulators[index].1.is_finite() {
                                return Err(Error::MultiplySumOverflow {
                                    row,
                                    inner,
                                    column,
                                });
                            }
                        }
                        Err(index) => accumulators.insert(index, (column, product)),
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

        let mut text = String::new();
        writeln!(
            &mut text,
            "SM1 {} {} {}",
            self.rows,
            self.cols,
            self.values.len()
        )
        .unwrap();

        for row in 0..self.rows {
            for slot in self.row_ptr[row]..self.row_ptr[row + 1] {
                writeln!(
                    &mut text,
                    "{} {} {:016x}",
                    row,
                    self.col_idx[slot],
                    self.values[slot].to_bits()
                )
                .unwrap();
            }
        }
        text
    }

    pub fn parse_text(text: &str) -> Result<Csr, Error> {
        let text_error = |line, kind| Error::Text { line, kind };

        let lf_count = text.as_bytes().iter().filter(|&&byte| byte == b'\n').count();
        if !text.ends_with('\n') {
            return Err(text_error(lf_count + 1, TextError::Newline));
        }

        let without_final_lf = &text[..text.len() - 1];
        let lines: Vec<&str> = without_final_lf.split('\n').collect();
        let header = lines[0];
        let header_fields: Vec<&str> = header.split(' ').collect();
        if header_fields.len() != 4
            || header_fields.iter().any(|field| field.is_empty())
            || header_fields[0] != "SM1"
        {
            return Err(text_error(1, TextError::Header));
        }

        let rows = parse_decimal(header_fields[1])
            .ok_or_else(|| text_error(1, TextError::Decimal))?;
        let cols = parse_decimal(header_fields[2])
            .ok_or_else(|| text_error(1, TextError::Decimal))?;
        let nnz = parse_decimal(header_fields[3])
            .ok_or_else(|| text_error(1, TextError::Decimal))?;

        if rows > MAX_DIM || cols > MAX_DIM {
            return Err(text_error(1, TextError::Dimension));
        }
        if nnz != lines.len() - 1 {
            return Err(text_error(1, TextError::Count));
        }

        let mut row_ptr = vec![0; rows + 1];
        let mut col_idx = Vec::with_capacity(nnz);
        let mut values = Vec::with_capacity(nnz);
        let mut previous: Option<(usize, usize)> = None;
        let mut current_row = 0;

        for (index, line) in lines.iter().enumerate().skip(1) {
            let line_number = index + 1;
            let fields: Vec<&str> = line.split(' ').collect();
            if fields.len() != 3 || fields.iter().any(|field| field.is_empty()) {
                return Err(text_error(line_number, TextError::Record));
            }

            let row = parse_decimal(fields[0])
                .ok_or_else(|| text_error(line_number, TextError::Decimal))?;
            let column = parse_decimal(fields[1])
                .ok_or_else(|| text_error(line_number, TextError::Decimal))?;

            let bits_text = fields[2].as_bytes();
            if bits_text.len() != 16
                || !bits_text
                    .iter()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
            {
                return Err(text_error(line_number, TextError::Hex));
            }
            let bits = u64::from_str_radix(fields[2], 16)
                .map_err(|_| text_error(line_number, TextError::Hex))?;
            let value = f64::from_bits(bits);

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
            let coordinate = (row, column);
            if previous.is_some_and(|prev| coordinate <= prev) {
                return Err(text_error(line_number, TextError::Order));
            }
            previous = Some(coordinate);

            while current_row < row {
                row_ptr[current_row + 1] = col_idx.len();
                current_row += 1;
            }
            col_idx.push(column);
            values.push(value);
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
    let bytes = token.as_bytes();
    if bytes.is_empty()
        || !bytes.iter().all(u8::is_ascii_digit)
        || (bytes.len() > 1 && bytes[0] == b'0')
    {
        return None;
    }
    token.parse::<usize>().ok()
}