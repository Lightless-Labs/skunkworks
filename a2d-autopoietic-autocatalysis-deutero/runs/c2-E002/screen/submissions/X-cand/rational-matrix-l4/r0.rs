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

fn abs_i64_u128(v: i64) -> u128 {
    if v < 0 { (-(v as i128)) as u128 } else { v as u128 }
}

impl Rational {
    fn from_i128(num: i128, den: i128) -> Result<Self, RationalError> {
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
        Self::from_i128(num as i128, den as i128)
    }

    pub fn numerator(&self) -> i64 { self.num }
    pub fn denominator(&self) -> i64 { self.den }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128(
            self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128(
            self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128(self.num as i128 * rhs.num as i128, self.den as i128 * rhs.den as i128)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 { return Err(RationalError::DivideByZero); }
        Self::from_i128(self.num as i128 * rhs.den as i128, self.den as i128 * rhs.num as i128)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::from_i128(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let b = text.as_bytes();
        let err = |at, kind| RationalError::Parse { at, kind };
        if b.is_empty() { return Err(err(0, ParseKind::Empty)); }

        let slash = b.iter().position(|&x| x == b'/');
        let n_end = slash.unwrap_or(b.len());
        let mut i = 0;
        if b[0] == b'-' { i = 1; }
        if i == n_end {
            return Err(err(i, ParseKind::InvalidByte));
        }
        while i < n_end {
            if !b[i].is_ascii_digit() { return Err(err(i, ParseKind::InvalidByte)); }
            i += 1;
        }
        if let Some(s) = slash {
            let mut j = s + 1;
            if j == b.len() { return Err(err(j, ParseKind::InvalidByte)); }
            while j < b.len() {
                if !b[j].is_ascii_digit() { return Err(err(j, ParseKind::InvalidByte)); }
                j += 1;
            }
        }

        let nstart = usize::from(b[0] == b'-');
        if n_end - nstart >= 2 && b[nstart] == b'0' {
            return Err(err(nstart + 1, ParseKind::NonCanonical));
        }
        if b[0] == b'-' && &b[..n_end] == b"-0" {
            return Err(err(0, ParseKind::NonCanonical));
        }
        let dstart = slash.map(|s| s + 1);
        if let Some(ds) = dstart {
            if b.len() - ds >= 2 && b[ds] == b'0' {
                return Err(err(ds + 1, ParseKind::NonCanonical));
            }
        }

        let parse_mag = |start: usize, end: usize, limit: u128| -> Option<u128> {
            let mut v = 0u128;
            for &c in &b[start..end] {
                v = v.checked_mul(10)?.checked_add((c - b'0') as u128)?;
                if v > limit { return None; }
            }
            Some(v)
        };
        let neg = b[0] == b'-';
        let nmag = parse_mag(nstart, n_end, if neg { 1u128 << 63 } else { i64::MAX as u128 })
            .ok_or_else(|| err(0, ParseKind::OutOfRange))?;
        let n = if neg {
            if nmag == 1u128 << 63 { i64::MIN } else { -(nmag as i64) }
        } else { nmag as i64 };

        let mut den = 1i64;
        if let Some(ds) = dstart {
            let dmag = parse_mag(ds, b.len(), i64::MAX as u128)
                .ok_or_else(|| err(ds, ParseKind::OutOfRange))?;
            if dmag == 0 { return Err(err(ds, ParseKind::ZeroDenominator)); }
            den = dmag as i64;
            if den == 1 { return Err(err(slash.unwrap(), ParseKind::NonCanonical)); }
            if n == 0 { return Err(err(slash.unwrap(), ParseKind::NonCanonical)); }
            if gcd_u128(abs_i64_u128(n), den as u128) > 1 {
                return Err(err(slash.unwrap(), ParseKind::NonCanonical));
            }
        }
        Ok(Self { num: n, den })
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 { write!(f, "{}", self.num) } else { write!(f, "{}/{}", self.num, self.den) }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<Rational>,
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
pub struct Reduction {
    pub matrix: Matrix,
    pub pivots: Vec<usize>,
    pub swaps: Vec<(usize, usize)>,
}

impl Reduction {
    pub fn rank(&self) -> usize { self.pivots.len() }
}

fn arithmetic(row: usize, col: usize, step: usize, source: RationalError) -> MatrixError {
    MatrixError::Arithmetic { row, col, step, source }
}

fn zero() -> Rational { Rational { num: 0, den: 1 } }
fn one() -> Rational { Rational { num: 1, den: 1 } }

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
        let mut data = vec![zero(); self.data.len()];
        for r in 0..self.rows {
            for c in 0..self.cols {
                data[c * self.rows + r] = self.data[r * self.cols + c];
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
                let mut acc = zero();
                for k in 0..self.cols {
                    let a = self.data[r * self.cols + k];
                    let b = rhs.data[k * rhs.cols + c];
                    let term = a.mul(&b).map_err(|e| arithmetic(r, c, k, e))?;
                    acc = acc.add(&term).map_err(|e| arithmetic(r, c, k, e))?;
                }
                data.push(acc);
            }
        }
        Ok(Self { rows: self.rows, cols: rhs.cols, data })
    }

    fn eliminate(&self, candidates: usize) -> Result<Reduction, MatrixError> {
        let mut m = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;
        for col in 0..candidates {
            if p == m.rows { break; }
            let mut best: Option<usize> = None;
            for r in p..m.rows {
                let v = m.data[r * m.cols + col];
                if v.num == 0 { continue; }
                let key = (v.den, abs_i64_u128(v.num), r);
                if best.map_or(true, |br| {
                    let w = m.data[br * m.cols + col];
                    key < (w.den, abs_i64_u128(w.num), br)
                }) { best = Some(r); }
            }
            let Some(q) = best else { continue; };
            if q != p {
                for c in 0..m.cols {
                    m.data.swap(p * m.cols + c, q * m.cols + c);
                }
                swaps.push((p, q));
            }
            let pivot = m.data[p * m.cols + col];
            for c in 0..m.cols {
                let v = m.data[p * m.cols + c];
                m.data[p * m.cols + c] = v.div(&pivot).map_err(|e| arithmetic(p, c, p, e))?;
            }
            for r in 0..m.rows {
                if r == p { continue; }
                let factor = m.data[r * m.cols + col];
                if factor.num == 0 { continue; }
                for c in 0..m.cols {
                    let prod = factor.mul(&m.data[p * m.cols + c])
                        .map_err(|e| arithmetic(r, c, p, e))?;
                    m.data[r * m.cols + c] = m.data[r * m.cols + c].sub(&prod)
                        .map_err(|e| arithmetic(r, c, p, e))?;
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
            for c in 0..n { data.push(self.data[r * n + c]); }
            for c in 0..n { data.push(if r == c { one() } else { zero() }); }
        }
        let aug = Self { rows: n, cols: width, data };
        let red = aug.eliminate(n)?;
        if red.pivots.len() < n { return Err(MatrixError::Singular { rank: red.pivots.len() }); }
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
            let coeff_zero = (0..n).all(|c| red.matrix.data[r * width + c].num == 0);
            if coeff_zero {
                for j in 0..k {
                    let v = red.matrix.data[r * width + n + j];
                    if v.num != 0 {
                        let witness = red.matrix.data[r * width..r * width + width].to_vec();
                        return Ok(Solution::None { row: r, rhs_col: j, witness });
                    }
                }
            }
        }

        let rank = red.pivots.len();
        let plen = n.checked_mul(k).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut particular = vec![zero(); plen];
        for (i, &pc) in red.pivots.iter().enumerate() {
            for j in 0..k {
                particular[pc * k + j] = red.matrix.data[i * width + n + j];
            }
        }
        let particular = Self { rows: n, cols: k, data: particular };
        if rank == n {
            return Ok(Solution::Unique { values: particular, pivots: red.pivots });
        }

        let free: Vec<usize> = (0..n).filter(|c| !red.pivots.contains(c)).collect();
        let f = free.len();
        let blen = n.checked_mul(f).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut basis = vec![zero(); blen];
        for (j, &fc) in free.iter().enumerate() {
            basis[fc * f + j] = one();
            for (i, &pc) in red.pivots.iter().enumerate() {
                let v = red.matrix.data[i * width + fc];
                basis[pc * f + j] = v.neg().map_err(|e| SolveError::Matrix(
                    MatrixError::Arithmetic { row: pc, col: j, step: rank, source: e }
                ))?;
            }
        }
        Ok(Solution::Infinite {
            particular, free, basis: Self { rows: n, cols: f, data: basis }, pivots: red.pivots,
        })
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