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

impl Rational {
    const ZERO: Self = Self { num: 0, den: 1 };

    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        Self::from_i128(num as i128, den as i128)
    }

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

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
        Self::from_i128(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        Self::from_i128(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::from_i128(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(parse_error(0, ParseKind::Empty));
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
            return Err(parse_error(i, ParseKind::InvalidByte));
        }
        let num_end = i;

        let mut den_start = None;
        let mut den_end = None;
        if i < bytes.len() && bytes[i] == b'/' {
            i += 1;
            den_start = Some(i);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start.unwrap() {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
        }

        if i < bytes.len() {
            return Err(parse_error(i, ParseKind::InvalidByte));
        }

        let negative = bytes[0] == b'-';
        let num_digits = num_end - num_start;
        if num_digits >= 2 && bytes[num_start] == b'0' {
            return Err(parse_error(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_digits == 1 && bytes[num_start] == b'0' {
            return Err(parse_error(0, ParseKind::NonCanonical));
        }

        if let Some(ds) = den_start {
            let de = den_end.unwrap();
            if de - ds >= 2 && bytes[ds] == b'0' {
                return Err(parse_error(ds + 1, ParseKind::NonCanonical));
            }
        }

        let num_limit: u128 = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let num_mag = parse_magnitude(&bytes[num_start..num_end]);
        if num_mag > num_limit {
            return Err(parse_error(0, ParseKind::OutOfRange));
        }

        let mut den_mag = 1u128;
        if let Some(ds) = den_start {
            let de = den_end.unwrap();
            den_mag = parse_magnitude(&bytes[ds..de]);
            if den_mag > i64::MAX as u128 {
                return Err(parse_error(ds, ParseKind::OutOfRange));
            }
            if den_mag == 0 {
                return Err(parse_error(ds, ParseKind::ZeroDenominator));
            }

            let slash = ds - 1;
            if den_mag == 1 {
                return Err(parse_error(slash, ParseKind::NonCanonical));
            }
            if num_mag == 0 {
                return Err(parse_error(slash, ParseKind::NonCanonical));
            }
            if gcd_u128(num_mag, den_mag) > 1 {
                return Err(parse_error(slash, ParseKind::NonCanonical));
            }
        }

        let signed_num = if negative {
            -(num_mag as i128)
        } else {
            num_mag as i128
        };
        Self::from_i128(signed_num, den_mag as i128)
    }

    fn from_i128(mut num: i128, mut den: i128) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        if den < 0 {
            num = -num;
            den = -den;
        }
        if num == 0 {
            return Ok(Self::ZERO);
        }

        let g = gcd_u128(num.unsigned_abs(), den as u128) as i128;
        num /= g;
        den /= g;

        if num < i64::MIN as i128
            || num > i64::MAX as i128
            || den < 1
            || den > i64::MAX as i128
        {
            return Err(RationalError::Overflow);
        }
        Ok(Self {
            num: num as i64,
            den: den as i64,
        })
    }
}

fn parse_error(at: usize, kind: ParseKind) -> RationalError {
    RationalError::Parse { at, kind }
}

fn parse_magnitude(digits: &[u8]) -> u128 {
    let mut value = 0u128;
    for &b in digits {
        value = value * 10 + (b - b'0') as u128;
    }
    value
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
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
        Ok(self.data[row * self.cols + col])
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
        let mut data = Vec::with_capacity(out_len);

        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = Rational::ZERO;
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

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        eliminate(self.clone(), self.cols)
    }

    pub fn rank(&self) -> Result<usize, MatrixError> {
        self.rref().map(|reduction| reduction.rank())
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
        let total = n.checked_mul(width).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(total);

        for r in 0..n {
            for c in 0..width {
                if c < n {
                    data.push(self.data[r * n + c]);
                } else {
                    data.push(if c - n == r {
                        Rational { num: 1, den: 1 }
                    } else {
                        Rational::ZERO
                    });
                }
            }
        }

        let augmented = Matrix {
            rows: n,
            cols: width,
            data,
        };
        let reduction = eliminate(augmented, n)?;
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
}

fn eliminate(mut matrix: Matrix, candidate_cols: usize) -> Result<Reduction, MatrixError> {
    let mut pivots = Vec::new();
    let mut swaps = Vec::new();
    let mut p = 0usize;

    for pivot_col in 0..candidate_cols {
        if p == matrix.rows {
            break;
        }

        let mut best: Option<(i64, u64, usize)> = None;
        for r in p..matrix.rows {
            let value = matrix.data[r * matrix.cols + pivot_col];
            if value.num == 0 {
                continue;
            }
            let key = (value.den, value.num.unsigned_abs(), r);
            if best.map_or(true, |current| key < current) {
                best = Some(key);
            }
        }
        let Some((_, _, q)) = best else {
            continue;
        };

        if q != p {
            for c in 0..matrix.cols {
                matrix.data.swap(p * matrix.cols + c, q * matrix.cols + c);
            }
            swaps.push((p, q));
        }

        let pivot = matrix.data[p * matrix.cols + pivot_col];
        for c in 0..matrix.cols {
            let idx = p * matrix.cols + c;
            matrix.data[idx] = matrix.data[idx]
                .div(&pivot)
                .map_err(|source| MatrixError::Arithmetic {
                    row: p,
                    col: c,
                    step: p,
                    source,
                })?;
        }

        for r in 0..matrix.rows {
            if r == p {
                continue;
            }
            let factor = matrix.data[r * matrix.cols + pivot_col];
            if factor.num == 0 {
                continue;
            }
            for c in 0..matrix.cols {
                let product = factor
                    .mul(&matrix.data[p * matrix.cols + c])
                    .map_err(|source| MatrixError::Arithmetic {
                        row: r,
                        col: c,
                        step: p,
                        source,
                    })?;
                let idx = r * matrix.cols + c;
                matrix.data[idx] = matrix.data[idx]
                    .sub(&product)
                    .map_err(|source| MatrixError::Arithmetic {
                        row: r,
                        col: c,
                        step: p,
                        source,
                    })?;
            }
        }

        pivots.push(pivot_col);
        p += 1;
    }

    Ok(Reduction {
        matrix,
        pivots,
        swaps,
    })
}