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
        n = -n;
        d = -d;
    }
    let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
    n /= g;
    d /= g;

    if n < i64::MIN as i128 || n > i64::MAX as i128 || d < 1 || d > i64::MAX as i128 {
        return Err(RationalError::Overflow);
    }
    Ok(Rational { num: n as i64, den: d as i64 })
}

fn rational_from_i128_pair(num: i128, den: i128) -> Result<Rational, RationalError> {
    rational_from_i128(num, den)
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
        let a = self.num as i128 * rhs.den as i128;
        let b = rhs.num as i128 * self.den as i128;
        rational_from_i128_pair(a + b, self.den as i128 * rhs.den as i128)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let a = self.num as i128 * rhs.den as i128;
        let b = rhs.num as i128 * self.den as i128;
        rational_from_i128_pair(a - b, self.den as i128 * rhs.den as i128)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_i128_pair(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        rational_from_i128_pair(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        rational_from_i128(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(RationalError::Parse { at: 0, kind: ParseKind::Empty });
        }

        let mut i = 0;
        let negative = bytes[0] == b'-';
        if negative {
            i += 1;
        }
        let num_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
        }
        let num_end = i;
        let mut den_start = None;
        let mut den_end = None;

        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
            }
            i += 1;
            let start = i;
            den_start = Some(start);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
            }
            den_end = Some(i);
            if i < bytes.len() {
                return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
            }
        }

        let digit_count = num_end - num_start;
        if digit_count >= 2 && bytes[num_start] == b'0' {
            return Err(RationalError::Parse {
                at: num_start + 1,
                kind: ParseKind::NonCanonical,
            });
        }
        if negative && digit_count == 1 && bytes[num_start] == b'0' {
            return Err(RationalError::Parse { at: 0, kind: ParseKind::NonCanonical });
        }
        if let (Some(ds), Some(de)) = (den_start, den_end) {
            if de - ds >= 2 && bytes[ds] == b'0' {
                return Err(RationalError::Parse { at: ds + 1, kind: ParseKind::NonCanonical });
            }
        }

        let num_limit: u128 = if negative { 1u128 << 63 } else { i64::MAX as u128 };
        let mut num_mag = 0u128;
        for &b in &bytes[num_start..num_end] {
            num_mag = num_mag.saturating_mul(10).saturating_add((b - b'0') as u128);
            if num_mag > num_limit {
                return Err(RationalError::Parse { at: 0, kind: ParseKind::OutOfRange });
            }
        }

        let mut den_mag = 1u128;
        if let (Some(ds), Some(de)) = (den_start, den_end) {
            den_mag = 0;
            for &b in &bytes[ds..de] {
                den_mag = den_mag.saturating_mul(10).saturating_add((b - b'0') as u128);
                if den_mag > i64::MAX as u128 {
                    return Err(RationalError::Parse { at: ds, kind: ParseKind::OutOfRange });
                }
            }
            if den_mag == 0 {
                return Err(RationalError::Parse { at: ds, kind: ParseKind::ZeroDenominator });
            }
            let slash = ds - 1;
            if den_mag == 1 {
                return Err(RationalError::Parse { at: slash, kind: ParseKind::NonCanonical });
            }
            if num_mag == 0 {
                return Err(RationalError::Parse { at: slash, kind: ParseKind::NonCanonical });
            }
            if gcd_u128(num_mag, den_mag) > 1 {
                return Err(RationalError::Parse { at: slash, kind: ParseKind::NonCanonical });
            }
        }

        let num = if negative {
            if num_mag == 1u128 << 63 { i64::MIN } else { -(num_mag as i64) }
        } else {
            num_mag as i64
        };
        Ok(Rational { num, den: den_mag as i64 })
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
    pub fn rank(&self) -> usize {
        self.pivots.len()
    }
}

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<Rational>) -> Result<Self, MatrixError> {
        let expected = rows.checked_mul(cols).ok_or(MatrixError::ShapeOverflow)?;
        if data.len() != expected {
            return Err(MatrixError::DataLength { expected, actual: data.len() });
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
        let index = row.checked_mul(self.cols)
            .and_then(|x| x.checked_add(col))
            .ok_or(MatrixError::Index { row, col })?;
        self.data.get(index).copied().ok_or(MatrixError::Index { row, col })
    }

    pub fn transpose(&self) -> Self {
        let mut data = Vec::with_capacity(self.data.len());
        if self.data.is_empty() {
            return Self { rows: self.cols, cols: self.rows, data };
        }
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
                left: (self.rows, self.cols),
                right: (rhs.rows, rhs.cols),
            });
        }
        let len = self.rows.checked_mul(rhs.cols).ok_or(MatrixError::ShapeOverflow)?;
        if len == 0 {
            return Ok(Self { rows: self.rows, cols: rhs.cols, data: Vec::new() });
        }
        let mut data = Vec::with_capacity(len);
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut sum = Rational { num: 0, den: 1 };
                for k in 0..self.cols {
                    let a = self.data[r * self.cols + k];
                    let b = rhs.data[k * rhs.cols + c];
                    let term = a.mul(&b).map_err(|source| MatrixError::Arithmetic {
                        row: r, col: c, step: k, source,
                    })?;
                    sum = sum.add(&term).map_err(|source| MatrixError::Arithmetic {
                        row: r, col: c, step: k, source,
                    })?;
                }
                data.push(sum);
            }
        }
        Ok(Self { rows: self.rows, cols: rhs.cols, data })
    }

    fn eliminate(&self, pivot_cols: usize) -> Result<Reduction, MatrixError> {
        let mut matrix = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;

        for col in 0..pivot_cols {
            if p == matrix.rows {
                break;
            }
            let mut best: Option<usize> = None;
            for row in p..matrix.rows {
                let value = matrix.data[row * matrix.cols + col];
                if value.num == 0 {
                    continue;
                }
                best = match best {
                    None => Some(row),
                    Some(old) => {
                        let prior = matrix.data[old * matrix.cols + col];
                        let key = (value.den, value.num.unsigned_abs(), row);
                        let prior_key = (prior.den, prior.num.unsigned_abs(), old);
                        if key < prior_key { Some(row) } else { Some(old) }
                    }
                };
            }
            let Some(q) = best else { continue };
            if q != p {
                for c in 0..matrix.cols {
                    matrix.data.swap(p * matrix.cols + c, q * matrix.cols + c);
                }
                swaps.push((p, q));
            }

            let pivot = matrix.data[p * matrix.cols + col];
            for c in 0..matrix.cols {
                let index = p * matrix.cols + c;
                let original = matrix.data[index];
                matrix.data[index] = original.div(&pivot).map_err(|source| MatrixError::Arithmetic {
                    row: p, col: c, step: p, source,
                })?;
            }

            for row in 0..matrix.rows {
                if row == p {
                    continue;
                }
                let factor = matrix.data[row * matrix.cols + col];
                if factor.num == 0 {
                    continue;
                }
                for c in 0..matrix.cols {
                    let row_index = row * matrix.cols + c;
                    let pivot_entry = matrix.data[p * matrix.cols + c];
                    let product = factor.mul(&pivot_entry).map_err(|source| MatrixError::Arithmetic {
                        row, col: c, step: p, source,
                    })?;
                    matrix.data[row_index] = matrix.data[row_index].sub(&product).map_err(|source| {
                        MatrixError::Arithmetic { row, col: c, step: p, source }
                    })?;
                }
            }
            pivots.push(col);
            p += 1;
        }

        Ok(Reduction { matrix, pivots, swaps })
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        self.eliminate(self.cols)
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
                if c < n {
                    data.push(self.data[r * n + c]);
                } else {
                    data.push(if c - n == r {
                        Rational { num: 1, den: 1 }
                    } else {
                        Rational { num: 0, den: 1 }
                    });
                }
            }
        }
        let augmented = Matrix { rows: n, cols: width, data };
        let reduction = augmented.eliminate(n)?;
        if reduction.pivots.len() < n {
            return Err(MatrixError::Singular { rank: reduction.pivots.len() });
        }
        let mut inverse_data = Vec::with_capacity(n * n);
        for r in 0..n {
            for c in 0..n {
                inverse_data.push(reduction.matrix.data[r * width + n + c]);
            }
        }
        Ok(Self { rows: n, cols: n, data: inverse_data })
    }
}