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

fn rational_from_i128(num: i128, den: i128) -> Result<Rational, RationalError> {
    if den == 0 {
        return Err(RationalError::ZeroDenominator);
    }
    let (mut n, mut d) = (num, den);
    if d < 0 {
        n = n.checked_neg().ok_or(RationalError::Overflow)?;
        d = d.checked_neg().ok_or(RationalError::Overflow)?;
    }
    if n == 0 {
        return Ok(Rational { num: 0, den: 1 });
    }
    let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
    n /= g;
    d /= g;
    if n < i64::MIN as i128 || n > i64::MAX as i128 || d > i64::MAX as i128 {
        return Err(RationalError::Overflow);
    }
    Ok(Rational { num: n as i64, den: d as i64 })
}

impl Rational {
    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        rational_from_i128(num as i128, den as i128)
    }

    pub fn numerator(&self) -> i64 { self.num }
    pub fn denominator(&self) -> i64 { self.den }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = (self.num as i128) * (rhs.den as i128)
            + (rhs.num as i128) * (self.den as i128);
        let d = (self.den as i128) * (rhs.den as i128);
        rational_from_i128(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = (self.num as i128) * (rhs.den as i128)
            - (rhs.num as i128) * (self.den as i128);
        let d = (self.den as i128) * (rhs.den as i128);
        rational_from_i128(n, d)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_i128(
            (self.num as i128) * (rhs.num as i128),
            (self.den as i128) * (rhs.den as i128),
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        rational_from_i128(
            (self.num as i128) * (rhs.den as i128),
            (self.den as i128) * (rhs.num as i128),
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        let n = (self.num as i128).checked_neg().ok_or(RationalError::Overflow)?;
        rational_from_i128(n, self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        let err = |at, kind| RationalError::Parse { at, kind };
        if bytes.is_empty() {
            return Err(err(0, ParseKind::Empty));
        }

        let mut i = 0;
        if bytes[i] == b'-' { i += 1; }
        let num_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
        if i == num_start {
            return Err(err(i, if i == bytes.len() { ParseKind::InvalidByte } else { ParseKind::InvalidByte }));
        }
        let num_end = i;
        let mut den_start = None;
        let mut den_end = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(err(i, ParseKind::InvalidByte));
            }
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
            if i == start {
                return Err(err(i, ParseKind::InvalidByte));
            }
            den_start = Some(start);
            den_end = Some(i);
            if i < bytes.len() {
                return Err(err(i, ParseKind::InvalidByte));
            }
        }

        let negative = bytes[0] == b'-';
        let num_digits = num_end - num_start;
        if num_digits >= 2 && bytes[num_start] == b'0' {
            return Err(err(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_digits == 1 && bytes[num_start] == b'0' {
            return Err(err(0, ParseKind::NonCanonical));
        }
        if let (Some(s), Some(e)) = (den_start, den_end) {
            if e - s >= 2 && bytes[s] == b'0' {
                return Err(err(s + 1, ParseKind::NonCanonical));
            }
        }

        let num_mag = parse_digits_u128(&bytes[num_start..num_end])
            .filter(|&v| v <= if negative { 1u128 << 63 } else { i64::MAX as u128 });
        let Some(num_mag) = num_mag else { return Err(err(0, ParseKind::OutOfRange)); };
        let num = if negative {
            if num_mag == 1u128 << 63 { i64::MIN } else { -(num_mag as i64) }
        } else {
            num_mag as i64
        };

        if let (Some(s), Some(e)) = (den_start, den_end) {
            let Some(dmag) = parse_digits_u128(&bytes[s..e]).filter(|&v| v <= i64::MAX as u128) else {
                return Err(err(s, ParseKind::OutOfRange));
            };
            if dmag == 0 {
                return Err(err(s, ParseKind::ZeroDenominator));
            }
            if dmag == 1 {
                return Err(err(s - 1, ParseKind::NonCanonical));
            }
            if num == 0 {
                return Err(err(s - 1, ParseKind::NonCanonical));
            }
            if gcd_u128(num.unsigned_abs() as u128, dmag) > 1 {
                return Err(err(s - 1, ParseKind::NonCanonical));
            }
            Ok(Rational { num, den: dmag as i64 })
        } else {
            Ok(Rational { num, den: 1 })
        }
    }
}

fn parse_digits_u128(bytes: &[u8]) -> Option<u128> {
    let mut value = 0u128;
    for &b in bytes {
        value = value.checked_mul(10)?.checked_add((b - b'0') as u128)?;
    }
    Some(value)
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
        let mut data = vec![Rational { num: 0, den: 1 }; self.data.len()];
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
                let mut acc = Rational { num: 0, den: 1 };
                for k in 0..self.cols {
                    let left = self.data[r * self.cols + k];
                    let right = rhs.data[k * rhs.cols + c];
                    let term = left.mul(&right).map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: k, source })?;
                    acc = acc.add(&term).map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: k, source })?;
                }
                data.push(acc);
            }
        }
        Ok(Self { rows: self.rows, cols: rhs.cols, data })
    }

    fn eliminate(&mut self, candidate_cols: usize) -> Result<(Vec<usize>, Vec<(usize, usize)>), MatrixError> {
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0;
        for col in 0..candidate_cols {
            if p == self.rows { break; }
            let mut chosen: Option<usize> = None;
            for row in p..self.rows {
                let value = self.data[row * self.cols + col];
                if value.num == 0 { continue; }
                let key = (value.den, value.num.unsigned_abs(), row);
                if chosen.map_or(true, |old| {
                    let v = self.data[old * self.cols + col];
                    key < (v.den, v.num.unsigned_abs(), old)
                }) {
                    chosen = Some(row);
                }
            }
            let Some(q) = chosen else { continue; };
            if q != p {
                for c in 0..self.cols {
                    self.data.swap(p * self.cols + c, q * self.cols + c);
                }
                swaps.push((p, q));
            }
            let pivot = self.data[p * self.cols + col];
            for c in 0..self.cols {
                let current = self.data[p * self.cols + c];
                self.data[p * self.cols + c] = current.div(&pivot)
                    .map_err(|source| MatrixError::Arithmetic { row: p, col: c, step: p, source })?;
            }
            for row in 0..self.rows {
                if row == p { continue; }
                let factor = self.data[row * self.cols + col];
                if factor.num == 0 { continue; }
                for c in 0..self.cols {
                    let product = factor.mul(&self.data[p * self.cols + c])
                        .map_err(|source| MatrixError::Arithmetic { row, col: c, step: p, source })?;
                    self.data[row * self.cols + c] = self.data[row * self.cols + c].sub(&product)
                        .map_err(|source| MatrixError::Arithmetic { row, col: c, step: p, source })?;
                }
            }
            pivots.push(col);
            p += 1;
        }
        Ok((pivots, swaps))
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        let mut matrix = self.clone();
        let (pivots, swaps) = matrix.eliminate(self.cols)?;
        Ok(Reduction { matrix, pivots, swaps })
    }

    pub fn rank(&self) -> Result<usize, MatrixError> {
        Ok(self.rref()?.rank())
    }

    pub fn inverse(&self) -> Result<Self, MatrixError> {
        if self.rows != self.cols {
            return Err(MatrixError::NotSquare { rows: self.rows, cols: self.cols });
        }
        let n = self.rows;
        let width = n.checked_mul(2).ok_or(MatrixError::ShapeOverflow)?;
        let len = n.checked_mul(width).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        for r in 0..n {
            for c in 0..width {
                data.push(if c < n { self.data[r * n + c] } else {
                    Rational { num: if c - n == r { 1 } else { 0 }, den: 1 }
                });
            }
        }
        let mut aug = Self { rows: n, cols: width, data };
        let (pivots, _) = aug.eliminate(n)?;
        if pivots.len() < n {
            return Err(MatrixError::Singular { rank: pivots.len() });
        }
        let out_len = n.checked_mul(n).ok_or(MatrixError::ShapeOverflow)?;
        let mut out = Vec::with_capacity(out_len);
        for r in 0..n {
            out.extend_from_slice(&aug.data[r * width + n..r * width + width]);
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
            data.extend_from_slice(&self.data[r * n..(r + 1) * n]);
            data.extend_from_slice(&rhs.data[r * k..(r + 1) * k]);
        }
        let mut aug = Self { rows: self.rows, cols: width, data };
        let (pivots, _) = aug.eliminate(n).map_err(SolveError::Matrix)?;

        for row in 0..self.rows {
            if aug.data[row * width..row * width + n].iter().all(|v| v.num == 0) {
                if let Some(rhs_col) = (0..k).find(|&c| aug.data[row * width + n + c].num != 0) {
                    return Ok(Solution::None {
                        row,
                        rhs_col,
                        witness: aug.data[row * width..(row + 1) * width].to_vec(),
                    });
                }
            }
        }

        let rank = pivots.len();
        let particular_len = n.checked_mul(k).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut particular_data = vec![Rational { num: 0, den: 1 }; particular_len];
        for (j, &pivot_col) in pivots.iter().enumerate() {
            let row = j;
            for c in 0..k {
                particular_data[pivot_col * k + c] = aug.data[row * width + n + c];
            }
        }
        let particular = Matrix { rows: n, cols: k, data: particular_data };
        if rank == n {
            return Ok(Solution::Unique { values: particular, pivots });
        }

        let free: Vec<usize> = (0..n).filter(|c| !pivots.contains(c)).collect();
        let basis_len = n.checked_mul(free.len()).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut basis_data = vec![Rational { num: 0, den: 1 }; basis_len];
        for (j, &free_col) in free.iter().enumerate() {
            basis_data[free_col * free.len() + j] = Rational { num: 1, den: 1 };
            for (i, &pivot_col) in pivots.iter().enumerate() {
                let coefficient = aug.data[i * width + free_col];
                let neg = coefficient.neg().map_err(|source| {
                    SolveError::Matrix(MatrixError::Arithmetic { row: pivot_col, col: j, step: rank, source })
                })?;
                basis_data[pivot_col * free.len() + j] = neg;
            }
        }
        Ok(Solution::Infinite {
            particular,
            free,
            basis: Matrix { rows: n, cols: pivots.len().checked_sub(rank).map_or_else(|| {
                n - rank
            }, |_| n - rank), data: basis_data },
            pivots,
        })
    }
}