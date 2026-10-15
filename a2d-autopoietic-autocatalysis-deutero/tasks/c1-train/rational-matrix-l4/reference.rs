use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseKind { Empty, InvalidByte, NonCanonical, OutOfRange, ZeroDenominator }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RationalError {
    ZeroDenominator,
    DivideByZero,
    Overflow,
    Parse { at: usize, kind: ParseKind },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rational { num: i64, den: i64 }

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 { let r = a % b; a = b; b = r; }
    a
}
impl Rational {
    fn reduced(mut n: i128, mut d: i128) -> Result<Self, RationalError> {
        if d == 0 { return Err(RationalError::ZeroDenominator); }
        if n == 0 { return Ok(Self { num: 0, den: 1 }); }
        if d < 0 { n = -n; d = -d; }
        let g = gcd(n.unsigned_abs(), d as u128) as i128;
        n /= g;
        d /= g;
        if n < i64::MIN as i128 || n > i64::MAX as i128 || d > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }
        Ok(Self { num: n as i64, den: d as i64 })
    }
    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        Self::reduced(num as i128, den as i128)
    }
    pub fn numerator(&self) -> i64 { self.num }
    pub fn denominator(&self) -> i64 { self.den }
    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::reduced(self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128,
                      self.den as i128 * rhs.den as i128)
    }
    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::reduced(self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128,
                      self.den as i128 * rhs.den as i128)
    }
    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::reduced(self.num as i128 * rhs.num as i128, self.den as i128 * rhs.den as i128)
    }
    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 { return Err(RationalError::DivideByZero); }
        Self::reduced(self.num as i128 * rhs.den as i128, self.den as i128 * rhs.num as i128)
    }
    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::reduced(-(self.num as i128), self.den as i128)
    }
    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let err = |at, kind| RationalError::Parse { at, kind };
        let b = text.as_bytes();
        if b.is_empty() { return Err(err(0, ParseKind::Empty)); }
        let neg = b[0] == b'-';
        let start = usize::from(neg);
        let mut i = start;
        if i == b.len() || !b[i].is_ascii_digit() { return Err(err(i, ParseKind::InvalidByte)); }
        while i < b.len() && b[i].is_ascii_digit() { i += 1; }
        let nend = i;
        let slash = if i == b.len() { None } else {
            if b[i] != b'/' { return Err(err(i, ParseKind::InvalidByte)); }
            let s = i;
            i += 1;
            if i == b.len() || !b[i].is_ascii_digit() { return Err(err(i, ParseKind::InvalidByte)); }
            while i < b.len() && b[i].is_ascii_digit() { i += 1; }
            if i != b.len() { return Err(err(i, ParseKind::InvalidByte)); }
            Some(s)
        };
        // Syntax is checked in full before any canonicality or magnitude check.
        if nend - start > 1 && b[start] == b'0' { return Err(err(start + 1, ParseKind::NonCanonical)); }
        if neg && nend - start == 1 && b[start] == b'0' { return Err(err(0, ParseKind::NonCanonical)); }
        if let Some(s) = slash {
            if b.len() - s - 1 > 1 && b[s + 1] == b'0' { return Err(err(s + 2, ParseKind::NonCanonical)); }
        }
        let magnitude = |digits: &[u8], limit: u128| -> Option<u128> {
            let mut v = 0u128;
            for &c in digits {
                v = v.checked_mul(10)?.checked_add((c - b'0') as u128)?;
                if v > limit { return None; }
            }
            Some(v)
        };
        let lim = if neg { 1u128 << 63 } else { i64::MAX as u128 };
        let mag = magnitude(&b[start..nend], lim).ok_or(err(0, ParseKind::OutOfRange))?;
        let n = if neg { -(mag as i128) } else { mag as i128 };
        let d = if let Some(s) = slash {
            magnitude(&b[s + 1..], i64::MAX as u128).ok_or(err(s + 1, ParseKind::OutOfRange))?
        } else { 1 };
        if let Some(s) = slash {
            if d == 0 { return Err(err(s + 1, ParseKind::ZeroDenominator)); }
            if d == 1 { return Err(err(s, ParseKind::NonCanonical)); }
            if n == 0 { return Err(err(s, ParseKind::NonCanonical)); }
            if gcd(n.unsigned_abs(), d) != 1 { return Err(err(s, ParseKind::NonCanonical)); }
        }
        Ok(Self { num: n as i64, den: d as i64 })
    }
}
impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 { write!(f, "{}", self.num) } else { write!(f, "{}/{}", self.num, self.den) }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatrixError {
    ShapeOverflow,
    DataLength { expected: usize, actual: usize },
    Index { row: usize, col: usize },
    DimensionMismatch { left: (usize, usize), right: (usize, usize) },
    Arithmetic { row: usize, col: usize, step: usize, source: RationalError },
    NotSquare { rows: usize, cols: usize },
    Singular { rank: usize },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Matrix { rows: usize, cols: usize, data: Vec<Rational> }
impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<Rational>) -> Result<Self, MatrixError> {
        let expected = rows.checked_mul(cols).ok_or(MatrixError::ShapeOverflow)?;
        if data.len() != expected { return Err(MatrixError::DataLength { expected, actual: data.len() }); }
        Ok(Self { rows, cols, data })
    }
    pub fn rows(&self) -> usize { self.rows }
    pub fn cols(&self) -> usize { self.cols }
    pub fn data(&self) -> &[Rational] { &self.data }
    pub fn get(&self, row: usize, col: usize) -> Result<Rational, MatrixError> {
        if row >= self.rows || col >= self.cols { return Err(MatrixError::Index { row, col }); }
        Ok(self.data[row * self.cols + col])
    }
    pub fn transpose(&self) -> Self {
        let mut data = vec![Rational { num: 0, den: 1 }; self.data.len()];
        for (i, &v) in self.data.iter().enumerate() {
            data[(i % self.cols) * self.rows + i / self.cols] = v;
        }
        Self { rows: self.cols, cols: self.rows, data }
    }
    pub fn mul(&self, rhs: &Self) -> Result<Self, MatrixError> {
        if self.cols != rhs.rows {
            return Err(MatrixError::DimensionMismatch { left: (self.rows, self.cols), right: (rhs.rows, rhs.cols) });
        }
        let len = self.rows.checked_mul(rhs.cols).ok_or(MatrixError::ShapeOverflow)?;
        let mut out = vec![Rational { num: 0, den: 1 }; len];
        for (index, slot) in out.iter_mut().enumerate() {
            let r = index / rhs.cols;
            let c = index % rhs.cols;
            for k in 0..self.cols {
                let map = |source| MatrixError::Arithmetic { row: r, col: c, step: k, source };
                let term = self.data[r * self.cols + k].mul(&rhs.data[k * rhs.cols + c]).map_err(map)?;
                *slot = slot.add(&term).map_err(map)?;
            }
        }
        Self::new(self.rows, rhs.cols, out)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reduction { pub matrix: Matrix, pub pivots: Vec<usize>, pub swaps: Vec<(usize, usize)> }
impl Reduction { pub fn rank(&self) -> usize { self.pivots.len() } }
impl Matrix {
    // Eliminate only columns before limit; update the entire augmented width.
    fn eliminate(&self, limit: usize) -> Result<Reduction, MatrixError> {
        let mut a = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0;
        for col in 0..limit {
            if p == a.rows { break; }
            let chosen = (p..a.rows).filter(|&r| a.data[r * a.cols + col].num != 0)
                .min_by_key(|&r| {
                    let v = a.data[r * a.cols + col];
                    (v.den, v.num.unsigned_abs(), r)
                });
            let Some(q) = chosen else { continue; };
            if q != p {
                for c in 0..a.cols { a.data.swap(p * a.cols + c, q * a.cols + c); }
                swaps.push((p, q));
            }
            let pivot = a.data[p * a.cols + col];
            for c in 0..a.cols {
                let map = |source| MatrixError::Arithmetic { row: p, col: c, step: p, source };
                a.data[p * a.cols + c] = a.data[p * a.cols + c].div(&pivot).map_err(map)?;
            }
            for r in 0..a.rows {
                if r == p { continue; }
                let factor = a.data[r * a.cols + col];
                if factor.num == 0 { continue; }
                for c in 0..a.cols {
                    let map = |source| MatrixError::Arithmetic { row: r, col: c, step: p, source };
                    let term = factor.mul(&a.data[p * a.cols + c]).map_err(map)?;
                    a.data[r * a.cols + c] = a.data[r * a.cols + c].sub(&term).map_err(map)?;
                }
            }
            pivots.push(col);
            p += 1;
        }
        Ok(Reduction { matrix: a, pivots, swaps })
    }
    pub fn rref(&self) -> Result<Reduction, MatrixError> { self.eliminate(self.cols) }
    pub fn rank(&self) -> Result<usize, MatrixError> { Ok(self.rref()?.rank()) }
    pub fn inverse(&self) -> Result<Self, MatrixError> {
        if self.rows != self.cols { return Err(MatrixError::NotSquare { rows: self.rows, cols: self.cols }); }
        let n = self.rows;
        let width = n.checked_mul(2).ok_or(MatrixError::ShapeOverflow)?;
        let len = n.checked_mul(width).ok_or(MatrixError::ShapeOverflow)?;
        let zero = Rational { num: 0, den: 1 };
        let one = Rational { num: 1, den: 1 };
        let mut data = vec![zero; len];
        for r in 0..n {
            for c in 0..n { data[r * width + c] = self.data[r * n + c]; }
            data[r * width + n + r] = one;
        }
        let reduced = Self::new(n, width, data)?.eliminate(n)?;
        if reduced.rank() != n { return Err(MatrixError::Singular { rank: reduced.rank() }); }
        let mut result = Vec::with_capacity(self.data.len());
        for r in 0..n { result.extend_from_slice(&reduced.matrix.data[r * width + n..(r + 1) * width]); }
        Self::new(n, n, result)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SolveError {
    RowMismatch { coefficients: usize, rhs: usize },
    AugmentedShapeOverflow,
    Matrix(MatrixError),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Solution {
    Unique { values: Matrix, pivots: Vec<usize> },
    None { row: usize, rhs_col: usize, witness: Vec<Rational> },
    Infinite { particular: Matrix, free: Vec<usize>, basis: Matrix, pivots: Vec<usize> },
}
impl Matrix {
    pub fn solve(&self, rhs: &Self) -> Result<Solution, SolveError> {
        if self.rows != rhs.rows {
            return Err(SolveError::RowMismatch { coefficients: self.rows, rhs: rhs.rows });
        }
        let width = self.cols.checked_add(rhs.cols).ok_or(SolveError::AugmentedShapeOverflow)?;
        let len = self.rows.checked_mul(width).ok_or(SolveError::AugmentedShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        if len != 0 {
            for r in 0..self.rows {
                data.extend_from_slice(&self.data[r * self.cols..(r + 1) * self.cols]);
                data.extend_from_slice(&rhs.data[r * rhs.cols..(r + 1) * rhs.cols]);
            }
        }
        let reduced = Self::new(self.rows, width, data).map_err(SolveError::Matrix)?
            .eliminate(self.cols).map_err(SolveError::Matrix)?;
        let a = &reduced.matrix;
        for r in 0..if rhs.cols == 0 { 0 } else { self.rows } {
            if a.data[r * width..r * width + self.cols].iter().all(|v| v.num == 0) {
                for c in 0..rhs.cols {
                    if a.data[r * width + self.cols + c].num != 0 {
                        return Ok(Solution::None { row: r, rhs_col: c,
                            witness: a.data[r * width..(r + 1) * width].to_vec() });
                    }
                }
            }
        }
        let zero = Rational { num: 0, den: 1 };
        let one = Rational { num: 1, den: 1 };
        let plen = self.cols.checked_mul(rhs.cols).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut particular = vec![zero; plen];
        for (r, &p) in reduced.pivots.iter().enumerate() {
            for c in 0..rhs.cols { particular[p * rhs.cols + c] = a.data[r * width + self.cols + c]; }
        }
        let particular = Self::new(self.cols, rhs.cols, particular).map_err(SolveError::Matrix)?;
        if reduced.rank() == self.cols {
            return Ok(Solution::Unique { values: particular, pivots: reduced.pivots });
        }
        let fcount = self.cols - reduced.rank();
        let blen = self.cols.checked_mul(fcount).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let free: Vec<usize> = (0..self.cols).filter(|c| !reduced.pivots.contains(c)).collect();
        let mut basis = vec![zero; blen];
        for (j, &f) in free.iter().enumerate() {
            basis[f * free.len() + j] = one;
            for (r, &p) in reduced.pivots.iter().enumerate() {
                basis[p * free.len() + j] = a.data[r * width + f].neg()
                    .map_err(|source| SolveError::Matrix(MatrixError::Arithmetic { row: p, col: j, step: reduced.rank(), source }))?;
            }
        }
        let basis = Self::new(self.cols, free.len(), basis).map_err(SolveError::Matrix)?;
        Ok(Solution::Infinite { particular, free, basis, pivots: reduced.pivots })
    }
}
