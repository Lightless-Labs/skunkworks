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

    let mut n = num;
    let mut d = den;
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
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        if num == 0 {
            return Ok(Self { num: 0, den: 1 });
        }

        let n = num as i128;
        let d = den as i128;
        let (n, d) = if d < 0 { (-n, -d) } else { (n, d) };
        let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
        let n = n / g;
        let d = d / g;

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
        rational_from_i128(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        rational_from_i128(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        if self.num == i64::MIN {
            return Err(RationalError::Overflow);
        }
        Ok(Self {
            num: -self.num,
            den: self.den,
        })
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::Empty,
            });
        }

        let mut i = 0;
        if bytes[i] == b'-' {
            i += 1;
        }
        let numerator_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == numerator_start {
            return Err(RationalError::Parse {
                at: i,
                kind: if i == bytes.len() {
                    ParseKind::InvalidByte
                } else {
                    ParseKind::InvalidByte
                },
            });
        }

        let numerator_end = i;
        let mut denominator_start = None;
        let mut denominator_end = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
            i += 1;
            denominator_start = Some(i);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == denominator_start.unwrap() {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
            denominator_end = Some(i);
            if i < bytes.len() {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
        }

        let negative = bytes[0] == b'-';
        let numerator_digits = &bytes[numerator_start..numerator_end];
        if numerator_digits.len() >= 2 && numerator_digits[0] == b'0' {
            return Err(RationalError::Parse {
                at: numerator_start + 1,
                kind: ParseKind::NonCanonical,
            });
        }
        if negative && numerator_digits == b"0" {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::NonCanonical,
            });
        }

        if let Some(ds) = denominator_start {
            let de = denominator_end.unwrap();
            if de - ds >= 2 && bytes[ds] == b'0' {
                return Err(RationalError::Parse {
                    at: ds + 1,
                    kind: ParseKind::NonCanonical,
                });
            }
        }

        let parse_magnitude = |digits: &[u8], limit: u128| -> Option<u128> {
            let mut value = 0u128;
            for &b in digits {
                value = value.checked_mul(10)?.checked_add((b - b'0') as u128)?;
                if value > limit {
                    return None;
                }
            }
            Some(value)
        };

        let n_limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let n_abs = parse_magnitude(numerator_digits, n_limit).ok_or(RationalError::Parse {
            at: 0,
            kind: ParseKind::OutOfRange,
        })?;

        let mut d_abs = 1u128;
        if let (Some(ds), Some(de)) = (denominator_start, denominator_end) {
            d_abs = parse_magnitude(&bytes[ds..de], i64::MAX as u128).ok_or(
                RationalError::Parse {
                    at: ds,
                    kind: ParseKind::OutOfRange,
                },
            )?;
            if d_abs == 0 {
                return Err(RationalError::Parse {
                    at: ds,
                    kind: ParseKind::ZeroDenominator,
                });
            }
        }

        if let (Some(ds), Some(de)) = (denominator_start, denominator_end) {
            if d_abs == 1 {
                return Err(RationalError::Parse {
                    at: ds - 1,
                    kind: ParseKind::NonCanonical,
                });
            }
            if n_abs == 0 {
                return Err(RationalError::Parse {
                    at: ds - 1,
                    kind: ParseKind::NonCanonical,
                });
            }
            if gcd_u128(n_abs, d_abs) > 1 {
                return Err(RationalError::Parse {
                    at: ds - 1,
                    kind: ParseKind::NonCanonical,
                });
            }
        }

        let n = if negative {
            if n_abs == (1u128 << 63) {
                i64::MIN
            } else {
                -(n_abs as i64)
            }
        } else {
            n_abs as i64
        };
        Ok(Self {
            num: n,
            den: d_abs as i64,
        })
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
        Ok(self.data[row * self.cols + col])
    }

    pub fn transpose(&self) -> Self {
        let mut data = Vec::with_capacity(self.data.len());
        for c in 0..self.cols {
            for r in 0..self.rows {
                data.push(self.data[r * self.cols + c]);
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
        let len = self
            .rows
            .checked_mul(rhs.cols)
            .ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = Rational { num: 0, den: 1 };
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
        let augmented = Self {
            rows: n,
            cols: width,
            data,
        };
        let reduction = augmented.eliminate(n)?;
        if reduction.pivots.len() < n {
            return Err(MatrixError::Singular {
                rank: reduction.pivots.len(),
            });
        }
        let mut inverse_data = Vec::with_capacity(n * n);
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

    fn eliminate(&self, pivot_cols: usize) -> Result<Reduction, MatrixError> {
        let mut matrix = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;

        for col in 0..pivot_cols {
            if p == matrix.rows {
                break;
            }
            let mut chosen: Option<usize> = None;
            for row in p..matrix.rows {
                let value = matrix.data[row * matrix.cols + col];
                if value.num == 0 {
                    continue;
                }
                let key = (value.den, value.num.unsigned_abs(), row);
                if chosen
                    .map(|best| {
                        let b = matrix.data[best * matrix.cols + col];
                        key < (b.den, b.num.unsigned_abs(), best)
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
                for c in 0..matrix.cols {
                    matrix.data.swap(p * matrix.cols + c, q * matrix.cols + c);
                }
                swaps.push((p, q));
            }

            let pivot = matrix.data[p * matrix.cols + col];
            let mut normalized = Vec::with_capacity(matrix.cols);
            for c in 0..matrix.cols {
                normalized.push(
                    matrix.data[p * matrix.cols + c]
                        .div(&pivot)
                        .map_err(|source| MatrixError::Arithmetic {
                            row: p,
                            col: c,
                            step: p,
                            source,
                        })?,
                );
            }
            for c in 0..matrix.cols {
                matrix.data[p * matrix.cols + c] = normalized[c];
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