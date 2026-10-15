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

#[derive(Clone, Debug, PartialEq)]
pub struct Csr { rows: usize, cols: usize, row_ptr: Vec<usize>, col_idx: Vec<usize>, values: Vec<f64> }
impl Sparse {
    pub fn to_csr(&self) -> Csr {
        let mut row_ptr = vec![0; self.rows + 1];
        let mut col_idx = Vec::new();
        let mut values = Vec::new();
        for e in &self.entries {
            row_ptr[e.row + 1] += 1;
            col_idx.push(e.col);
            values.push(e.value);
        }
        for r in 0..self.rows { row_ptr[r + 1] += row_ptr[r]; }
        Csr { rows: self.rows, cols: self.cols, row_ptr, col_idx, values }
    }
}
impl Csr {
    pub fn from_parts(rows: usize, cols: usize, row_ptr: Vec<usize>, col_idx: Vec<usize>, values: Vec<f64>) -> Result<Self, Error> {
        if rows > MAX_DIM { return Err(Error::Shape { axis: Axis::Row, size: rows }); }
        if cols > MAX_DIM { return Err(Error::Shape { axis: Axis::Column, size: cols }); }
        if row_ptr.len() != rows + 1 { return Err(Error::RowPtrLength { expected: rows + 1, actual: row_ptr.len() }); }
        if col_idx.len() != values.len() { return Err(Error::ColumnValueLength { columns: col_idx.len(), values: values.len() }); }
        if row_ptr[0] != 0 { return Err(Error::RowPtrStart { found: row_ptr[0] }); }
        if row_ptr[rows] != values.len() { return Err(Error::RowPtrEnd { found: row_ptr[rows], expected: values.len() }); }
        for r in 0..rows {
            if row_ptr[r] > row_ptr[r + 1] { return Err(Error::RowPtrOrder { row: r }); }
        }
        for r in 0..rows {
            for s in row_ptr[r]..row_ptr[r + 1] {
                if col_idx[s] >= cols { return Err(Error::ColumnBounds { slot: s, column: col_idx[s], limit: cols }); }
                if s > row_ptr[r] && col_idx[s - 1] >= col_idx[s] { return Err(Error::ColumnOrder { slot: s }); }
                if !values[s].is_finite() { return Err(Error::NonFiniteValue { slot: s }); }
                if values[s] == 0.0 { return Err(Error::ExplicitZero { slot: s }); }
            }
        }
        Ok(Self { rows, cols, row_ptr, col_idx, values })
    }
    pub fn shape(&self) -> (usize, usize) { (self.rows, self.cols) }
    pub fn row_ptr(&self) -> &[usize] { &self.row_ptr }
    pub fn col_indices(&self) -> &[usize] { &self.col_idx }
    pub fn values(&self) -> &[f64] { &self.values }
    pub fn to_coo(&self) -> Sparse {
        let mut entries = Vec::new();
        for r in 0..self.rows {
            for s in self.row_ptr[r]..self.row_ptr[r + 1] {
                entries.push(Entry { row: r, col: self.col_idx[s], value: self.values[s] });
            }
        }
        Sparse { rows: self.rows, cols: self.cols, entries }
    }
    pub fn mul_vector(&self, x: &[f64]) -> Result<Vec<f64>, Error> {
        if x.len() != self.cols { return Err(Error::VectorLength { expected: self.cols, actual: x.len() }); }
        for (index, v) in x.iter().enumerate() {
            if !v.is_finite() { return Err(Error::NonFiniteVector { index }); }
        }
        let mut out = vec![0.0; self.rows];
        for r in 0..self.rows {
            let mut sum = 0.0;
            for s in self.row_ptr[r]..self.row_ptr[r + 1] {
                let column = self.col_idx[s];
                let p = self.values[s] * x[column];
                if !p.is_finite() { return Err(Error::ProductOverflow { row: r, column }); }
                sum += p;
                if !sum.is_finite() { return Err(Error::AccumulationOverflow { row: r, column }); }
            }
            out[r] = if sum == 0.0 { 0.0 } else { sum };
        }
        Ok(out)
    }
}

