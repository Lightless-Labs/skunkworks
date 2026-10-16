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

fn from_i128_fraction(num: i128, den: i128) -> Result<Rational, RationalError> {
    if den == 0 {
        return Err(RationalError::ZeroDenominator);
    }
    let mut n = num;
    let mut d = den;
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
        let mut n = num as i128;
        let mut d = den as i128;
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
        Ok(Self {
            num: n as i64,
            den: d as i64,
        })
    }

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g = gcd_u128(self.den as u128, rhs.den as u128) as i128;
        let a = rhs.den as i128 / g;
        let b = self.den as i128 / g;
        let n = (self.num as i128)
            .checked_mul(a)
            .and_then(|x| x.checked_add((rhs.num as i128).checked_mul(b)?))
            .ok_or(RationalError::Overflow)?;
        let d = (self.den as i128)
            .checked_mul(a)
            .ok_or(RationalError::Overflow)?;
        from_i128_fraction(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g = gcd_u128(self.den as u128, rhs.den as u128) as i128;
        let a = rhs.den as i128 / g;
        let b = self.den as i128 / g;
        let n = (self.num as i128)
            .checked_mul(a)
            .and_then(|x| x.checked_sub((rhs.num as i128).checked_mul(b)?))
            .ok_or(RationalError::Overflow)?;
        let d = (self.den as i128)
            .checked_mul(a)
            .ok_or(RationalError::Overflow)?;
        from_i128_fraction(n, d)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g1 = gcd_u128(self.num.unsigned_abs() as u128, rhs.den as u128);
        let g2 = gcd_u128(rhs.num.unsigned_abs() as u128, self.den as u128);
        let n = (self.num as i128 / g1 as i128)
            .checked_mul(rhs.num as i128 / g2 as i128)
            .ok_or(RationalError::Overflow)?;
        let d = (self.den as i128 / g2 as i128)
            .checked_mul(rhs.den as i128 / g1 as i128)
            .ok_or(RationalError::Overflow)?;
        from_i128_fraction(n, d)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        let g1 = gcd_u128(self.num.unsigned_abs() as u128, rhs.num.unsigned_abs() as u128);
        let g2 = gcd_u128(self.den as u128, rhs.den as u128);
        let mut n = (self.num as i128 / g1 as i128)
            .checked_mul(rhs.den as i128 / g2 as i128)
            .ok_or(RationalError::Overflow)?;
        let mut d = (self.den as i128 / g2 as i128)
            .checked_mul(rhs.num as i128 / g1 as i128)
            .ok_or(RationalError::Overflow)?;
        if d < 0 {
            n = n.checked_neg().ok_or(RationalError::Overflow)?;
            d = d.checked_neg().ok_or(RationalError::Overflow)?;
        }
        from_i128_fraction(n, d)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        let n = (self.num as i128).checked_neg().ok_or(RationalError::Overflow)?;
        from_i128_fraction(n, self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        let parse_err = |at, kind| RationalError::Parse { at, kind };
        if bytes.is_empty() {
            return Err(parse_err(0, ParseKind::Empty));
        }

        let mut i = 0;
        if bytes[0] == b'-' {
            i = 1;
        }
        let num_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return Err(parse_err(i, ParseKind::InvalidByte));
        }
        let num_end = i;
        let mut den_start = None;
        let mut den_end = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            i += 1;
            den_start = Some(i);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start.unwrap() {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
            if i < bytes.len() {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
        }

        let negative = bytes[0] == b'-';
        let num_digits = &bytes[num_start..num_end];
        if num_digits.len() >= 2 && num_digits[0] == b'0' {
            return Err(parse_err(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_digits == b"0" {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }
        if let (Some(ds), Some(de)) = (den_start, den_end) {
            let digits = &bytes[ds..de];
            if digits.len() >= 2 && digits[0] == b'0' {
                return Err(parse_err(ds + 1, ParseKind::NonCanonical));
            }
        }

        fn parse_magnitude(digits: &[u8], limit: u128) -> Option<u128> {
            let mut value = 0u128;
            for &b in digits {
                value = value.checked_mul(10)?.checked_add((b - b'0') as u128)?;
                if value > limit {
                    return None;
                }
            }
            Some(value)
        }

        let nmag = parse_magnitude(
            num_digits,
            if negative {
                1u128 << 63
            } else {
                i64::MAX as u128
            },
        )
        .ok_or_else(|| parse_err(0, ParseKind::OutOfRange))?;
        let n = if negative {
            if nmag == (1u128 << 63) {
                i64::MIN
            } else {
                -(nmag as i64)
            }
        } else {
            nmag as i64
        };

        let den = if let (Some(ds), Some(de)) = (den_start, den_end) {
            let digits = &bytes[ds..de];
            let mag = parse_magnitude(digits, i64::MAX as u128)
                .ok_or_else(|| parse_err(ds, ParseKind::OutOfRange))?;
            if mag == 0 {
                return Err(parse_err(ds, ParseKind::ZeroDenominator));
            }
            if mag == 1 {
                return Err(parse_err(ds - 1, ParseKind::NonCanonical));
            }
            Some((mag as i64, ds - 1))
        } else {
            None
        };

        if let Some((d, slash)) = den {
            if n == 0 {
                return Err(parse_err(slash, ParseKind::NonCanonical));
            }
            if gcd_u128(n.unsigned_abs() as u128, d as u128) > 1 {
                return Err(parse_err(slash, ParseKind::NonCanonical));
            }
            Ok(Self { num: n, den: d })
        } else {
            Ok(Self { num: n, den: 1 })
        }
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

fn checked_area(rows: usize, cols: usize) -> Result<usize, MatrixError> {
    rows.checked_mul(cols).ok_or(MatrixError::ShapeOverflow)
}

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<Rational>) -> Result<Self, MatrixError> {
        let expected = checked_area(rows, cols)?;
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
            .and_then(|base| base.checked_add(col))
            .ok_or(MatrixError::Index { row, col })?;
        self.data
            .get(index)
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
        let output_len = checked_area(self.rows, rhs.cols)?;
        let mut data = Vec::with_capacity(output_len);
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
                data.push(acc);
            }
        }
        Ok(Self {
            rows: self.rows,
            cols: rhs.cols,
            data,
        })
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
                let better = match best {
                    None => true,
                    Some(old) => {
                        let prior = matrix.data[old * matrix.cols + col];
                        (value.den, value.num.unsigned_abs(), row)
                            < (prior.den, prior.num.unsigned_abs(), old)
                    }
                };
                if better {
                    best = Some(row);
                }
            }
            let Some(q) = best else {
                continue;
            };
            if q != p {
                for c in 0..matrix.cols {
                    matrix.data.swap(p * matrix.cols + c, q * matrix.cols + c);
                }
                swaps.push((p, q));
            }

            let pivot = matrix.data[p * matrix.cols + col];
            for c in 0..matrix.cols {
                let original = matrix.data[p * matrix.cols + c];
                matrix.data[p * matrix.cols + c] =
                    original.div(&pivot).map_err(|source| MatrixError::Arithmetic {
                        row: p,
                        col: c,
                        step: p,
                        source,
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
                    let product = factor
                        .mul(&matrix.data[p * matrix.cols + c])
                        .map_err(|source| MatrixError::Arithmetic {
                            row,
                            col: c,
                            step: p,
                            source,
                        })?;
                    matrix.data[row * matrix.cols + c] = matrix.data[row * matrix.cols + c]
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

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        self.eliminate(self.cols)
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
        let area = checked_area(n, width)?;
        let mut data = Vec::with_capacity(area);
        for r in 0..n {
            for c in 0..width {
                data.push(if c < n {
                    self.data[r * n + c]
                } else if c - n == r {
                    Rational { num: 1, den: 1 }
                } else {
                    Rational { num: 0, den: 1 }
                });
            }
        }
        let augmented = Self {
            rows: n,
            cols: width,
            data,
        };
        let reduced = augmented.eliminate(n)?;
        if reduced.pivots.len() < n {
            return Err(MatrixError::Singular {
                rank: reduced.pivots.len(),
            });
        }
        let mut inverse_data = Vec::with_capacity(area);
        for r in 0..n {
            for c in 0..n {
                inverse_data.push(reduced.matrix.data[r * width + n + c]);
            }
        }
        Ok(Self {
            rows: n,
            cols: n,
            data: inverse_data,
        })
    }
}