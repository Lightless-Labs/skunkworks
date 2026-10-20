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
        n = -n;
        d = -d;
    }
    if n == 0 {
        return Ok(Rational { num: 0, den: 1 });
    }
    let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
    n /= g;
    d /= g;
    if n < i64::MIN as i128 || n > i64::MAX as i128 || d < 1 || d > i64::MAX as i128 {
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
        rational_from_i128(
            self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_i128(
            self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_i128(self.num as i128 * rhs.num as i128, self.den as i128 * rhs.den as i128)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 { return Err(RationalError::DivideByZero); }
        rational_from_i128(self.num as i128 * rhs.den as i128, self.den as i128 * rhs.num as i128)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        rational_from_i128(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let b = text.as_bytes();
        let err = |at, kind| RationalError::Parse { at, kind };
        if b.is_empty() { return Err(err(0, ParseKind::Empty)); }

        let mut i = 0;
        if b[0] == b'-' { i = 1; }
        let num_start = i;
        while i < b.len() && b[i].is_ascii_digit() { i += 1; }
        if i == num_start {
            return Err(err(i, ParseKind::InvalidByte));
        }
        let num_end = i;
        let slash = if i < b.len() && b[i] == b'/' {
            let s = i;
            i += 1;
            let den_start = i;
            while i < b.len() && b[i].is_ascii_digit() { i += 1; }
            if i == den_start { return Err(err(i, ParseKind::InvalidByte)); }
            Some((s, den_start, i))
        } else {
            None
        };
        if i < b.len() { return Err(err(i, ParseKind::InvalidByte)); }

        let neg = b[0] == b'-';
        let digits_start = if neg { 1 } else { 0 };
        let digits_len = num_end - digits_start;
        if digits_len >= 2 && b[digits_start] == b'0' {
            return Err(err(digits_start + 1, ParseKind::NonCanonical));
        }
        if neg && digits_len == 1 && b[digits_start] == b'0' {
            return Err(err(0, ParseKind::NonCanonical));
        }
        if let Some((_, ds, de)) = slash {
            if de - ds >= 2 && b[ds] == b'0' {
                return Err(err(ds + 1, ParseKind::NonCanonical));
            }
        }

        let parse_u128 = |start: usize, end: usize| -> Option<u128> {
            let mut v = 0u128;
            for &c in &b[start..end] {
                v = v.checked_mul(10)?.checked_add((c - b'0') as u128)?;
            }
            Some(v)
        };
        let nm = parse_u128(digits_start, num_end);
        let nlimit = if neg { 1u128 << 63 } else { i64::MAX as u128 };
        if nm.is_none() || nm.unwrap() > nlimit {
            return Err(err(0, ParseKind::OutOfRange));
        }
        let nmag = nm.unwrap();
        let n = if neg {
            if nmag == 1u128 << 63 { i64::MIN } else { -(nmag as i64) }
        } else { nmag as i64 };

        let (d, slash_at) = if let Some((s, ds, de)) = slash {
            let dm = parse_u128(ds, de);
            if dm.is_none() || dm.unwrap() > i64::MAX as u128 {
                return Err(err(ds, ParseKind::OutOfRange));
            }
            (dm.unwrap() as i64, Some(s))
        } else { (1, None) };

        if let Some((_, ds, de)) = slash {
            if de - ds == 1 && b[ds] == b'0' {
                return Err(err(ds, ParseKind::ZeroDenominator));
            }
            if d == 1 {
                return Err(err(slash_at.unwrap(), ParseKind::NonCanonical));
            }
            if n == 0 {
                return Err(err(slash_at.unwrap(), ParseKind::NonCanonical));
            }
            if gcd_u128(n.unsigned_abs() as u128, d as u128) > 1 {
                return Err(err(slash_at.unwrap(), ParseKind::NonCanonical));
            }
        }
        Ok(Rational { num: n, den: d })
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 { write!(f, "{}", self.num) }
        else { write!(f, "{}/{}", self.num, self.den) }
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

fn arithmetic(row: usize, col: usize, step: usize, source: RationalError) -> MatrixError {
    MatrixError::Arithmetic { row, col, step, source }
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
        let mut out = Vec::with_capacity(len);
        let zero = Rational { num: 0, den: 1 };
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = zero;
                for k in 0..self.cols {
                    let a = self.data[r * self.cols + k];
                    let b = rhs.data[k * rhs.cols + c];
                    let product = a.mul(&b).map_err(|e| arithmetic(r, c, k, e))?;
                    acc = acc.add(&product).map_err(|e| arithmetic(r, c, k, e))?;
                }
                out.push(acc);
            }
        }
        Ok(Self { rows: self.rows, cols: rhs.cols, data: out })
    }

    fn eliminate(&mut self, candidates: usize) -> Result<(Vec<usize>, Vec<(usize, usize)>), MatrixError> {
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;
        for col in 0..candidates {
            if p == self.rows { break; }
            let mut chosen: Option<usize> = None;
            for r in p..self.rows {
                let v = self.data[r * self.cols + col];
                if v.num == 0 { continue; }
                let key = (v.den, v.num.unsigned_abs(), r);
                if chosen.map_or(true, |q| {
                    let w = self.data[q * self.cols + col];
                    key < (w.den, w.num.unsigned_abs(), q)
                }) {
                    chosen = Some(r);
                }
            }
            let Some(q) = chosen else { continue };
            if q != p {
                for c in 0..self.cols {
                    self.data.swap(p * self.cols + c, q * self.cols + c);
                }
                swaps.push((p, q));
            }
            let pivot = self.data[p * self.cols + col];
            for c in 0..self.cols {
                let value = self.data[p * self.cols + c].div(&pivot)
                    .map_err(|e| arithmetic(p, c, p, e))?;
                self.data[p * self.cols + c] = value;
            }
            for r in 0..self.rows {
                if r == p { continue; }
                let factor = self.data[r * self.cols + col];
                if factor.num == 0 { continue; }
                for c in 0..self.cols {
                    let product = factor.mul(&self.data[p * self.cols + c])
                        .map_err(|e| arithmetic(r, c, p, e))?;
                    let value = self.data[r * self.cols + c].sub(&product)
                        .map_err(|e| arithmetic(r, c, p, e))?;
                    self.data[r * self.cols + c] = value;
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
        let zero = Rational { num: 0, den: 1 };
        let one = Rational { num: 1, den: 1 };
        for r in 0..n {
            for c in 0..n { data.push(self.data[r * n + c]); }
            for c in 0..n { data.push(if r == c { one } else { zero }); }
        }
        let mut aug = Self { rows: n, cols: width, data };
        let (pivots, _) = aug.eliminate(n)?;
        if pivots.len() < n { return Err(MatrixError::Singular { rank: pivots.len() }); }
        let mut out = Vec::with_capacity(n * n);
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
            data.extend_from_slice(&self.data[r * n..r * n + n]);
            data.extend_from_slice(&rhs.data[r * k..r * k + k]);
        }
        let mut aug = Self { rows: self.rows, cols: width, data };
        let (pivots, _) = aug.eliminate(n).map_err(SolveError::Matrix)?;

        for r in 0..aug.rows {
            let coeff_zero = (0..n).all(|c| aug.data[r * width + c].num == 0);
            if coeff_zero {
                for j in 0..k {
                    if aug.data[r * width + n + j].num != 0 {
                        return Ok(Solution::None {
                            row: r,
                            rhs_col: j,
                            witness: aug.data[r * width..(r + 1) * width].to_vec(),
                        });
                    }
                }
            }
        }

        let particular_len = n.checked_mul(k).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut particular_data = vec![Rational { num: 0, den: 1 }; particular_len];
        for (i, &pivot_col) in pivots.iter().enumerate() {
            for j in 0..k {
                particular_data[pivot_col * k + j] = aug.data[i * width + n + j];
            }
        }
        let particular = Self { rows: n, cols: k, data: particular_data };
        if pivots.len() == n {
            return Ok(Solution::Unique { values: particular, pivots });
        }

        let mut free = Vec::new();
        let mut pi = 0usize;
        for c in 0..n {
            if pi < pivots.len() && pivots[pi] == c { pi += 1; }
            else { free.push(c); }
        }
        let f = free.len();
        let basis_len = n.checked_mul(f).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut basis_data = vec![Rational { num: 0, den: 1 }; basis_len];
        let one = Rational { num: 1, den: 1 };
        for (j, &free_col) in free.iter().enumerate() {
            basis_data[free_col * f + j] = one;
            for (i, &pivot_col) in pivots.iter().enumerate() {
                let coefficient = aug.data[i * width + free_col].neg().map_err(|source| {
                    SolveError::Matrix(MatrixError::Arithmetic {
                        row: pivot_col, col: j, step: pivots.len(), source,
                    })
                })?;
                basis_data[pivot_col * f + j] = coefficient;
            }
        }
        let basis = Self { rows: n, cols: f, data: basis_data };
        Ok(Solution::Infinite { particular, free, basis, pivots })
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