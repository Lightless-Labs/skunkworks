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
        rational_from_i128(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::Empty,
            });
        }

        let mut slash = None;
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'/' {
                if slash.is_some() || i == 0 || i + 1 == bytes.len() {
                    return Err(RationalError::Parse {
                        at: i,
                        kind: ParseKind::InvalidByte,
                    });
                }
                slash = Some(i);
                i += 1;
                continue;
            }
            if b.is_ascii_digit() {
                i += 1;
                continue;
            }
            if b == b'-' && i == 0 {
                i += 1;
                if i == bytes.len() || !bytes[i].is_ascii_digit() {
                    return Err(RationalError::Parse {
                        at: i,
                        kind: ParseKind::InvalidByte,
                    });
                }
                continue;
            }
            return Err(RationalError::Parse {
                at: i,
                kind: ParseKind::InvalidByte,
            });
        }

        let n_end = slash.unwrap_or(bytes.len());
        let n_start = usize::from(bytes[0] == b'-');
        if n_start == n_end {
            return Err(RationalError::Parse {
                at: n_end,
                kind: ParseKind::InvalidByte,
            });
        }
        if let Some(s) = slash {
            if s + 1 == bytes.len() {
                return Err(RationalError::Parse {
                    at: bytes.len(),
                    kind: ParseKind::InvalidByte,
                });
            }
        }

        let num_digits = n_end - n_start;
        if num_digits >= 2 && bytes[n_start] == b'0' {
            return Err(RationalError::Parse {
                at: n_start + 1,
                kind: ParseKind::NonCanonical,
            });
        }
        if bytes[0] == b'-' && num_digits == 1 && bytes[n_start] == b'0' {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::NonCanonical,
            });
        }

        let mut num_mag: u128 = 0;
        for &b in &bytes[n_start..n_end] {
            num_mag = num_mag * 10 + (b - b'0') as u128;
        }
        let neg = bytes[0] == b'-';
        let num_limit = if neg { 1u128 << 63 } else { i64::MAX as u128 };
        if num_mag > num_limit {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::OutOfRange,
            });
        }

        let mut den_mag = 1u128;
        let den_start = slash.map(|s| s + 1);
        if let Some(ds) = den_start {
            let de = bytes.len();
            if de - ds >= 2 && bytes[ds] == b'0' {
                return Err(RationalError::Parse {
                    at: ds + 1,
                    kind: ParseKind::NonCanonical,
                });
            }
            den_mag = 0;
            for &b in &bytes[ds..de] {
                den_mag = den_mag * 10 + (b - b'0') as u128;
            }
            if den_mag > i64::MAX as u128 {
                return Err(RationalError::Parse {
                    at: ds,
                    kind: ParseKind::OutOfRange,
                });
            }
            if den_mag == 0 {
                return Err(RationalError::Parse {
                    at: ds,
                    kind: ParseKind::ZeroDenominator,
                });
            }
            if den_mag == 1 {
                return Err(RationalError::Parse {
                    at: slash.unwrap(),
                    kind: ParseKind::NonCanonical,
                });
            }
            if num_mag == 0 {
                return Err(RationalError::Parse {
                    at: slash.unwrap(),
                    kind: ParseKind::NonCanonical,
                });
            }
            if gcd_u128(num_mag, den_mag) > 1 {
                return Err(RationalError::Parse {
                    at: slash.unwrap(),
                    kind: ParseKind::NonCanonical,
                });
            }
        }

        let signed_num = if neg {
            -(num_mag as i128)
        } else {
            num_mag as i128
        };
        rational_from_i128(signed_num, den_mag as i128)
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

fn zero() -> Rational {
    Rational { num: 0, den: 1 }
}

fn one() -> Rational {
    Rational { num: 1, den: 1 }
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
            .and_then(|v| v.checked_add(col))
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
        let len = self
            .rows
            .checked_mul(rhs.cols)
            .ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = zero();
                for k in 0..self.cols {
                    let left = self.get(r, k)?;
                    let right = rhs.get(k, c)?;
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

    fn eliminate(
        &self,
        candidate_cols: usize,
        total_cols: usize,
    ) -> Result<Reduction, MatrixError> {
        let mut work = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;

        for pivot_col in 0..candidate_cols {
            if p == work.rows {
                break;
            }

            let mut best: Option<(i64, u64, usize)> = None;
            for row in p..work.rows {
                let value = work.get(row, pivot_col)?;
                if value.num == 0 {
                    continue;
                }
                let candidate = (value.den, value.num.unsigned_abs(), row);
                if best.map(|current| candidate < current).unwrap_or(true) {
                    best = Some(candidate);
                }
            }
            let q = match best {
                Some((_, _, row)) => row,
                None => continue,
            };

            if q != p {
                for c in 0..total_cols {
                    let a = p * total_cols + c;
                    let b = q * total_cols + c;
                    work.data.swap(a, b);
                }
                swaps.push((p, q));
            }

            let pivot = work.get(p, pivot_col)?;
            for c in 0..total_cols {
                let idx = p * total_cols + c;
                work.data[idx] = work.data[idx]
                    .div(&pivot)
                    .map_err(|source| MatrixError::Arithmetic {
                        row: p,
                        col: c,
                        step: p,
                        source,
                    })?;
            }

            for row in 0..work.rows {
                if row == p {
                    continue;
                }
                let factor = work.get(row, pivot_col)?;
                if factor.num == 0 {
                    continue;
                }
                for c in 0..total_cols {
                    let idx = row * total_cols + c;
                    let product = factor
                        .mul(&work.data[p * total_cols + c])
                        .map_err(|source| MatrixError::Arithmetic {
                            row,
                            col: c,
                            step: p,
                            source,
                        })?;
                    work.data[idx] = work.data[idx]
                        .sub(&product)
                        .map_err(|source| MatrixError::Arithmetic {
                            row,
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
            matrix: work,
            pivots,
            swaps,
        })
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        self.eliminate(self.cols, self.cols)
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
        let mut data = vec![zero(); len];

        for r in 0..n {
            for c in 0..n {
                data[r * width + c] = self.data[r * n + c];
            }
            data[r * width + n + r] = one();
        }

        let augmented = Matrix {
            rows: n,
            cols: width,
            data,
        };
        let reduction = augmented.eliminate(n, width)?;
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
        Ok(Matrix {
            rows: n,
            cols: n,
            data: inverse_data,
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