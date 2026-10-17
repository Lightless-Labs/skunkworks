use std::cmp::Ordering;
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

    if n < i64::MIN as i128
        || n > i64::MAX as i128
        || d < 1
        || d > i64::MAX as i128
    {
        return Err(RationalError::Overflow);
    }

    Ok(Rational {
        num: n as i64,
        den: d as i64,
    })
}

fn rational_from_i128_with_positive_den(num: i128, den: i128) -> Result<Rational, RationalError> {
    if den <= 0 {
        return Err(RationalError::Overflow);
    }
    rational_from_i128(num, den)
}

fn rational_add(a: &Rational, b: &Rational) -> Result<Rational, RationalError> {
    let g = gcd_u128(a.den as u128, b.den as u128) as i128;
    let ad = a.den as i128 / g;
    let bd = b.den as i128 / g;
    let n = (a.num as i128)
        .checked_mul(bd)
        .and_then(|x| (b.num as i128).checked_mul(ad).and_then(|y| x.checked_add(y)))
        .ok_or(RationalError::Overflow)?;
    let d = ad
        .checked_mul(b.den as i128)
        .ok_or(RationalError::Overflow)?;
    rational_from_i128(n, d)
}

fn rational_sub(a: &Rational, b: &Rational) -> Result<Rational, RationalError> {
    let g = gcd_u128(a.den as u128, b.den as u128) as i128;
    let ad = a.den as i128 / g;
    let bd = b.den as i128 / g;
    let n = (a.num as i128)
        .checked_mul(bd)
        .and_then(|x| (b.num as i128).checked_mul(ad).and_then(|y| x.checked_sub(y)))
        .ok_or(RationalError::Overflow)?;
    let d = ad
        .checked_mul(b.den as i128)
        .ok_or(RationalError::Overflow)?;
    rational_from_i128(n, d)
}

fn rational_mul(a: &Rational, b: &Rational) -> Result<Rational, RationalError> {
    let g1 = gcd_u128(a.num.unsigned_abs() as u128, b.den as u128) as i128;
    let g2 = gcd_u128(b.num.unsigned_abs() as u128, a.den as u128) as i128;
    let an = a.num as i128 / g1;
    let bd = b.den as i128 / g1;
    let bn = b.num as i128 / g2;
    let ad = a.den as i128 / g2;
    let n = an.checked_mul(bn).ok_or(RationalError::Overflow)?;
    let d = ad.checked_mul(bd).ok_or(RationalError::Overflow)?;
    rational_from_i128(n, d)
}

