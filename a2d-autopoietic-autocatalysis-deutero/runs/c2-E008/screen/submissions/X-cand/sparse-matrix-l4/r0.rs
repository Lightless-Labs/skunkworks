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
    Bounds { entry: usize, axis: Axis, index: usize, limit: usize },
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
        return Err(Error::Shape { axis: Axis::Row, size: rows });
    }
    if cols > MAX_DIM {
        return Err(Error::Shape { axis: Axis::Column, size: cols });
    }
    Ok(())
}

impl Sparse {
    pub fn from_coo(rows: usize, cols: usize, entries: &[Entry]) -> Result<Self, Error> {
        check_shape(rows, cols)?;

        for (i, e) in entries.iter().enumerate() {
            if e.row >= rows {
                return Err(Error::Bounds {
                    entry: i,
                    axis: Axis::Row,
                    index: e.row,
                    limit: rows,
                });
            }
            if e.col >= cols {
                return Err(Error::Bounds {
                    entry: i,
                    axis: Axis::Column,
                    index: e.col,
                    limit: cols,
                });
            }
            if !e.value.is_finite() {
                return Err(Error::NonFinite { entry: i });
            }
        }

        let mut sums: Vec<(usize, usize, f64)> = Vec::new();
        for (i, e) in entries.iter().enumerate() {
            let pos = sums
                .binary_search_by_key(&(e.row, e.col), |&(r, c, _)| (r, c));
            match pos {
                Ok(index) => {
                    sums[index].2 += e.value;
                    if !sums[index].2.is_finite() {
                        return Err(Error::SumOverflow { entry: i });
                    }
                }
                Err(index) => sums.insert(index, (e.row, e.col, 0.0 + e.value)),
            }
        }

        let canonical = sums
            .into_iter()
            .filter_map(|(row, col, value)| {
                (value != 0.0).then_some(Entry { row, col, value })
            })
            .collect();

        Ok(Self { rows, cols, entries: canonical })
    }