impl Csr {
    pub fn transpose(&self) -> Csr {
        let mut row_ptr = vec![0; self.cols + 1];
        for &c in &self.col_idx { row_ptr[c + 1] += 1; }
        for r in 0..self.cols { row_ptr[r + 1] += row_ptr[r]; }
        let mut next = row_ptr.clone();
        let mut col_idx = vec![0; self.values.len()];
        let mut values = vec![0.0; self.values.len()];
        for r in 0..self.rows {
            for s in self.row_ptr[r]..self.row_ptr[r + 1] {
                let c = self.col_idx[s];
                let dst = next[c];
                col_idx[dst] = r;
                values[dst] = self.values[s];
                next[c] += 1;
            }
        }
        Csr { rows: self.cols, cols: self.rows, row_ptr, col_idx, values }
    }
    pub fn multiply(&self, rhs: &Csr) -> Result<Csr, Error> {
        if self.cols != rhs.rows { return Err(Error::DimensionMismatch { left_cols: self.cols, right_rows: rhs.rows }); }
        let mut row_ptr = vec![0];
        let mut col_idx = Vec::new();
        let mut values = Vec::new();
        for r in 0..self.rows {
            let mut sums = BTreeMap::new();
            for a in self.row_ptr[r]..self.row_ptr[r + 1] {
                let k = self.col_idx[a];
                for b in rhs.row_ptr[k]..rhs.row_ptr[k + 1] {
                    let c = rhs.col_idx[b];
                    let p = self.values[a] * rhs.values[b];
                    if !p.is_finite() { return Err(Error::MultiplyOverflow { row: r, inner: k, column: c }); }
                    let old = *sums.get(&c).unwrap_or(&0.0);
                    let sum: f64 = old + p;
                    if !sum.is_finite() { return Err(Error::MultiplySumOverflow { row: r, inner: k, column: c }); }
                    sums.insert(c, sum);
                }
            }
            for (c, v) in sums {
                if v != 0.0 { col_idx.push(c); values.push(v); }
            }
            row_ptr.push(values.len());
        }
        Ok(Csr { rows: self.rows, cols: rhs.cols, row_ptr, col_idx, values })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextError { Newline, Header, Decimal, Dimension, Count, Record, Hex, RowBounds, ColumnBounds, NonFinite, Zero, Order }

fn text_err(line: usize, kind: TextError) -> Error { Error::Text { line, kind } }
fn decimal(s: &str, line: usize) -> Result<usize, Error> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || (s.len() > 1 && s.starts_with('0')) {
        return Err(text_err(line, TextError::Decimal));
    }
    s.parse().map_err(|_| text_err(line, TextError::Decimal))
}
impl Csr {
    pub fn prune(&self, tolerance: f64) -> Result<Csr, Error> {
        if !tolerance.is_finite() || tolerance < 0.0 { return Err(Error::Tolerance); }
        let mut row_ptr = vec![0];
        let mut col_idx = Vec::new();
        let mut values = Vec::new();
        for r in 0..self.rows {
            for s in self.row_ptr[r]..self.row_ptr[r + 1] {
                if self.values[s].abs() >= tolerance {
                    col_idx.push(self.col_idx[s]);
                    values.push(self.values[s]);
                }
            }
            row_ptr.push(values.len());
        }
        Ok(Csr { rows: self.rows, cols: self.cols, row_ptr, col_idx, values })
    }
    pub fn write_text(&self) -> String {
        use std::fmt::Write;
        let mut out = format!("SM1 {} {} {}\n", self.rows, self.cols, self.values.len());
        for r in 0..self.rows {
            for s in self.row_ptr[r]..self.row_ptr[r + 1] {
                writeln!(&mut out, "{} {} {:016x}", r, self.col_idx[s], self.values[s].to_bits()).unwrap();
            }
        }
        out
    }
    pub fn parse_text(text: &str) -> Result<Csr, Error> {
        if !text.ends_with('\n') { return Err(text_err(text.bytes().filter(|&b| b == b'\n').count() + 1, TextError::Newline)); }
        let lines: Vec<_> = text.split_terminator('\n').collect();
        let h: Vec<_> = lines[0].split(' ').collect();
        if h.len() != 4 || h.iter().any(|x| x.is_empty()) || h[0] != "SM1" { return Err(text_err(1, TextError::Header)); }
        let rows = decimal(h[1], 1)?;
        let cols = decimal(h[2], 1)?;
        let count = decimal(h[3], 1)?;
        if rows > MAX_DIM || cols > MAX_DIM { return Err(text_err(1, TextError::Dimension)); }
        if lines.len() - 1 != count { return Err(text_err(1, TextError::Count)); }
        let mut entries = Vec::new();
        let mut previous = None;
        for (i, line) in lines.iter().enumerate().skip(1) {
            let line_no = i + 1;
            let fields: Vec<_> = line.split(' ').collect();
            if fields.len() != 3 || fields.iter().any(|x| x.is_empty()) { return Err(text_err(line_no, TextError::Record)); }
            let row = decimal(fields[0], line_no)?;
            let col = decimal(fields[1], line_no)?;
            let hex = fields[2];
            if hex.len() != 16 || !hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) { return Err(text_err(line_no, TextError::Hex)); }
            let value = f64::from_bits(u64::from_str_radix(hex, 16).unwrap());
            if row >= rows { return Err(text_err(line_no, TextError::RowBounds)); }
            if col >= cols { return Err(text_err(line_no, TextError::ColumnBounds)); }
            if !value.is_finite() { return Err(text_err(line_no, TextError::NonFinite)); }
            if value == 0.0 { return Err(text_err(line_no, TextError::Zero)); }
            if previous.is_some_and(|p| p >= (row, col)) { return Err(text_err(line_no, TextError::Order)); }
            previous = Some((row, col));
            entries.push(Entry { row, col, value });
        }
        Ok(Sparse { rows, cols, entries }.to_csr())
    }
}
