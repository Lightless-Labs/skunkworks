use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rational {
    num: i64,
    den: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseKind {
    Empty,
    InvalidByte,
    NonCanonical,
    OutOfRange,
    ZeroDenominator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RationalError {
    ZeroDenominator,
    DivideByZero,
    Overflow,
    Parse { at: usize, kind: ParseKind },
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

impl Rational {
    fn from_i128_parts(num: i128, den: i128) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        let (mut n, mut d) = (num, den);
        if d < 0 {
            n = -n;
            d = -d;
        }
        if n == 0 {
            return Ok(Self { num: 0, den: 1 });
        }
        let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
        n /= g;
        d /= g;
        if n < i64::MIN as i128 || n > i64::MAX as i128 || d < 1 || d > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }
        Ok(Self { num: n as i64, den: d as i64 })
    }

    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        let (mut n, mut d) = (num as i128, den as i128);
        if d < 0 {
            n = -n;
            d = -d;
        }
        if n == 0 {
            return Ok(Self { num: 0, den: 1 });
        }
        let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
        n /= g;
        d /= g;
        if n < i64::MIN as i128 || n > i64::MAX as i128 || d > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }
        Ok(Self { num: n as i64, den: d as i64 })
    }

    pub fn numerator(&self) -> i64 { self.num }
    pub fn denominator(&self) -> i64 { self.den }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128_parts(
            self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128_parts(
            self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128_parts(self.num as i128 * rhs.num as i128, self.den as i128 * rhs.den as i128)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 { return Err(RationalError::DivideByZero); }
        Self::from_i128_parts(self.num as i128 * rhs.den as i128, self.den as i128 * rhs.num as i128)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::from_i128_parts(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let b = text.as_bytes();
        if b.is_empty() {
            return Err(RationalError::Parse { at: 0, kind: ParseKind::Empty });
        }
        let mut i = 0;
        if b[0] == b'-' { i = 1; }
        let ns = i;
        while i < b.len() && b[i].is_ascii_digit() { i += 1; }
        if i == ns {
            return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
        }
        let ne = i;
        let mut ds = None;
        let mut de = None;
        if i < b.len() {
            if b[i] != b'/' {
                return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
            }
            i += 1;
            ds = Some(i);
            while i < b.len() && b[i].is_ascii_digit() { i += 1; }
            if i == ds.unwrap() {
                return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
            }
            de = Some(i);
            if i < b.len() {
                return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
            }
        }

        let parse_err = |at, kind| RationalError::Parse { at, kind };
        let neg = b[0] == b'-';
        let nstart = if neg { 1 } else { 0 };
        let ndigits = ne - nstart;
        if ndigits >= 2 && b[nstart] == b'0' {
            return Err(parse_err(nstart + 1, ParseKind::NonCanonical));
        }
        if neg && ndigits == 1 && b[1] == b'0' {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }
        if let (Some(s), Some(e)) = (ds, de) {
            if e - s >= 2 && b[s] == b'0' {
                return Err(parse_err(s + 1, ParseKind::NonCanonical));
            }
        }

        let limit: u128 = if neg { 1u128 << 63 } else { i64::MAX as u128 };
        let mut nm = 0u128;
        for &c in &b[nstart..ne] {
            nm = nm.saturating_mul(10).saturating_add((c - b'0') as u128);
            if nm > limit {
                return Err(parse_err(0, ParseKind::OutOfRange));
            }
        }
        let mut dm = 1u128;
        if let (Some(s), Some(e)) = (ds, de) {
            dm = 0;
            for &c in &b[s..e] {
                dm = dm.saturating_mul(10).saturating_add((c - b'0') as u128);
                if dm > i64::MAX as u128 {
                    return Err(parse_err(s, ParseKind::OutOfRange));
                }
            }
            if dm == 0 {
                return Err(parse_err(s, ParseKind::ZeroDenominator));
            }
            if dm == 1 {
                return Err(parse_err(s - 1, ParseKind::NonCanonical));
            }
            if nm == 0 {
                return Err(parse_err(s - 1, ParseKind::NonCanonical));
            }
            if gcd_u128(nm, dm) > 1 {
                return Err(parse_err(s - 1, ParseKind::NonCanonical));
            }
        }
        let n = if neg { -(nm as i128) } else { nm as i128 };
        Self::from_i128_parts(n, dm as i128)
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 { write!(f, "{}", self.num) }
        else { write!(f, "{}/{}", self.num, self.den) }
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
pub struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<Rational>,
}

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<Rational>) -> Result<Self, MatrixError> {
        let expected = rows.checked_mul(cols).ok_or(MatrixError::ShapeOverflow)?;
        if data.len() != expected {
            return Err(MatrixError::DataLength { expected, actual: data.len() });
        }
        Ok(Self { rows, cols, data })
    }

    pub fn rows(&self) -> usize { self.rows }
    pub fn cols(&self) -> usize { self.cols }
    pub fn data(&self) -> &[Rational] { &self.data }

    pub fn get(&self, row: usize, col: usize) -> Result<Rational, MatrixError> {
        if row >= self.rows || col >= self.cols {
            return Err(MatrixError::Index { row, col });
        }
        Ok(self.data[row * self.cols + col])
    }

    pub fn transpose(&self) -> Self {
        let mut data = Vec::with_capacity(self.data.len());
        for c in 0..self.cols {
            for r in 0..self.rows {
                data.push(self.data[r * self.cols + c]);
            }
        }
        Self { rows: self.cols, cols: self.rows, data }
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, MatrixError> {
        if self.cols != rhs.rows {
            return Err(MatrixError::DimensionMismatch {
                left: (self.rows, self.cols), right: (rhs.rows, rhs.cols),
            });
        }
        let len = self.rows.checked_mul(rhs.cols).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = Rational { num: 0, den: 1 };
                for k in 0..self.cols {
                    let a = self.data[r * self.cols + k];
                    let b = rhs.data[k * rhs.cols + c];
                    let term = a.mul(&b).map_err(|e| MatrixError::Arithmetic { row: r, col: c, step: k, source: e })?;
                    acc = acc.add(&term).map_err(|e| MatrixError::Arithmetic { row: r, col: c, step: k, source: e })?;
                }
                data.push(acc);
            }
        }
        Ok(Self { rows: self.rows, cols: rhs.cols, data })
    }

    fn eliminate(&self, candidate_cols: usize) -> Result<Reduction, MatrixError> {
        let mut m = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;
        for col in 0..candidate_cols {
            if p == m.rows { break; }
            let mut best: Option<usize> = None;
            for row in p..m.rows {
                let x = m.data[row * m.cols + col];
                if x.num == 0 { continue; }
                let key = (x.den, x.num.unsigned_abs(), row);
                if best.map_or(true, |br| {
                    let y = m.data[br * m.cols + col];
                    key < (y.den, y.num.unsigned_abs(), br)
                }) {
                    best = Some(row);
                }
            }
            let Some(q) = best else { continue };
            if q != p {
                for c in 0..m.cols { m.data.swap(p * m.cols + c, q * m.cols + c); }
                swaps.push((p, q));
            }
            let pivot = m.data[p * m.cols + col];
            for c in 0..m.cols {
                let v = m.data[p * m.cols + c].div(&pivot).map_err(|e| MatrixError::Arithmetic { row: p, col: c, step: p, source: e })?;
                m.data[p * m.cols + c] = v;
            }
            for row in 0..m.rows {
                if row == p { continue; }
                let factor = m.data[row * m.cols + col];
                if factor.num == 0 { continue; }
                for c in 0..m.cols {
                    let prod = factor.mul(&m.data[p * m.cols + c]).map_err(|e| MatrixError::Arithmetic { row, col: c, step: p, source: e })?;
                    let v = m.data[row * m.cols + c].sub(&prod).map_err(|e| MatrixError::Arithmetic { row, col: c, step: p, source: e })?;
                    m.data[row * m.cols + c] = v;
                }
            }
            pivots.push(col);
            p += 1;
        }
        Ok(Reduction { matrix: m, pivots, swaps })
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> { self.eliminate(self.cols) }

    pub fn rank(&self) -> Result<usize, MatrixError> { Ok(self.rref()?.rank()) }

    pub fn inverse(&self) -> Result<Self, MatrixError> {
        if self.rows != self.cols {
            return Err(MatrixError::NotSquare { rows: self.rows, cols: self.cols });
        }
        let n = self.rows;
        let width = n.checked_mul(2).ok_or(MatrixError::ShapeOverflow)?;
        let len = n.checked_mul(width).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        for r in 0..n {
            data.extend_from_slice(&self.data[r * n..(r + 1) * n]);
            for c in 0..n {
                data.push(if r == c { Rational { num: 1, den: 1 } } else { Rational { num: 0, den: 1 } });
            }
        }
        let aug = Self { rows: n, cols: width, data };
        let red = aug.eliminate(n)?;
        if red.pivots.len() < n {
            return Err(MatrixError::Singular { rank: red.pivots.len() });
        }
        let mut out = Vec::with_capacity(n * n);
        for r in 0..n {
            out.extend_from_slice(&red.matrix.data[r * width + n..r * width + width]);
        }
        Ok(Self { rows: n, cols: n, data: out })
    }

    pub fn solve(&self, rhs: &Self) -> Result<Solution, SolveError> {
        if self.rows != rhs.rows {
            return Err(SolveError::RowMismatch { coefficients: self.rows, rhs: rhs.rows });
        }
        let n = self.cols;
        let k = rhs.cols;
        let width = n.checked_add(k).ok_or(SolveError::AugmentedShapeOverflow)?;
        let len = self.rows.checked_mul(width).ok_or(SolveError::AugmentedShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        for r in 0..self.rows {
            data.extend_from_slice(&self.data[r * n..r * n + n]);
            data.extend_from_slice(&rhs.data[r * k..r * k + k]);
        }
        let aug = Self { rows: self.rows, cols: width, data };
        let red = aug.eliminate(n).map_err(SolveError::Matrix)?;

        for r in 0..self.rows {
            if red.matrix.data[r * width..r * width + n].iter().all(|x| x.num == 0) {
                if let Some(local) = red.matrix.data[r * width + n..r * width + width].iter().position(|x| x.num != 0) {
                    return Ok(Solution::None {
                        row: r,
                        rhs_col: local,
                        witness: red.matrix.data[r * width..r * width + width].to_vec(),
                    });
                }
            }
        }

        let rank = red.pivots.len();
        let particular_len = n.checked_mul(k).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut particular = vec![Rational { num: 0, den: 1 }; particular_len];
        for (j, &pc) in red.pivots.iter().enumerate() {
            for c in 0..k {
                particular[pc * k + c] = red.matrix.data[j * width + n + c];
            }
        }
        let particular = Matrix { rows: n, cols: k, data: particular };
        if rank == n {
            return Ok(Solution::Unique { values: particular, pivots: red.pivots });
        }

        let mut free = Vec::new();
        let mut pi = 0;
        for c in 0..n {
            if pi < red.pivots.len() && red.pivots[pi] == c { pi += 1; }
            else { free.push(c); }
        }
        let f = free.len();
        let basis_len = n.checked_mul(f).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut basis = vec![Rational { num: 0, den: 1 }; basis_len];
        for (j, &fc) in free.iter().enumerate() {
            basis[fc * f + j] = Rational { num: 1, den: 1 };
            for (i, &pc) in red.pivots.iter().enumerate() {
                let v = red.matrix.data[i * width + fc].neg().map_err(|source| {
                    SolveError::Matrix(MatrixError::Arithmetic { row: pc, col: j, step: rank, source })
                })?;
                basis[pc * f + j] = v;
            }
        }
        Ok(Solution::Infinite {
            particular,
            free,
            basis: Matrix { rows: n, cols: f, data: basis },
            pivots: red.pivots,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reduction {
    pub matrix: Matrix,
    pub pivots: Vec<usize>,
    pub swaps: Vec<(usize, usize)>,
}

impl Reduction {
    pub fn rank(&self) -> usize { self.pivots.len() }
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