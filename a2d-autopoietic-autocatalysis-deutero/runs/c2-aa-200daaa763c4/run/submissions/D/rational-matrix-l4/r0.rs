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

fn rational_from_i128(mut num: i128, mut den: i128) -> Result<Rational, RationalError> {
    if den == 0 {
        return Err(RationalError::ZeroDenominator);
    }
    if den < 0 {
        num = -num;
        den = -den;
    }
    if num == 0 {
        return Ok(Rational { num: 0, den: 1 });
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
    Ok(Rational {
        num: num as i64,
        den: den as i64,
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
        }
        if i < bytes.len() {
            return Err(fail(i, ParseKind::InvalidByte));
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

        let num_limit: u128 = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let mut num_mag = 0u128;
        for &b in num_digits {
            num_mag = num_mag
                .saturating_mul(10)
                .saturating_add((b - b'0') as u128);
            if num_mag > num_limit {
                return Err(fail(0, ParseKind::OutOfRange));
            }
        }

        let den_value = if let (Some(ds), Some(de)) = (den_start, den_end) {
            let mut value = 0u128;
            for &b in &bytes[ds..de] {
                value = value
                    .saturating_mul(10)
                    .saturating_add((b - b'0') as u128);
                if value > i64::MAX as u128 {
                    return Err(fail(ds, ParseKind::OutOfRange));
                }
            }
            Some(value as u64)
        } else {
            None
        };

        if let (Some(ds), Some(_)) = (den_start, den_end) {
            if den_value == Some(0) {
                return Err(fail(ds, ParseKind::ZeroDenominator));
            }
            if den_value == Some(1) {
                return Err(fail(ds - 1, ParseKind::NonCanonical));
            }
            if num_mag == 0 {
                return Err(fail(ds - 1, ParseKind::NonCanonical));
            }
            if gcd_u128(num_mag, den_value.unwrap() as u128) > 1 {
                return Err(fail(ds - 1, ParseKind::NonCanonical));
            }
        }

        let signed_num = if negative {
            if num_mag == (1u128 << 63) {
                i64::MIN
            } else {
                -(num_mag as i64)
            }
        } else {
            num_mag as i64
        };
        let den = den_value.map(|v| v as i64).unwrap_or(1);
        Ok(Rational {
            num: signed_num,
            den,
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

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<Rational>) -> Result<Self, MatrixError> {
        let expected = rows.checked_mul(cols).ok_or(MatrixError::ShapeOverflow)?;
        if data.len() != expected {
            return Err(MatrixError::DataLength {
                expected,
                actual: data.len(),
            });
        }
        Ok(Matrix { rows, cols, data })
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
            .and_then(|base| base.checked_add(col))
            .ok_or(MatrixError::Index { row, col })?;
        self.data
            .get(idx)
            .copied()
            .ok_or(MatrixError::Index { row, col })
    }

    pub fn transpose(&self) -> Self {
        let mut data = Vec::with_capacity(self.data.len());
        if self.rows != 0 && self.cols != 0 {
            for r in 0..self.cols {
                for c in 0..self.rows {
                    data.push(self.data[c * self.cols + r]);
                }
            }
        }
        Matrix {
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
        Ok(Matrix {
            rows: self.rows,
            cols: rhs.cols,
            data,
        })
    }

    fn rref_limited(&self, pivot_cols: usize) -> Result<Reduction, MatrixError> {
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
                if chosen.map_or(true, |q| {
                    let old = matrix.data[q * matrix.cols + col];
                    key < (old.den, old.num.unsigned_abs(), q)
                }) {
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

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        self.rref_limited(self.cols)
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
        let mut data = vec![Rational { num: 0, den: 1 }; len];
        for r in 0..n {
            for c in 0..n {
                data[r * width + c] = self.data[r * n + c];
            }
            data[r * width + n + r] = Rational { num: 1, den: 1 };
        }
        let augmented = Matrix {
            rows: n,
            cols: width,
            data,
        };
        let reduction = augmented.rref_limited(n)?;
        if reduction.pivots.len() < n {
            return Err(MatrixError::Singular {
                rank: reduction.pivots.len(),
            });
        }
        let mut inverse = Vec::with_capacity(n * n);
        for r in 0..n {
            for c in 0..n {
                inverse.push(reduction.matrix.data[r * width + n + c]);
            }
        }
        Ok(Matrix {
            rows: n,
            cols: n,
            data: inverse,
        })
    }

    pub fn solve(&self, rhs: &Self) -> Result<Solution, SolveError> {
        if self.rows != rhs.rows {
            return Err(SolveError::RowMismatch {
                coefficients: self.rows,
                rhs: rhs.rows,
            });
        }
        let m = self.rows;
        let n = self.cols;
        let k = rhs.cols;
        let width = n
            .checked_add(k)
            .ok_or(SolveError::AugmentedShapeOverflow)?;
        let len = m
            .checked_mul(width)
            .ok_or(SolveError::AugmentedShapeOverflow)?;

        let mut data = vec![Rational { num: 0, den: 1 }; len];
        for r in 0..m {
            for c in 0..n {
                data[r * width + c] = self.data[r * n + c];
            }
            for c in 0..k {
                data[r * width + n + c] = rhs.data[r * k + c];
            }
        }
        let augmented = Matrix {
            rows: m,
            cols: width,
            data,
        };
        let reduction = augmented
            .rref_limited(n)
            .map_err(SolveError::Matrix)?;

        for row in 0..m {
            let all_zero = (0..n).all(|c| reduction.matrix.data[row * width + c].num == 0);
            if all_zero {
                for rhs_col in 0..k {
                    let value = reduction.matrix.data[row * width + n + rhs_col];
                    if value.num != 0 {
                        let start = row * width;
                        return Ok(Solution::None {
                            row,
                            rhs_col,
                            witness: reduction.matrix.data[start..start + width].to_vec(),
                        });
                    }
                }
            }
        }

        let rank = reduction.pivots.len();
        let mut is_pivot = vec![false; n];
        for &p in &reduction.pivots {
            is_pivot[p] = true;
        }
        let free: Vec<usize> = (0..n).filter(|&c| !is_pivot[c]).collect();

        let particular_len = n
            .checked_mul(k)
            .ok_or_else(|| SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut particular_data = vec![Rational { num: 0, den: 1 }; particular_len];
        for (i, &pivot_col) in reduction.pivots.iter().enumerate() {
            for rhs_col in 0..k {
                particular_data[pivot_col * k + rhs_col] =
                    reduction.matrix.data[i * width + n + rhs_col];
            }
        }
        let particular = Matrix {
            rows: n,
            cols: k,
            data: particular_data,
        };

        if rank == n {
            return Ok(Solution::Unique {
                values: particular,
                pivots: reduction.pivots,
            });
        }

        let basis_len = n
            .checked_mul(free.len())
            .ok_or_else(|| SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut basis_data = vec![Rational { num: 0, den: 1 }; basis_len];
        for (j, &free_col) in free.iter().enumerate() {
            basis_data[free_col * free.len() + j] = Rational { num: 1, den: 1 };
            for (i, &pivot_col) in reduction.pivots.iter().enumerate() {
                let coefficient = reduction.matrix.data[i * width + free_col];
                let negative = coefficient.neg().map_err(|source| {
                    SolveError::Matrix(MatrixError::Arithmetic {
                        row: pivot_col,
                        col: j,
                        step: rank,
                        source,
                    })
                })?;
                basis_data[pivot_col * free.len() + j] = negative;
            }
        }
        let basis = Matrix {
            rows: n,
            cols: free.len(),
            data: basis_data,
        };

        Ok(Solution::Infinite {
            particular,
            free,
            basis,
            pivots: reduction.pivots,
        })
    }
}