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
        Self::from_i128(num as i128, den as i128)
    }

    fn from_i128(num: i128, den: i128) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        if num == 0 {
            return Ok(Self { num: 0, den: 1 });
        }

        let negative = (num < 0) ^ (den < 0);
        let n = num.unsigned_abs();
        let d = den.unsigned_abs();
        let g = gcd_u128(n, d);
        let rn = n / g;
        let rd = d / g;

        let max_num = if negative {
            (i64::MAX as u128) + 1
        } else {
            i64::MAX as u128
        };
        if rn > max_num || rd > i64::MAX as u128 {
            return Err(RationalError::Overflow);
        }

        let out_num = if negative {
            if rn == (i64::MAX as u128) + 1 {
                i64::MIN
            } else {
                -(rn as i64)
            }
        } else {
            rn as i64
        };
        Ok(Self {
            num: out_num,
            den: rd as i64,
        })
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
        let parse_err = |at, kind| RationalError::Parse { at, kind };
        if bytes.is_empty() {
            return Err(parse_err(0, ParseKind::Empty));
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
            return Err(parse_err(i, ParseKind::InvalidByte));
        }

        let num_end = i;
        let mut slash = None;
        let mut den_start = None;
        let mut den_end = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            slash = Some(i);
            i += 1;
            den_start = Some(i);
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
            if i < bytes.len() {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
        }

        if num_end - num_start >= 2 && bytes[num_start] == b'0' {
            return Err(parse_err(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_end - num_start == 1 && bytes[num_start] == b'0' {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }
        if let (Some(ds), Some(de)) = (den_start, den_end) {
            if de - ds >= 2 && bytes[ds] == b'0' {
                return Err(parse_err(ds + 1, ParseKind::NonCanonical));
            }
        }

        let mut nmag: u128 = 0;
        for &b in &bytes[num_start..num_end] {
            nmag = nmag.saturating_mul(10).saturating_add((b - b'0') as u128);
        }
        let nlimit = if negative {
            (i64::MAX as u128) + 1
        } else {
            i64::MAX as u128
        };
        if nmag > nlimit {
            return Err(parse_err(0, ParseKind::OutOfRange));
        }

        let (num, den) = if let (Some(ds), Some(de), Some(sl)) = (den_start, den_end, slash) {
            let mut dmag: u128 = 0;
            for &b in &bytes[ds..de] {
                dmag = dmag.saturating_mul(10).saturating_add((b - b'0') as u128);
            }
            if dmag > i64::MAX as u128 {
                return Err(parse_err(ds, ParseKind::OutOfRange));
            }
            if dmag == 0 {
                return Err(parse_err(ds, ParseKind::ZeroDenominator));
            }
            if dmag == 1 {
                return Err(parse_err(sl, ParseKind::NonCanonical));
            }
            if nmag == 0 {
                return Err(parse_err(sl, ParseKind::NonCanonical));
            }
            if gcd_u128(nmag, dmag) > 1 {
                return Err(parse_err(sl, ParseKind::NonCanonical));
            }
            (nmag, dmag)
        } else {
            (nmag, 1)
        };

        let signed_num = if negative {
            if num == (i64::MAX as u128) + 1 {
                i64::MIN
            } else {
                -(num as i64)
            }
        } else {
            num as i64
        };
        Ok(Self {
            num: signed_num,
            den: den as i64,
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
pub struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<Rational>,
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
        let out_len = self
            .rows
            .checked_mul(rhs.cols)
            .ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(out_len);
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = Rational { num: 0, den: 1 };
                for k in 0..self.cols {
                    let product = self.data[r * self.cols + k]
                        .mul(&rhs.data[k * rhs.cols + c])
                        .map_err(|source| MatrixError::Arithmetic {
                            row: r,
                            col: c,
                            step: k,
                            source,
                        })?;
                    acc = acc.add(&product).map_err(|source| MatrixError::Arithmetic {
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
        data: &mut [Rational],
        rows: usize,
        cols: usize,
        pivot_cols: usize,
    ) -> Result<(Vec<usize>, Vec<(usize, usize)>), MatrixError> {
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0;

        for candidate in 0..pivot_cols {
            if p == rows {
                break;
            }
            let mut chosen: Option<usize> = None;
            for r in p..rows {
                let value = data[r * cols + candidate];
                if value.num == 0 {
                    continue;
                }
                let key = (value.den, value.num.unsigned_abs(), r);
                if chosen.map_or(true, |q| {
                    let old = data[q * cols + candidate];
                    key < (old.den, old.num.unsigned_abs(), q)
                }) {
                    chosen = Some(r);
                }
            }
            let Some(q) = chosen else {
                continue;
            };

            if q != p {
                for c in 0..cols {
                    data.swap(p * cols + c, q * cols + c);
                }
                swaps.push((p, q));
            }

            let pivot = data[p * cols + candidate];
            for c in 0..cols {
                data[p * cols + c] = data[p * cols + c]
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
                let factor = data[r * cols + candidate];
                if factor.num == 0 {
                    continue;
                }
                for c in 0..cols {
                    let product = factor
                        .mul(&data[p * cols + c])
                        .map_err(|source| MatrixError::Arithmetic {
                            row: r,
                            col: c,
                            step: p,
                            source,
                        })?;
                    data[r * cols + c] = data[r * cols + c]
                        .sub(&product)
                        .map_err(|source| MatrixError::Arithmetic {
                            row: r,
                            col: c,
                            step: p,
                            source,
                        })?;
                }
            }

            pivots.push(candidate);
            p += 1;
        }
        Ok((pivots, swaps))
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        let mut data = self.data.clone();
        let (pivots, swaps) = Self::eliminate(&mut data, self.rows, self.cols, self.cols)?;
        Ok(Reduction {
            matrix: Self {
                rows: self.rows,
                cols: self.cols,
                data,
            },
            pivots,
            swaps,
        })
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
        for r in 0..n {
            for c in 0..n {
                augmented.push(self.data[r * n + c]);
            }
            for c in 0..n {
                augmented.push(if r == c {
                    Rational { num: 1, den: 1 }
                } else {
                    Rational { num: 0, den: 1 }
                });
            }
        }

        let (pivots, _) = Self::eliminate(&mut augmented, n, width, n)?;
        if pivots.len() < n {
            return Err(MatrixError::Singular { rank: pivots.len() });
        }

        let mut result = Vec::with_capacity(n * n);
        for r in 0..n {
            for c in 0..n {
                result.push(augmented[r * width + n + c]);
            }
        }
        Ok(Self {
            rows: n,
            cols: n,
            data: result,
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