use std::fmt;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rational {
    num: i64,
    den: i64,
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
        if n < i64::MIN as i128 || n > i64::MAX as i128 || d > i64::MAX as i128 {
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
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        Self::from_i128(self.num as i128 * rhs.den as i128, self.den as i128 * rhs.num as i128)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::from_i128(-(self.num as i128), self.den as i128)
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
        if i < b.len() {
            if b[i] != b'/' {
                return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
            }
            i += 1;
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() { i += 1; }
            if i == start {
                return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
            }
            ds = Some((start, i));
            if i < b.len() {
                return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
            }
        }

        let parse_err = |at, kind| RationalError::Parse { at, kind };
        let negative = b[0] == b'-';
        let ndigits = ne - ns;
        if ndigits >= 2 && b[ns] == b'0' {
            return Err(parse_err(ns + 1, ParseKind::NonCanonical));
        }
        if negative && ndigits == 1 && b[ns] == b'0' {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }
        if let Some((s, e)) = ds {
            if e - s >= 2 && b[s] == b'0' {
                return Err(parse_err(s + 1, ParseKind::NonCanonical));
            }
        }

        let nmag = parse_digits(&b[ns..ne]).ok_or_else(|| parse_err(0, ParseKind::OutOfRange))?;
        let limit = if negative { 1u128 << 63 } else { i64::MAX as u128 };
        if nmag > limit {
            return Err(parse_err(0, ParseKind::OutOfRange));
        }
        let n = if negative {
            if nmag == 1u128 << 63 { i64::MIN } else { -(nmag as i64) }
        } else { nmag as i64 };

        let (d, slash) = if let Some((s, e)) = ds {
            let dmag = parse_digits(&b[s..e]).ok_or_else(|| parse_err(s, ParseKind::OutOfRange))?;
            if dmag > i64::MAX as u128 {
                return Err(parse_err(s, ParseKind::OutOfRange));
            }
            if dmag == 0 {
                return Err(parse_err(s, ParseKind::ZeroDenominator));
            }
            (dmag as i64, s - 1)
        } else {
            return Ok(Self { num: n, den: 1 });
        };

        if d == 1 {
            return Err(parse_err(slash, ParseKind::NonCanonical));
        }
        if n == 0 {
            return Err(parse_err(slash, ParseKind::NonCanonical));
        }
        if gcd_u128(n.unsigned_abs() as u128, d as u128) > 1 {
            return Err(parse_err(slash, ParseKind::NonCanonical));
        }
        Ok(Self { num: n, den: d })
    }
}

fn parse_digits(bytes: &[u8]) -> Option<u128> {
    let mut n = 0u128;
    for &c in bytes {
        n = n.checked_mul(10)?.checked_add((c - b'0') as u128)?;
    }
    Some(n)
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
        for r in 0..self.cols {
            for c in 0..self.rows {
                data.push(self.data[c * self.cols + r]);
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
        let mut out = Vec::with_capacity(len);
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = Rational { num: 0, den: 1 };
                for k in 0..self.cols {
                    let a = self.data[r * self.cols + k];
                    let b = rhs.data[k * rhs.cols + c];
                    let term = a.mul(&b).map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: k, source })?;
                    acc = acc.add(&term).map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: k, source })?;
                }
                out.push(acc);
            }
        }
        Ok(Self { rows: self.rows, cols: rhs.cols, data: out })
    }
    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        eliminate(self.clone(), self.cols, self.cols)
    }
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
            for c in 0..n { data.push(if r == c { Rational { num: 1, den: 1 } } else { Rational { num: 0, den: 1 } }); }
        }
        let red = eliminate(Matrix { rows: n, cols: width, data }, n, width)?;
        if red.pivots.len() < n { return Err(MatrixError::Singular { rank: red.pivots.len() }); }
        let mut right = Vec::with_capacity(n * n);
        for r in 0..n { for c in 0..n { right.push(red.matrix.data[r * width + n + c]); } }
        Ok(Matrix { rows: n, cols: n, data: right })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reduction {
    pub matrix: Matrix,
    pub pivots: Vec<usize>,
    pub swaps: Vec<(usize, usize)>,
}
impl Reduction { pub fn rank(&self) -> usize { self.pivots.len() } }

