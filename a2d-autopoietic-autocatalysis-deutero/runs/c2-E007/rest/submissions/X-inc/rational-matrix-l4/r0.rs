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
    if num == 0 {
        return Ok(Rational { num: 0, den: 1 });
    }

    let (mut n, mut d) = (num, den);
    if d < 0 {
        n = n.checked_neg().ok_or(RationalError::Overflow)?;
        d = d.checked_neg().ok_or(RationalError::Overflow)?;
    }
    let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
    n /= g;
    d /= g;
    if n < i64::MIN as i128 || n > i64::MAX as i128 || d < 1 || d > i64::MAX as i128 {
        return Err(RationalError::Overflow);
    }
    Ok(Rational {
        num: n as i64,
        den: d as i64,
    })
}

impl Rational {
    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        rational_from_i128(num as i128, den as i128)
    }

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

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
        fn perr(at: usize, kind: ParseKind) -> RationalError {
            RationalError::Parse { at, kind }
        }

        let b = text.as_bytes();
        if b.is_empty() {
            return Err(perr(0, ParseKind::Empty));
        }

        let mut i = 0;
        if b[0] == b'-' {
            i = 1;
        }
        let num_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return Err(perr(i, ParseKind::InvalidByte));
        }

        let num_end = i;
        let mut den_start = None;
        let mut den_end = None;
        if i < b.len() {
            if b[i] != b'/' {
                return Err(perr(i, ParseKind::InvalidByte));
            }
            i += 1;
            let start = i;
            den_start = Some(start);
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(perr(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
            if i < b.len() {
                return Err(perr(i, ParseKind::InvalidByte));
            }
        }

        let neg = b[0] == b'-';
        let digit_start = if neg { 1 } else { 0 };
        if num_end - digit_start >= 2 && b[digit_start] == b'0' {
            return Err(perr(digit_start + 1, ParseKind::NonCanonical));
        }
        if neg && num_end == 2 && b[1] == b'0' {
            return Err(perr(0, ParseKind::NonCanonical));
        }
        if let (Some(s), Some(e)) = (den_start, den_end) {
            if e - s >= 2 && b[s] == b'0' {
                return Err(perr(s + 1, ParseKind::NonCanonical));
            }
        }

        let mut nmag: u128 = 0;
        for &c in &b[digit_start..num_end] {
            nmag = nmag.saturating_mul(10).saturating_add((c - b'0') as u128);
            let limit = if neg { 1u128 << 63 } else { i64::MAX as u128 };
            if nmag > limit {
                return Err(perr(0, ParseKind::OutOfRange));
            }
        }

        let mut dval = 1u128;
        if let (Some(s), Some(e)) = (den_start, den_end) {
            dval = 0;
            for &c in &b[s..e] {
                dval = dval.saturating_mul(10).saturating_add((c - b'0') as u128);
                if dval > i64::MAX as u128 {
                    return Err(perr(s, ParseKind::OutOfRange));
                }
            }
            if dval == 0 {
                return Err(perr(s, ParseKind::ZeroDenominator));
            }
            if dval == 1 {
                return Err(perr(s - 1, ParseKind::NonCanonical));
            }
            if nmag == 0 {
                return Err(perr(s - 1, ParseKind::NonCanonical));
            }
            if gcd_u128(nmag, dval) > 1 {
                return Err(perr(s - 1, ParseKind::NonCanonical));
            }
        }

        let n = if neg {
            if nmag == 1u128 << 63 { i64::MIN } else { -(nmag as i64) }
        } else {
            nmag as i64
        };
        Ok(Rational { num: n, den: dval as i64 })
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
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
                    let product = self.data[r * self.cols + k].mul(&rhs.data[k * rhs.cols + c])
                        .map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: k, source })?;
                    acc = acc.add(&product)
                        .map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: k, source })?;
                }
                data.push(acc);
            }
        }
        Ok(Self { rows: self.rows, cols: rhs.cols, data })
    }

    fn eliminate(&self, candidate_cols: usize) -> Result<(Self, Vec<usize>, Vec<(usize, usize)>), MatrixError> {
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
                for c in 0..m.cols {
                    m.data.swap(p * m.cols + c, q * m.cols + c);
                }
                swaps.push((p, q));
            }
            let pivot = m.data[p * m.cols + col];
            for c in 0..m.cols {
                let v = m.data[p * m.cols + c].div(&pivot)
                    .map_err(|source| MatrixError::Arithmetic { row: p, col: c, step: p, source })?;
                m.data[p * m.cols + c] = v;
            }
            for row in 0..m.rows {
                if row == p { continue; }
                let factor = m.data[row * m.cols + col];
                if factor.num == 0 { continue; }
                for c in 0..m.cols {
                    let product = factor.mul(&m.data[p * m.cols + c])
                        .map_err(|source| MatrixError::Arithmetic { row, col: c, step: p, source })?;
                    let value = m.data[row * m.cols + c].sub(&product)
                        .map_err(|source| MatrixError::Arithmetic { row, col: c, step: p, source })?;
                    m.data[row * m.cols + c] = value;
                }
            }
            pivots.push(col);
            p += 1;
        }
        Ok((m, pivots, swaps))
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        let (matrix, pivots, swaps) = self.eliminate(self.cols)?;
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
            data.extend_from_slice(&self.data[r * n..(r + 1) * n]);
            for c in 0..n {
                data.push(Rational { num: (r == c) as i64, den: 1 });
            }
        }
        let aug = Self { rows: n, cols: width, data };
        let (reduced, pivots, _) = aug.eliminate(n)?;
        if pivots.len() < n {
            return Err(MatrixError::Singular { rank: pivots.len() });
        }
        let mut out = Vec::with_capacity(n * n);
        for r in 0..n {
            out.extend_from_slice(&reduced.data[r * width + n..r * width + width]);
        }
        Ok(Self { rows: n, cols: n, data: out })
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
            data.extend_from_slice(&self.data[r * n..(r + 1) * n]);
            data.extend_from_slice(&rhs.data[r * k..(r + 1) * k]);
        }
        let aug = Self { rows: self.rows, cols: width, data };
        let (reduced, pivots, _) = aug.eliminate(n).map_err(SolveError::Matrix)?;

        for row in 0..self.rows {
            if reduced.data[row * width..row * width + n].iter().all(|x| x.num == 0) {
                if let Some(rhs_col) = (0..k).find(|&c| reduced.data[row * width + n + c].num != 0) {
                    return Ok(Solution::None {
                        row, rhs_col, witness: reduced.data[row * width..(row + 1) * width].to_vec(),
                    });
                }
            }
        }

        let particular_len = n.checked_mul(k).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut particular_data = vec![Rational { num: 0, den: 1 }; particular_len];
        for (j, &pivot_col) in pivots.iter().enumerate() {
            for c in 0..k {
                particular_data[pivot_col * k + c] = reduced.data[j * width + n + c];
            }
        }
        let particular = Self { rows: n, cols: k, data: particular_data };
        if pivots.len() == n {
            return Ok(Solution::Unique { values: particular, pivots });
        }

        let mut free = Vec::new();
        let mut pi = 0;
        for c in 0..n {
            if pi < pivots.len() && pivots[pi] == c {
                pi += 1;
            } else {
                free.push(c);
            }
        }
        let f = free.len();
        let basis_len = n.checked_mul(f).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut basis_data = vec![Rational { num: 0, den: 1 }; basis_len];
        for (j, &free_col) in free.iter().enumerate() {
            basis_data[free_col * f + j] = Rational { num: 1, den: 1 };
            for (i, &pivot_col) in pivots.iter().enumerate() {
                let coefficient = reduced.data[i * width + free_col];
                let value = coefficient.neg().map_err(|source| {
                    SolveError::Matrix(MatrixError::Arithmetic {
                        row: pivot_col, col: j, step: pivots.len(), source,
                    })
                })?;
                basis_data[pivot_col * f + j] = value;
            }
        }
        Ok(Solution::Infinite {
            particular, free, basis: Self { rows: n, cols: f, data: basis_data }, pivots,
        })
    }
}