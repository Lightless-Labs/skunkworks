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
    fn from_i128(num: i128, den: i128) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }

        let (mut n, mut d) = if den < 0 { (-num, -den) } else { (num, den) };
        if n == 0 {
            return Ok(Self { num: 0, den: 1 });
        }

        fn gcd(mut a: u128, mut b: u128) -> u128 {
            while b != 0 {
                let r = a % b;
                a = b;
                b = r;
            }
            a
        }

        let g = gcd(n.unsigned_abs(), d as u128) as i128;
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
        let err = |at, kind| RationalError::Parse { at, kind };
        if bytes.is_empty() {
            return Err(err(0, ParseKind::Empty));
        }

        let slash = bytes.iter().position(|&b| b == b'/');
        let num_end = slash.unwrap_or(bytes.len());
        let mut i = 0;
        if bytes[0] == b'-' {
            i = 1;
        }
        if i == num_end {
            return Err(err(i, ParseKind::InvalidByte));
        }
        for (idx, &b) in bytes.iter().enumerate() {
            if idx == 0 && b == b'-' {
                continue;
            }
            if idx == num_end && slash.is_some() {
                continue;
            }
            if idx > num_end && slash.is_some() {
                if !b.is_ascii_digit() {
                    return Err(err(idx, ParseKind::InvalidByte));
                }
                continue;
            }
            if idx < num_end && !b.is_ascii_digit() {
                return Err(err(idx, ParseKind::InvalidByte));
            }
        }
        if let Some(s) = slash {
            if s + 1 == bytes.len() {
                return Err(err(bytes.len(), ParseKind::InvalidByte));
            }
            if bytes[s + 1..].iter().any(|b| !b.is_ascii_digit()) {
                let at = bytes[s + 1..]
                    .iter()
                    .position(|b| !b.is_ascii_digit())
                    .unwrap()
                    + s
                    + 1;
                return Err(err(at, ParseKind::InvalidByte));
            }
        }

        let num_digits_start = usize::from(bytes[0] == b'-');
        let num_digits = num_end - num_digits_start;
        if num_digits >= 2 && bytes[num_digits_start] == b'0' {
            return Err(err(num_digits_start + 1, ParseKind::NonCanonical));
        }
        if bytes[0] == b'-' && bytes[1..num_end] == *b"0" {
            return Err(err(0, ParseKind::NonCanonical));
        }

        let den_start = slash.map(|s| s + 1);
        if let Some(ds) = den_start {
            if bytes.len() - ds >= 2 && bytes[ds] == b'0' {
                return Err(err(ds + 1, ParseKind::NonCanonical));
            }
        }

        let negative = bytes[0] == b'-';
        let mut nmag: u128 = 0;
        for &b in &bytes[num_digits_start..num_end] {
            nmag = nmag
                .saturating_mul(10)
                .saturating_add((b - b'0') as u128);
        }
        let nlimit = if negative { 1u128 << 63 } else { i64::MAX as u128 };
        if nmag > nlimit {
            return Err(err(0, ParseKind::OutOfRange));
        }

        let mut dmag = 1u128;
        if let Some(ds) = den_start {
            dmag = 0;
            for &b in &bytes[ds..] {
                dmag = dmag
                    .saturating_mul(10)
                    .saturating_add((b - b'0') as u128);
            }
            if dmag > i64::MAX as u128 {
                return Err(err(ds, ParseKind::OutOfRange));
            }
            if dmag == 0 {
                return Err(err(ds, ParseKind::ZeroDenominator));
            }
            if dmag == 1 {
                return Err(err(slash.unwrap(), ParseKind::NonCanonical));
            }
            if nmag == 0 {
                return Err(err(slash.unwrap(), ParseKind::NonCanonical));
            }
            fn gcd(mut a: u128, mut b: u128) -> u128 {
                while b != 0 {
                    let r = a % b;
                    a = b;
                    b = r;
                }
                a
            }
            if gcd(nmag, dmag) > 1 {
                return Err(err(slash.unwrap(), ParseKind::NonCanonical));
            }
        }

        let n = if negative {
            if nmag == 1u128 << 63 {
                i64::MIN
            } else {
                -(nmag as i64)
            }
        } else {
            nmag as i64
        };
        Ok(Self {
            num: n,
            den: dmag as i64,
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
        let len = rows.checked_mul(cols).ok_or(MatrixError::ShapeOverflow)?;
        if data.len() != len {
            return Err(MatrixError::DataLength {
                expected: len,
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

    fn eliminate(
        mut augmented: Vec<Rational>,
        rows: usize,
        width: usize,
        pivot_cols: usize,
    ) -> Result<Reduction, MatrixError> {
        let mut p = 0usize;
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let zero = Rational { num: 0, den: 1 };

        for col in 0..pivot_cols {
            if p == rows {
                break;
            }
            let mut chosen: Option<usize> = None;
            for r in p..rows {
                let value = augmented[r * width + col];
                if value.num == 0 {
                    continue;
                }
                let key = (value.den, value.num.unsigned_abs(), r);
                if chosen.map_or(true, |old| {
                    let v = augmented[old * width + col];
                    key < (v.den, v.num.unsigned_abs(), old)
                }) {
                    chosen = Some(r);
                }
            }
            let Some(q) = chosen else {
                continue;
            };
            if q != p {
                for c in 0..width {
                    augmented.swap(p * width + c, q * width + c);
                }
                swaps.push((p, q));
            }

            let pivot = augmented[p * width + col];
            for c in 0..width {
                augmented[p * width + c] =
                    augmented[p * width + c]
                        .div(&pivot)
                        .map_err(|source| MatrixError::Arithmetic {
                            row: p,
                            col: c,
                            step: p,
                            source,
                        })?;
            }

            for r in 0..rows {
                if r == p {
                    continue;
                }
                let factor = augmented[r * width + col];
                if factor == zero {
                    continue;
                }
                for c in 0..width {
                    let product = factor.mul(&augmented[p * width + c]).map_err(
                        |source| MatrixError::Arithmetic {
                            row: r,
                            col: c,
                            step: p,
                            source,
                        },
                    )?;
                    augmented[r * width + c] =
                        augmented[r * width + c]
                            .sub(&product)
                            .map_err(|source| MatrixError::Arithmetic {
                                row: r,
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
            matrix: Matrix {
                rows,
                cols: width,
                data: augmented,
            },
            pivots,
            swaps,
        })
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        Self::eliminate(self.data.clone(), self.rows, self.cols, self.cols)
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
        let mut augmented = Vec::with_capacity(len);
        let zero = Rational { num: 0, den: 1 };
        let one = Rational { num: 1, den: 1 };
        for r in 0..n {
            for c in 0..n {
                augmented.push(self.data[r * n + c]);
            }
            for c in 0..n {
                augmented.push(if r == c { one } else { zero });
            }
        }
        let red = Self::eliminate(augmented, n, width, n)?;
        if red.pivots.len() < n {
            return Err(MatrixError::Singular {
                rank: red.pivots.len(),
            });
        }
        let mut data = Vec::with_capacity(n * n);
        for r in 0..n {
            for c in n..width {
                data.push(red.matrix.data[r * width + c]);
            }
        }
        Ok(Self {
            rows: n,
            cols: n,
            data,
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

fn main() {}