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
    fn from_i128(num: i128, den: i128) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        let (mut n, mut d) = (num, den);
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
        if n < i64::MIN as i128 || n > i64::MAX as i128 || d < 1 || d > i64::MAX as i128 {
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
        Self::from_i128(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        let fail = |at, kind| RationalError::Parse { at, kind };
        if bytes.is_empty() {
            return Err(fail(0, ParseKind::Empty));
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
                return Err(fail(i, ParseKind::InvalidByte));
            }
            return Err(fail(i, ParseKind::InvalidByte));
        }

        let num_end = i;
        let mut den_start = None;
        let mut den_end = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(fail(i, ParseKind::InvalidByte));
            }
            i += 1;
            den_start = Some(i);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start.unwrap() {
                return Err(fail(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
            if i < bytes.len() {
                return Err(fail(i, ParseKind::InvalidByte));
            }
        }

        let num_digits = &bytes[num_start..num_end];
        if num_digits.len() >= 2 && num_digits[0] == b'0' {
            return Err(fail(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_digits == b"0" {
            return Err(fail(0, ParseKind::NonCanonical));
        }
        if let (Some(ds), Some(de)) = (den_start, den_end) {
            let digits = &bytes[ds..de];
            if digits.len() >= 2 && digits[0] == b'0' {
                return Err(fail(ds + 1, ParseKind::NonCanonical));
            }
        }

        let mut mag: u128 = 0;
        for &b in num_digits {
            mag = mag * 10 + (b - b'0') as u128;
            let limit = if negative { 1u128 << 63 } else { i64::MAX as u128 };
            if mag > limit {
                return Err(fail(0, ParseKind::OutOfRange));
            }
        }
        let num = if negative {
            if mag == 1u128 << 63 {
                i64::MIN
            } else {
                -(mag as i64)
            }
        } else {
            mag as i64
        };

        if let (Some(ds), Some(de)) = (den_start, den_end) {
            let digits = &bytes[ds..de];
            let mut den_mag: u128 = 0;
            for &b in digits {
                den_mag = den_mag * 10 + (b - b'0') as u128;
                if den_mag > i64::MAX as u128 {
                    return Err(fail(ds, ParseKind::OutOfRange));
                }
            }
            if den_mag == 0 {
                return Err(fail(ds, ParseKind::ZeroDenominator));
            }
            if den_mag == 1 {
                return Err(fail(ds - 1, ParseKind::NonCanonical));
            }
            if num == 0 {
                return Err(fail(ds - 1, ParseKind::NonCanonical));
            }
            if gcd_u128(num.unsigned_abs() as u128, den_mag) > 1 {
                return Err(fail(ds - 1, ParseKind::NonCanonical));
            }
            return Ok(Self {
                num,
                den: den_mag as i64,
            });
        }
        Ok(Self { num, den: 1 })
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
        let len = self
            .rows
            .checked_mul(rhs.cols)
            .ok_or(MatrixError::ShapeOverflow)?;
        let zero = Rational { num: 0, den: 1 };
        let mut data = Vec::with_capacity(len);
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
        &self,
        candidate_cols: usize,
    ) -> Result<Reduction, MatrixError> {
        let mut out = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0;

        for col in 0..candidate_cols {
            if p == out.rows {
                break;
            }
            let mut best: Option<usize> = None;
            for row in p..out.rows {
                let value = out.data[row * out.cols + col];
                if value.num == 0 {
                    continue;
                }
                let key = (value.den, value.num.unsigned_abs(), row);
                if best.map_or(true, |b| {
                    let v = out.data[b * out.cols + col];
                    key < (v.den, v.num.unsigned_abs(), b)
                }) {
                    best = Some(row);
                }
            }
            let Some(q) = best else {
                continue;
            };

            if q != p {
                for c in 0..out.cols {
                    out.data.swap(p * out.cols + c, q * out.cols + c);
                }
                swaps.push((p, q));
            }

            let pivot = out.data[p * out.cols + col];
            for c in 0..out.cols {
                let original = out.data[p * out.cols + c];
                out.data[p * out.cols + c] =
                    original
                        .div(&pivot)
                        .map_err(|source| MatrixError::Arithmetic {
                            row: p,
                            col: c,
                            step: p,
                            source,
                        })?;
            }

            for row in 0..out.rows {
                if row == p {
                    continue;
                }
                let factor = out.data[row * out.cols + col];
                if factor.num == 0 {
                    continue;
                }
                for c in 0..out.cols {
                    let product = factor
                        .mul(&out.data[p * out.cols + c])
                        .map_err(|source| MatrixError::Arithmetic {
                            row,
                            col: c,
                            step: p,
                            source,
                        })?;
                    out.data[row * out.cols + c] =
                        out.data[row * out.cols + c]
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
            matrix: out,
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
        let len = n.checked_mul(width).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        let zero = Rational { num: 0, den: 1 };
        let one = Rational { num: 1, den: 1 };
        for r in 0..n {
            for c in 0..width {
                data.push(if c < n {
                    self.data[r * n + c]
                } else if c - n == r {
                    one
                } else {
                    zero
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
        let mut result = Vec::with_capacity(n * n);
        for r in 0..n {
            for c in 0..n {
                result.push(reduced.matrix.data[r * width + n + c]);
            }
        }
        Ok(Self {
            rows: n,
            cols: n,
            data: result,
        })
    }

    pub fn solve(&self, rhs: &Self) -> Result<Solution, SolveError> {
        if self.rows != rhs.rows {
            return Err(SolveError::RowMismatch {
                coefficients: self.rows,
                rhs: rhs.rows,
            });
        }
        let n = self.cols;
        let k = rhs.cols;
        let width = n.checked_add(k).ok_or(SolveError::AugmentedShapeOverflow)?;
        let len = self
            .rows
            .checked_mul(width)
            .ok_or(SolveError::AugmentedShapeOverflow)?;

        let mut data = Vec::with_capacity(len);
        for r in 0..self.rows {
            data.extend_from_slice(&self.data[r * n..r * n + n]);
            data.extend_from_slice(&rhs.data[r * k..r * k + k]);
        }
        let augmented = Self {
            rows: self.rows,
            cols: width,
            data,
        };
        let reduced = augmented
            .eliminate(n)
            .map_err(SolveError::Matrix)?;

        for row in 0..self.rows {
            let coeff_zero = (0..n).all(|c| reduced.matrix.data[row * width + c].num == 0);
            if coeff_zero {
                for rhs_col in 0..k {
                    let value = reduced.matrix.data[row * width + n + rhs_col];
                    if value.num != 0 {
                        let witness =
                            reduced.matrix.data[row * width..row * width + width].to_vec();
                        return Ok(Solution::None {
                            row,
                            rhs_col,
                            witness,
                        });
                    }
                }
            }
        }

        let rank = reduced.pivots.len();
        let mut free = Vec::new();
        let mut is_pivot = vec![false; n];
        for &p in &reduced.pivots {
            is_pivot[p] = true;
        }
        for c in 0..n {
            if !is_pivot[c] {
                free.push(c);
            }
        }

        let particular_len = n.checked_mul(k).ok_or(SolveError::Matrix(
            MatrixError::ShapeOverflow,
        ))?;
        let zero = Rational { num: 0, den: 1 };
        let mut particular_data = vec![zero; particular_len];
        for (row, &pivot_col) in reduced.pivots.iter().enumerate() {
            for rhs_col in 0..k {
                particular_data[pivot_col * k + rhs_col] =
                    reduced.matrix.data[row * width + n + rhs_col];
            }
        }
        let particular = Self {
            rows: n,
            cols: k,
            data: particular_data,
        };

        if rank == n {
            return Ok(Solution::Unique {
                values: particular,
                pivots: reduced.pivots,
            });
        }

        let f = free.len();
        let basis_len = n.checked_mul(f).ok_or(SolveError::Matrix(
            MatrixError::ShapeOverflow,
        ))?;
        let mut basis_data = vec![zero; basis_len];
        let one = Rational { num: 1, den: 1 };
        for (basis_col, &free_col) in free.iter().enumerate() {
            basis_data[free_col * f + basis_col] = one;
            for (pivot_row, &pivot_col) in reduced.pivots.iter().enumerate() {
                let value = reduced.matrix.data[pivot_row * width + free_col];
                basis_data[pivot_col * f + basis_col] =
                    value.neg().map_err(|source| {
                        SolveError::Matrix(MatrixError::Arithmetic {
                            row: pivot_col,
                            col: basis_col,
                            step: rank,
                            source,
                        })
                    })?;
            }
        }
        Ok(Solution::Infinite {
            particular,
            free,
            basis: Self {
                rows: n,
                cols: f,
                data: basis_data,
            },
            pivots: reduced.pivots,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SolveError {
    RowMismatch {
        coefficients: usize,
        rhs: usize,
    },
    AugmentedShapeOverflow,
    Matrix(MatrixError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Solution {
    Unique {
        values: Matrix,
        pivots: Vec<usize>,
    },
    None {
        row: usize,
        rhs_col: usize,
        witness: Vec<Rational>,
    },
    Infinite {
        particular: Matrix,
        free: Vec<usize>,
        basis: Matrix,
        pivots: Vec<usize>,
    },
}