fn eliminate(mut matrix: Matrix, candidates: usize, update_cols: usize) -> Result<Reduction, MatrixError> {
    let mut pivots = Vec::new();
    let mut swaps = Vec::new();
    let mut p = 0;
    for col in 0..candidates {
        if p == matrix.rows { break; }
        let mut best: Option<usize> = None;
        for r in p..matrix.rows {
            let v = matrix.data[r * matrix.cols + col];
            if v.num == 0 { continue; }
            let key = (v.den, v.num.unsigned_abs(), r);
            if best.map_or(true, |br| {
                let bv = matrix.data[br * matrix.cols + col];
                key < (bv.den, bv.num.unsigned_abs(), br)
            }) { best = Some(r); }
        }
        let Some(q) = best else { continue; };
        if q != p {
            for c in 0..update_cols {
                matrix.data.swap(p * matrix.cols + c, q * matrix.cols + c);
            }
            swaps.push((p, q));
        }
        let pivot = matrix.data[p * matrix.cols + col];
        for c in 0..update_cols {
            let cur = matrix.data[p * matrix.cols + c];
            matrix.data[p * matrix.cols + c] = cur.div(&pivot)
                .map_err(|source| MatrixError::Arithmetic { row: p, col: c, step: p, source })?;
        }
        for r in 0..matrix.rows {
            if r == p { continue; }
            let factor = matrix.data[r * matrix.cols + col];
            if factor.num == 0 { continue; }
            for c in 0..update_cols {
                let product = factor.mul(&matrix.data[p * matrix.cols + c])
                    .map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: p, source })?;
                matrix.data[r * matrix.cols + c] = matrix.data[r * matrix.cols + c].sub(&product)
                    .map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: p, source })?;
            }
        }
        pivots.push(col);
        p += 1;
    }
    Ok(Reduction { matrix, pivots, swaps })
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
        let n = self.cols;
        let k = rhs.cols;
        let width = n.checked_add(k).ok_or(SolveError::AugmentedShapeOverflow)?;
        let len = self.rows.checked_mul(width).ok_or(SolveError::AugmentedShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        for r in 0..self.rows {
            for c in 0..n { data.push(self.data[r * n + c]); }
            for c in 0..k { data.push(rhs.data[r * k + c]); }
        }
        let red = eliminate(Matrix { rows: self.rows, cols: width, data }, n, width)
            .map_err(SolveError::Matrix)?;
        for r in 0..self.rows {
            if (0..n).all(|c| red.matrix.data[r * width + c].num == 0) {
                for c in 0..k {
                    if red.matrix.data[r * width + n + c].num != 0 {
                        return Ok(Solution::None {
                            row: r, rhs_col: c,
                            witness: red.matrix.data[r * width..(r + 1) * width].to_vec(),
                        });
                    }
                }
            }
        }
        let rank = red.pivots.len();
        let particular_len = n.checked_mul(k).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut particular = vec![Rational { num: 0, den: 1 }; particular_len];
        for (i, &col) in red.pivots.iter().enumerate() {
            for c in 0..k {
                particular[col * k + c] = red.matrix.data[i * width + n + c];
            }
        }
        let particular = Matrix { rows: n, cols: k, data: particular };
        if rank == n {
            return Ok(Solution::Unique { values: particular, pivots: red.pivots });
        }
        let mut free = Vec::new();
        let mut is_pivot = vec![false; n];
        for &p in &red.pivots { is_pivot[p] = true; }
        for c in 0..n { if !is_pivot[c] { free.push(c); } }
        let f = free.len();
        let basis_len = n.checked_mul(f).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut basis = vec![Rational { num: 0, den: 1 }; basis_len];
        for (j, &free_col) in free.iter().enumerate() {
            basis[free_col * f + j] = Rational { num: 1, den: 1 };
            for (i, &pivot_col) in red.pivots.iter().enumerate() {
                let coeff = red.matrix.data[i * width + free_col];
                let neg = coeff.neg().map_err(|source| SolveError::Matrix(MatrixError::Arithmetic {
                    row: pivot_col, col: j, step: rank, source,
                }))?;
                basis[pivot_col * f + j] = neg;
            }
        }
        Ok(Solution::Infinite {
            particular, free,
            basis: Matrix { rows: n, cols: f, data: basis },
            pivots: red.pivots,
        })
    }
}
