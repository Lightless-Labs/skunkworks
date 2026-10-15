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
                let p = self.values[s] + x[column];
                if !p.is_finite() { return Err(Error::ProductOverflow { row: r, column }); }
                sum += p;
                if !sum.is_finite() { return Err(Error::AccumulationOverflow { row: r, column }); }
            }
            out[r] = if sum == 0.0 { 0.0 } else { sum };
        }
        Ok(out)
    }
}
