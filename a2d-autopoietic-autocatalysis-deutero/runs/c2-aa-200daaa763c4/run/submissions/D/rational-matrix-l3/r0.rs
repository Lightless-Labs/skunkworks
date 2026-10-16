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

fn rational_from_parts(num: i128, den: i128) -> Result<Rational, RationalError> {
    if den == 0 {
        return Err(RationalError::ZeroDenominator);
    }
    if num == 0 {
        return Ok(Rational { num: 0, den: 1 });
    }

    let (mut n, mut d) = (num, den);
    if d < 0 {
        n = -n;
        d = -d;
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
        rational_from_parts(num as i128, den as i128)
    }

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_parts(
            self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_parts(
            self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_parts(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        rational_from_parts(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        if self.num == i64::MIN {
            return Err(RationalError::Overflow);
        }
        Ok(Rational {
            num: -self.num,
            den: self.den,
        })
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
            den_start = Some(i);
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start.unwrap() {
                return Err(perr(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
            if i < b.len() {
                return Err(perr(i, ParseKind::InvalidByte));
            }
        }

        let negative = b[0] == b'-';
        let num_digits_start = if negative { 1 } else { 0 };
        let num_digits = num_end - num_digits_start;

        if num_digits >= 2 && b[num_digits_start] == b'0' {
            return Err(perr(num_digits_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_digits == 1 && b[1] == b'0' {
            return Err(perr(0, ParseKind::NonCanonical));
        }

        if let (Some(ds), Some(de)) = (den_start, den_end) {
            if de - ds >= 2 && b[ds] == b'0' {
                return Err(perr(ds + 1, ParseKind::NonCanonical));
            }
        }

        let mut nmag: u128 = 0;
        let limit = if negative { 1u128 << 63 } else { i64::MAX as u128 };
        for &c in &b[num_digits_start..num_end] {
            nmag = nmag.saturating_mul(10).saturating_add((c - b'0') as u128);
            if nmag > limit {
                return Err(perr(0, ParseKind::OutOfRange));
            }
        }

        let mut dmag: u128 = 1;
        if let (Some(ds), Some(de)) = (den_start, den_end) {
            dmag = 0;
            for &c in &b[ds..de] {
                dmag = dmag.saturating_mul(10).saturating_add((c - b'0') as u128);
                if dmag > i64::MAX as u128 {
                    return Err(perr(ds, ParseKind::OutOfRange));
                }
            }
            if dmag == 0 {
                return Err(perr(ds, ParseKind::ZeroDenominator));
            }
        }

        if let (Some(ds), Some(_)) = (den_start, den_end) {
            let slash = ds - 1;
            if dmag == 1 {
                return Err(perr(slash, ParseKind::NonCanonical));
            }
            if nmag == 0 {
                return Err(perr(slash, ParseKind::NonCanonical));
            }
            if gcd_u128(nmag, dmag) > 1 {
                return Err(perr(slash, ParseKind::NonCanonical));
            }
        }

        let signed_num = if negative {
            -(nmag as i128)
        } else {
            nmag as i128
        };
        rational_from_parts(signed_num, dmag as i128)
            .map_err(|_| perr(0, ParseKind::OutOfRange))
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
    DimensionMismatch {
        left: (usize, usize),
        right: (usize, usize),
    },
    Arithmetic {
        row: usize,
        col: usize,
        step: usize,
        source: RationalError,
    },
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
    pub fn rank(&self) -> usize {
        self.pivots.len()
    }
}

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<Rational>) -> Result<Self, MatrixError> {
        let expected = rows.checked_mul(cols).ok_or(MatrixError::ShapeOverflow)?;
        if data.len() != expected {
            return Err(MatrixError::DataLength {
                expected,
                actual: data.len(),
            });
        }
        Ok(Self { rows, cols, data })
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn data(&self) -> &[Rational] {
        &self.data
    }

    pub fn get(&self, row: usize, col: usize) -> Result<Rational, MatrixError> {
        if row >= self.rows || col >= self.cols {
            return Err(MatrixError::Index { row, col });
        }
        let idx = row
            .checked_mul(self.cols)
            .and_then(|x| x.checked_add(col))
            .ok_or(MatrixError::Index { row, col })?;
        self.data
            .get(idx)
            .copied()
            .ok_or(MatrixError::Index { row, col })
    }

    pub fn transpose(&self) -> Self {
        let mut data = Vec::with_capacity(self.data.len());
        for r in 0..self.cols {
            for c in 0..self.rows {
                data.push(self.data[c * self.cols + r]);
            }
        }
        Self {
            rows: self.cols,
            cols: self.rows,
            data,
        }
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, MatrixError> {
        if self.cols != rhs.rows {
            return Err(MatrixError::DimensionMismatch {
                left: (self.rows, self.cols),
                right: (rhs.rows, rhs.cols),
            });
        }
        let out_len = self
            .rows
            .checked_mul(rhs.cols)
            .ok_or(MatrixError::ShapeOverflow)?;
        let mut out = Vec::with_capacity(out_len);
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = Rational { num: 0, den: 1 };
                for k in 0..self.cols {
                    let left = self.data[r * self.cols + k];
                    let right = rhs.data[k * rhs.cols + c];
                    let term = left.mul(&right).map_err(|source| MatrixError::Arithmetic {
                        row: r,
                        col: c,
                        step: k,
                        source,
                    })?;
                    acc = acc.add(&term).map_err(|source| MatrixError::Arithmetic {
                        row: r,
                        col: c,
                        step: k,
                        source,
                    })?;
                }
                out.push(acc);
            }
        }
        Ok(Self {
            rows: self.rows,
            cols: rhs.cols,
            data: out,
        })
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        eliminate(self.clone(), self.cols)
    }

    pub fn rank(&self) -> Result<usize, MatrixError> {
        self.rref().map(|r| r.rank())
    }

    pub fn inverse(&self) -> Result<Self, MatrixError> {
        if self.rows != self.cols {
            return Err(MatrixError::NotSquare {
                rows: self.rows,
                cols: self.cols,
            });
        }
        let n = self.rows;
        let width = n.checked_mul(2).ok_or(MatrixError::ShapeOverflow)?;
        let len = n.checked_mul(width).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        for r in 0..n {
            data.extend_from_slice(&self.data[r * n..(r + 1) * n]);
            for c in 0..n {
                data.push(Rational {
                    num: if r == c { 1 } else { 0 },
                    den: 1,
                });
            }
        }
        let red = eliminate(
            Matrix {
                rows: n,
                cols: width,
                data,
            },
            n,
        )?;
        if red.pivots.len() < n {
            return Err(MatrixError::Singular {
                rank: red.pivots.len(),
            });
        }
        let mut result = Vec::with_capacity(n * n);
        for r in 0..n {
            result.extend_from_slice(&red.matrix.data[r * width + n..r * width + width]);
        }
        Ok(Matrix {
            rows: n,
            cols: n,
            data: result,
        })
    }
}

fn eliminate(mut matrix: Matrix, pivot_limit: usize) -> Result<Reduction, MatrixError> {
    let rows = matrix.rows;
    let cols = matrix.cols;
    let mut p = 0usize;
    let mut pivots = Vec::new();
    let mut swaps = Vec::new();

    for col in 0..pivot_limit {
        if p == rows {
            break;
        }
        let mut chosen: Option<usize> = None;
        for row in p..rows {
            let value = matrix.data[row * cols + col];
            if value.num == 0 {
                continue;
            }
            let key = (value.den, value.num.unsigned_abs(), row);
            if chosen
                .map(|old| {
                    let v = matrix.data[old * cols + col];
                    key < (v.den, v.num.unsigned_abs(), old)
                })
                .unwrap_or(true)
            {
                chosen = Some(row);
            }
        }
        let Some(q) = chosen else {
            continue;
        };

        if q != p {
            for c in 0..cols {
                matrix.data.swap(p * cols + c, q * cols + c);
            }
            swaps.push((p, q));
        }

        let pivot = matrix.data[p * cols + col];
        for c in 0..cols {
            let original = matrix.data[p * cols + c];
            matrix.data[p * cols + c] =
                original
                    .div(&pivot)
                    .map_err(|source| MatrixError::Arithmetic {
                        row: p,
                        col: c,
                        step: p,
                        source,
                    })?;
        }

        for row in 0..rows {
            if row == p {
                continue;
            }
            let factor = matrix.data[row * cols + col];
            if factor.num == 0 {
                continue;
            }
            for c in 0..cols {
                let product = factor
                    .mul(&matrix.data[p * cols + c])
                    .map_err(|source| MatrixError::Arithmetic {
                        row,
                        col: c,
                        step: p,
                        source,
                    })?;
                matrix.data[row * cols + c] =
                    matrix.data[row * cols + c]
                        .sub(&product)
                        .map_err(|source| MatrixError::Arithmetic {
                            row,
                            col: c,
                            step: p,
                            source,
                        })?;
            }
        }

        pivots.push(col);
        p += 1;
    }

    Ok(Reduction {
        matrix,
        pivots,
        swaps,
    })
}