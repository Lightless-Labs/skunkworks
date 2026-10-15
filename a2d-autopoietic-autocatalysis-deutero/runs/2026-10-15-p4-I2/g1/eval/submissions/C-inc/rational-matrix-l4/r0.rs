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

    let g = gcd_u128(n.unsigned_abs(), d as u128);
    n /= g as i128;
    d /= g as i128;

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
        let n = self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        rational_from_i128(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        rational_from_i128(n, d)
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

        if i < bytes.len() && bytes[i] == b'/' {
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

        let num_digits = num_end - num_start;
        if num_digits >= 2 && bytes[num_start] == b'0' {
            return Err(fail(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_digits == 1 && bytes[num_start] == b'0' {
            return Err(fail(0, ParseKind::NonCanonical));
        }

        if let (Some(ds), Some(de)) = (den_start, den_end) {
            if de - ds >= 2 && bytes[ds] == b'0' {
                return Err(fail(ds + 1, ParseKind::NonCanonical));
            }
        }

        let mut num_mag: u128 = 0;
        for &b in &bytes[num_start..num_end] {
            num_mag = num_mag * 10 + (b - b'0') as u128;
        }
        let num_limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        if num_mag > num_limit {
            return Err(fail(0, ParseKind::OutOfRange));
        }

        let mut den_mag: u128 = 1;
        if let (Some(ds), Some(de)) = (den_start, den_end) {
            den_mag = 0;
            for &b in &bytes[ds..de] {
                den_mag = den_mag * 10 + (b - b'0') as u128;
            }
            if den_mag > i64::MAX as u128 {
                return Err(fail(ds, ParseKind::OutOfRange));
            }
            if den_mag == 0 {
                return Err(fail(ds, ParseKind::ZeroDenominator));
            }
        }

        if let (Some(ds), Some(_)) = (den_start, den_end) {
            let slash = ds - 1;
            if den_mag == 1 {
                return Err(fail(slash, ParseKind::NonCanonical));
            }
            if num_mag == 0 {
                return Err(fail(slash, ParseKind::NonCanonical));
            }
            if gcd_u128(num_mag, den_mag) > 1 {
                return Err(fail(slash, ParseKind::NonCanonical));
            }
        }

        let signed_num = if negative {
            if num_mag == (1u128 << 63) {
                i64::MIN as i128
            } else {
                -(num_mag as i128)
            }
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

fn zero() -> Rational {
    Rational { num: 0, den: 1 }
}

fn one() -> Rational {
    Rational { num: 1, den: 1 }
}

fn arithmetic_error(row: usize, col: usize, step: usize, source: RationalError) -> MatrixError {
    MatrixError::Arithmetic {
        row,
        col,
        step,
        source,
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
        for row in 0..self.cols {
            for col in 0..self.rows {
                data.push(self.data[col * self.cols + row]);
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
        for row in 0..self.rows {
            for col in 0..rhs.cols {
                let mut acc = zero();
                for step in 0..self.cols {
                    let left = self.data[row * self.cols + step];
                    let right = rhs.data[step * rhs.cols + col];
                    let term = left
                        .mul(&right)
                        .map_err(|e| arithmetic_error(row, col, step, e))?;
                    acc = acc
                        .add(&term)
                        .map_err(|e| arithmetic_error(row, col, step, e))?;
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
        mut matrix: Matrix,
        pivot_cols: usize,
    ) -> Result<Reduction, MatrixError> {
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
                let key = (value.den as u64, value.num.unsigned_abs(), row);
                if chosen
                    .map(|old| {
                        let old_value = matrix.data[old * matrix.cols + col];
                        key < (old_value.den as u64, old_value.num.unsigned_abs(), old)
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
            for c in 0..matrix.cols {
                let index = p * matrix.cols + c;
                matrix.data[index] = matrix.data[index]
                    .div(&pivot)
                    .map_err(|e| arithmetic_error(p, c, p, e))?;
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
                    let pivot_entry = matrix.data[p * matrix.cols + c];
                    let product = factor
                        .mul(&pivot_entry)
                        .map_err(|e| arithmetic_error(row, c, p, e))?;
                    let index = row * matrix.cols + c;
                    matrix.data[index] = matrix.data[index]
                        .sub(&product)
                        .map_err(|e| arithmetic_error(row, c, p, e))?;
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
        Self::eliminate(self.clone(), self.cols)
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

        for row in 0..n {
            for col in 0..n {
                data.push(self.data[row * n + col]);
            }
            for col in 0..n {
                data.push(if row == col { one() } else { zero() });
            }
        }

        let augmented = Self {
            rows: n,
            cols: width,
            data,
        };
        let reduction = Self::eliminate(augmented, n)?;
        if reduction.pivots.len() < n {
            return Err(MatrixError::Singular {
                rank: reduction.pivots.len(),
            });
        }

        let mut inverse_data = Vec::with_capacity(n * n);
        for row in 0..n {
            for col in 0..n {
                inverse_data.push(reduction.matrix.data[row * width + n + col]);
            }
        }

        Ok(Self {
            rows: n,
            cols: n,
            data: inverse_data,
        })
    }

    pub fn solve(&self, rhs: &Self) -> Result<Solution, SolveError> {
        if self.rows != rhs.rows {
            return Err(SolveError::RowMismatch {
                coefficients: self.rows,
                rhs: rhs.rows,
            });
        }

        let width = self
            .cols
            .checked_add(rhs.cols)
            .ok_or(SolveError::AugmentedShapeOverflow)?;
        let len = self
            .rows
            .checked_mul(width)
            .ok_or(SolveError::AugmentedShapeOverflow)?;

        let mut data = Vec::with_capacity(len);
        for row in 0..self.rows {
            for col in 0..self.cols {
                data.push(self.data[row * self.cols + col]);
            }
            for col in 0..rhs.cols {
                data.push(rhs.data[row * rhs.cols + col]);
            }
        }

        let augmented = Self {
            rows: self.rows,
            cols: width,
            data,
        };
        let reduction = Self::eliminate(augmented, self.cols).map_err(SolveError::Matrix)?;
        let rank = reduction.pivots.len();

        for row in 0..self.rows {
            let coefficient_zero = (0..self.cols)
                .all(|col| reduction.matrix.data[row * width + col].num == 0);
            if !coefficient_zero {
                continue;
            }
            for rhs_col in 0..rhs.cols {
                let value = reduction.matrix.data[row * width + self.cols + rhs_col];
                if value.num != 0 {
                    let witness = reduction.matrix.data[row * width..(row + 1) * width].to_vec();
                    return Ok(Solution::None {
                        row,
                        rhs_col,
                        witness,
                    });
                }
            }
        }

        let particular_len = self
            .cols
            .checked_mul(rhs.cols)
            .ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut particular_data = vec![zero(); particular_len];
        for (pivot_row, &pivot_col) in reduction.pivots.iter().enumerate() {
            for rhs_col in 0..rhs.cols {
                particular_data[pivot_col * rhs.cols + rhs_col] =
                    reduction.matrix.data[pivot_row * width + self.cols + rhs_col];
            }
        }
        let particular = Self {
            rows: self.cols,
            cols: rhs.cols,
            data: particular_data,
        };

        let mut free = Vec::new();
        let mut pivot_index = 0usize;
        for col in 0..self.cols {
            if pivot_index < rank && reduction.pivots[pivot_index] == col {
                pivot_index += 1;
            } else {
                free.push(col);
            }
        }

        if rank == self.cols {
            return Ok(Solution::Unique {
                values: particular,
                pivots: reduction.pivots,
            });
        }

        let basis_len = self
            .cols
            .checked_mul(free.len())
            .ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut basis_data = vec![zero(); basis_len];

        for (basis_col, &free_col) in free.iter().enumerate() {
            basis_data[free_col * free.len() + basis_col] = one();

            for (pivot_row, &pivot_col) in reduction.pivots.iter().enumerate() {
                let coefficient = reduction.matrix.data[pivot_row * width + free_col];
                let negated = coefficient.neg().map_err(|source| {
                    SolveError::Matrix(MatrixError::Arithmetic {
                        row: pivot_col,
                        col: basis_col,
                        step: rank,
                        source,
                    })
                })?;
                basis_data[pivot_col * free.len() + basis_col] = negated;
            }
        }

        Ok(Solution::Infinite {
            particular,
            free,
            basis: Self {
                rows: self.cols,
                cols: basis_len.checked_div(self.cols.max(1)).unwrap_or(0),
                data: basis_data,
            },
            pivots: reduction.pivots,
        })
    }
}

fn main() {}