    pub fn shape(&self) -> (usize, usize) {
        (self.rows, self.cols)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn to_csr(&self) -> Csr {
        let mut row_ptr = vec![0; self.rows + 1];
        for e in &self.entries {
            row_ptr[e.row + 1] += 1;
        }
        for r in 0..self.rows {
            row_ptr[r + 1] += row_ptr[r];
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
        let expected = rows + 1;
        if row_ptr.len() != expected {
            return Err(Error::RowPtrLength { expected, actual: row_ptr.len() });
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
            return Err(Error::RowPtrEnd { found: row_ptr[rows], expected: values.len() });
        }
        for r in 0..rows {
            if row_ptr[r] > row_ptr[r + 1] {
                return Err(Error::RowPtrOrder { row: r });
            }
        }
        for r in 0..rows {
            let start = row_ptr[r];
            let end = row_ptr[r + 1];
            let mut previous = None;
            for slot in start..end {
                let column = col_idx[slot];
                if column >= cols {
                    return Err(Error::ColumnBounds { slot, column, limit: cols });
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
        Ok(Self { rows, cols, row_ptr, col_idx, values })
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
        Sparse { rows: self.rows, cols: self.cols, entries }
    }

    pub fn mul_vector(&self, x: &[f64]) -> Result<Vec<f64>, Error> {
        if x.len() != self.cols {
            return Err(Error::VectorLength { expected: self.cols, actual: x.len() });
        }
        for (index, &value) in x.iter().enumerate() {
            if !value.is_finite() {
                return Err(Error::NonFiniteVector { index });
            }
        }
        let mut result = Vec::with_capacity(self.rows);
        for row in 0..self.rows {
            let mut sum = 0.0;
            for slot in self.row_ptr[row]..self.row_ptr[row + 1] {
                let column = self.col_idx[slot];
                let product = self.values[slot] * x[column];
                if !product.is_finite() {
                    return Err(Error::ProductOverflow { row, column });
                }
                sum += product;
                if !sum.is_finite() {
                    return Err(Error::AccumulationOverflow { row, column });
                }
            }
            result.push(if sum == 0.0 { 0.0 } else { sum });
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
                let column = self.col_idx[slot];
                let dest = next[column];
                next[column] += 1;
                col_idx[dest] = row;
                values[dest] = self.values[slot];
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
                for right_slot in rhs.row_ptr[inner]..rhs.row_ptr[inner + 1] {
                    let column = rhs.col_idx[right_slot];
                    let product = left_value * rhs.values[right_slot];
                    if !product.is_finite() {
                        return Err(Error::MultiplyOverflow { row, inner, column });
                    }
                    let index = match accumulators.binary_search_by_key(&column, |&(c, _)| c) {
                        Ok(index) => index,
                        Err(index) => {
                            accumulators.insert(index, (column, 0.0));
                            index
                        }
                    };
                    accumulators[index].1 += product;
                    if !accumulators[index].1.is_finite() {
                        return Err(Error::MultiplySumOverflow { row, inner, column });
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
                if self.values[slot].abs() > tolerance {
                    col_idx.push(self.col_idx[slot]);
                    values.push(self.values[slot]);
                }
            }
            row_ptr.push(values.len());
        }
        Ok(Csr { rows: self.rows, cols: self.cols, row_ptr, col_idx, values })
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
            let line = 1 + text.bytes().filter(|&b| b == b'\n').count();
            return Err(Error::Text { line, kind: TextError::Newline });
        }

        let body = &text[..text.len() - 1];
        let lines: Vec<&str> = body.split('\n').collect();
        let header: Vec<&str> = lines[0].split(' ').collect();
        if header.len() != 4 || header.iter().any(|field| field.is_empty()) || header[0] != "SM1" {
            return Err(Error::Text { line: 1, kind: TextError::Header });
        }

        let rows = parse_decimal(header[1]).ok_or(Error::Text { line: 1, kind: TextError::Decimal })?;
        let cols = parse_decimal(header[2]).ok_or(Error::Text { line: 1, kind: TextError::Decimal })?;
        let nnz = parse_decimal(header[3]).ok_or(Error::Text { line: 1, kind: TextError::Decimal })?;

        if rows > MAX_DIM || cols > MAX_DIM {
            return Err(Error::Text { line: 1, kind: TextError::Dimension });
        }
        let records = &lines[1..];
        if nnz != records.len() {
            return Err(Error::Text { line: 1, kind: TextError::Count });
        }

        let mut row_ptr = vec![0usize; rows + 1];
        let mut col_idx = Vec::with_capacity(nnz);
        let mut values = Vec::with_capacity(nnz);
        let mut previous = None;

        for (index, record) in records.iter().enumerate() {
            let line = index + 2;
            let fields: Vec<&str> = record.split(' ').collect();
            if fields.len() != 3 || fields.iter().any(|field| field.is_empty()) {
                return Err(Error::Text { line, kind: TextError::Record });
            }
            let row = parse_decimal(fields[0])
                .ok_or(Error::Text { line, kind: TextError::Decimal })?;
            let column = parse_decimal(fields[1])
                .ok_or(Error::Text { line, kind: TextError::Decimal })?;
            let bits = fields[2].as_bytes();
            if bits.len() != 16
                || !bits.iter().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
            {
                return Err(Error::Text { line, kind: TextError::Hex });
            }
            let mut parsed_bits = 0u64;
            for &b in bits {
                parsed_bits = (parsed_bits << 4)
                    | match b {
                        b'0'..=b'9' => (b - b'0') as u64,
                        b'a'..=b'f' => (b - b'a' + 10) as u64,
                        _ => unreachable!(),
                    };
            }
            let value = f64::from_bits(parsed_bits);

            if row >= rows {
                return Err(Error::Text { line, kind: TextError::RowBounds });
            }
            if column >= cols {
                return Err(Error::Text { line, kind: TextError::ColumnBounds });
            }
            if !value.is_finite() {
                return Err(Error::Text { line, kind: TextError::NonFinite });
            }
            if value == 0.0 {
                return Err(Error::Text { line, kind: TextError::Zero });
            }
            if previous.is_some_and(|p| (row, column) <= p) {
                return Err(Error::Text { line, kind: TextError::Order });
            }
            previous = Some((row, column));
            row_ptr[row + 1] += 1;
            col_idx.push(column);
            values.push(value);
        }

        for row in 0..rows {
            row_ptr[row + 1] += row_ptr[row];
        }
        Ok(Csr { rows, cols, row_ptr, col_idx, values })
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