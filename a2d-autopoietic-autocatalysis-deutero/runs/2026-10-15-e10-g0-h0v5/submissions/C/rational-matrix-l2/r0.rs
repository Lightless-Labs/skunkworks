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
        let n = self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        Self::from_i128(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        Self::from_i128(n, d)
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
        let n = -(self.num as i128);
        if n < i64::MIN as i128 || n > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }
        Ok(Self {
            num: n as i64,
            den: self.den,
        })
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        fn err(at: usize, kind: ParseKind) -> RationalError {
            RationalError::Parse { at, kind }
        }

        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(err(0, ParseKind::Empty));
        }

        let mut i = 0;
        if bytes[i] == b'-' {
            i += 1;
        }

        let num_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return Err(err(i, ParseKind::InvalidByte));
        }

        let num_end = i;
        let mut den_start = None;
        let mut den_end = None;

        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(err(i, ParseKind::InvalidByte));
            }
            i += 1;
            den_start = Some(i);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start.unwrap() {
                return Err(err(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
            if i < bytes.len() {
                return Err(err(i, ParseKind::InvalidByte));
            }
        }

        let numerator_digits = &bytes[num_start..num_end];
        if numerator_digits.len() >= 2 && numerator_digits[0] == b'0' {
            return Err(err(if bytes[0] == b'-' { 2 } else { 1 }, ParseKind::NonCanonical));
        }
        if bytes[0] == b'-' && numerator_digits == b"0" {
            return Err(err(0, ParseKind::NonCanonical));
        }

        if let (Some(ds), Some(de)) = (den_start, den_end) {
            let digits = &bytes[ds..de];
            if digits.len() >= 2 && digits[0] == b'0' {
                return Err(err(ds + 1, ParseKind::NonCanonical));
            }
        }

        let negative = bytes[0] == b'-';
        let mut nmag: u128 = 0;
        for &b in numerator_digits {
            nmag = nmag * 10 + (b - b'0') as u128;
            let limit = if negative {
                1u128 << 63
            } else {
                i64::MAX as u128
            };
            if nmag > limit {
                return Err(err(0, ParseKind::OutOfRange));
            }
        }

        let num = if negative {
            if nmag == (1u128 << 63) {
                i64::MIN
            } else {
                -(nmag as i64)
            }
        } else {
            nmag as i64
        };

        let Some(ds) = den_start else {
            return Ok(Self { num, den: 1 });
        };
        let de = den_end.unwrap();
        let mut dmag: u128 = 0;
        for &b in &bytes[ds..de] {
            dmag = dmag * 10 + (b - b'0') as u128;
            if dmag > i64::MAX as u128 {
                return Err(err(ds, ParseKind::OutOfRange));
            }
        }
        if dmag == 0 {
            return Err(err(ds, ParseKind::ZeroDenominator));
        }

        let slash = ds - 1;
        if dmag == 1 || num == 0 {
            return Err(err(slash, ParseKind::NonCanonical));
        }
        if gcd_u128(num.unsigned_abs() as u128, dmag) > 1 {
            return Err(err(slash, ParseKind::NonCanonical));
        }

        Ok(Self {
            num,
            den: dmag as i64,
        })
    }

    fn from_i128(n: i128, d: i128) -> Result<Self, RationalError> {
        if d == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        if n == 0 {
            return Ok(Self { num: 0, den: 1 });
        }
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
        let output_len = self
            .rows
            .checked_mul(rhs.cols)
            .ok_or(MatrixError::ShapeOverflow)?;
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
}

fn main() {}