fn rational_div(a: &Rational, b: &Rational) -> Result<Rational, RationalError> {
    if b.num == 0 {
        return Err(RationalError::DivideByZero);
    }
    let g1 = gcd_u128(a.num.unsigned_abs() as u128, b.num.unsigned_abs() as u128) as i128;
    let g2 = gcd_u128(a.den as u128, b.den as u128) as i128;
    let an = a.num as i128 / g1;
    let bn = b.num as i128 / g1;
    let ad = a.den as i128 / g2;
    let bd = b.den as i128 / g2;
    let n = an.checked_mul(bd).ok_or(RationalError::Overflow)?;
    let d = ad
        .checked_mul(bn)
        .ok_or(RationalError::Overflow)?;
    rational_from_i128(n, d)
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
        rational_add(self, rhs)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_sub(self, rhs)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_mul(self, rhs)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_div(self, rhs)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        let n = -(self.num as i128);
        rational_from_i128(n, self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(parse_error(0, ParseKind::Empty));
        }

        let mut i = 0;
        let negative = bytes[0] == b'-';
        if negative {
            i = 1;
        }

        let num_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            if i == bytes.len() {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
            return Err(parse_error(i, ParseKind::InvalidByte));
        }
        let num_end = i;

        let mut den_start = None;
        let mut den_end = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
            i += 1;
            den_start = Some(i);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start.unwrap() {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
            if i < bytes.len() {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
        }

        let num_digits = &bytes[num_start..num_end];
        if num_digits.len() >= 2 && num_digits[0] == b'0' {
            return Err(parse_error(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_digits == b"0" {
            return Err(parse_error(0, ParseKind::NonCanonical));
        }

        if let (Some(ds), Some(de)) = (den_start, den_end) {
            let den_digits = &bytes[ds..de];
            if den_digits.len() >= 2 && den_digits[0] == b'0' {
                return Err(parse_error(ds + 1, ParseKind::NonCanonical));
            }
        }

        let num_mag = parse_magnitude(num_digits, if negative { (1u128 << 63) } else { i64::MAX as u128 })
            .ok_or_else(|| parse_error(0, ParseKind::OutOfRange))?;

        let num = if negative {
            if num_mag == (1u128 << 63) {
                i64::MIN
            } else {
                -(num_mag as i64)
            }
        } else {
            num_mag as i64
        };

        if let (Some(ds), Some(de)) = (den_start, den_end) {
            let den_digits = &bytes[ds..de];
            let den_mag = parse_magnitude(den_digits, i64::MAX as u128)
                .ok_or_else(|| parse_error(ds, ParseKind::OutOfRange))?;
            if den_digits == b"0" {
                return Err(parse_error(ds, ParseKind::ZeroDenominator));
            }
            let slash = ds - 1;
            if den_mag == 1 {
                return Err(parse_error(slash, ParseKind::NonCanonical));
            }
            if num == 0 {
                return Err(parse_error(slash, ParseKind::NonCanonical));
            }
            if gcd_u128(num_mag, den_mag) > 1 {
                return Err(parse_error(slash, ParseKind::NonCanonical));
            }
            Ok(Rational {
                num,
                den: den_mag as i64,
            })
        } else {
            Ok(Rational { num, den: 1 })
        }
    }
}

fn parse_error(at: usize, kind: ParseKind) -> RationalError {
    RationalError::Parse { at, kind }
}

fn parse_magnitude(bytes: &[u8], limit: u128) -> Option<u128> {
    let mut value = 0u128;
    for &b in bytes {
        let digit = (b - b'0') as u128;
        value = value.checked_mul(10)?.checked_add(digit)?;
        if value > limit {
            return None;
        }
    }
    Some(value)
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
    DataLength {
        expected: usize,
        actual: usize,
    },
    Index {
        row: usize,
        col: usize,
    },
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
    NotSquare {
        rows: usize,
        cols: usize,
    },
    Singular {
        rank: usize,
    },
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
        let index = row
            .checked_mul(self.cols)
            .and_then(|x| x.checked_add(col))
            .ok_or(MatrixError::Index { row, col })?;
        self.data
            .get(index)
            .copied()
            .ok_or(MatrixError::Index { row, col })
    }

    pub fn transpose(&self) -> Self {
        let len = self.data.len();
        let mut data = vec![Rational { num: 0, den: 1 }; len];
        if self.rows != 0 && self.cols != 0 {
            for r in 0..self.rows {
                for c in 0..self.cols {
                    data[c * self.rows + r] = self.data[r * self.cols + c];
                }
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
        let mut data = Vec::with_capacity(out_len);
        let zero = Rational { num: 0, den: 1 };

        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = zero;
                for k in 0..self.cols {
                    let a = self.data[r * self.cols + k];
                    let b = rhs.data[k * rhs.cols + c];
                    let term = a.mul(&b).map_err(|source| MatrixError::Arithmetic {
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
                data.push(acc);
            }
        }
        Ok(Self {
            rows: self.rows,
            cols: rhs.cols,
            data,
        })
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        eliminate(self, self.cols)
    }

    pub fn rank(&self) -> Result<usize, MatrixError> {
        Ok(self.rref()?.rank())
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
        let zero = Rational { num: 0, den: 1 };
        let one = Rational { num: 1, den: 1 };

        for r in 0..n {
            for c in 0..n {
                data.push(self.data[r * n + c]);
            }
            for c in 0..n {
                data.push(if r == c { one } else { zero });
            }
        }

        let augmented = Matrix {
            rows: n,
            cols: width,
            data,
        };
        let reduction = eliminate(&augmented, n)?;
        if reduction.pivots.len() < n {
            return Err(MatrixError::Singular {
                rank: reduction.pivots.len(),
            });
        }

        let mut inverse_data = Vec::with_capacity(n.checked_mul(n).ok_or(MatrixError::ShapeOverflow)?);
        for r in 0..n {
            for c in 0..n {
                inverse_data.push(reduction.matrix.data[r * width + n + c]);
            }
        }
        Ok(Self {
            rows: n,
            cols: n,
            data: inverse_data,
        })
    }
}

fn eliminate(matrix: &Matrix, candidate_cols: usize) -> Result<Reduction, MatrixError> {
    let mut work = matrix.clone();
    let mut pivots = Vec::new();
    let mut swaps = Vec::new();
    let mut p = 0usize;

    for col in 0..candidate_cols {
        if p == work.rows {
            break;
        }

        let mut chosen: Option<usize> = None;
        for row in p..work.rows {
            let value = work.data[row * work.cols + col];
            if value.num == 0 {
                continue;
            }
            chosen = match chosen {
                None => Some(row),
                Some(current) => {
                    let a = work.data[current * work.cols + col];
                    let ordering = (value.den, value.num.unsigned_abs(), row)
                        .cmp(&(a.den, a.num.unsigned_abs(), current));
                    if ordering == Ordering::Less {
                        Some(row)
                    } else {
                        Some(current)
                    }
                }
            };
        }

        let q = match chosen {
            Some(q) => q,
            None => continue,
        };

        if q != p {
            for c in 0..work.cols {
                work.data.swap(p * work.cols + c, q * work.cols + c);
            }
            swaps.push((p, q));
        }

        let pivot = work.data[p * work.cols + col];
        for c in 0..work.cols {
            let current = work.data[p * work.cols + c];
            work.data[p * work.cols + c] = current.div(&pivot).map_err(|source| {
                MatrixError::Arithmetic {
                    row: p,
                    col: c,
                    step: p,
                    source,
                }
            })?;
        }

        for row in 0..work.rows {
            if row == p {
                continue;
            }
            let factor = work.data[row * work.cols + col];
            if factor.num == 0 {
                continue;
            }
            for c in 0..work.cols {
                let product = factor.mul(&work.data[p * work.cols + c]).map_err(|source| {
                    MatrixError::Arithmetic {
                        row,
                        col: c,
                        step: p,
                        source,
                    }
                })?;
                let current = work.data[row * work.cols + c];
                work.data[row * work.cols + c] =
                    current.sub(&product).map_err(|source| MatrixError::Arithmetic {
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
        matrix: work,
        pivots,
        swaps,
    })
}