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
        if num == 0 {
            return Ok(Self { num: 0, den: 1 });
        }

        let mut n = num;
        let mut d = den;
        if d < 0 {
            n = n.checked_neg().ok_or(RationalError::Overflow)?;
            d = d.checked_neg().ok_or(RationalError::Overflow)?;
        }

        let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
        n /= g;
        d /= g;

        if n < i64::MIN as i128
            || n > i64::MAX as i128
            || d < 1
            || d > i64::MAX as i128
        {
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
        let a = self.num as i128;
        let b = rhs.num as i128;
        let da = self.den as i128;
        let db = rhs.den as i128;
        let g = gcd_u128(da as u128, db as u128) as i128;
        let left_scale = db / g;
        let right_scale = da / g;
        let n = a * left_scale + b * right_scale;
        let d = da * left_scale;
        Self::from_i128(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let a = self.num as i128;
        let b = rhs.num as i128;
        let da = self.den as i128;
        let db = rhs.den as i128;
        let g = gcd_u128(da as u128, db as u128) as i128;
        let left_scale = db / g;
        let right_scale = da / g;
        let n = a * left_scale - b * right_scale;
        let d = da * left_scale;
        Self::from_i128(n, d)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let mut a = self.num as i128;
        let mut b = rhs.num as i128;
        let mut da = self.den as i128;
        let mut db = rhs.den as i128;

        let g1 = gcd_u128(a.unsigned_abs(), db as u128) as i128;
        let g2 = gcd_u128(b.unsigned_abs(), da as u128) as i128;
        a /= g1;
        db /= g1;
        b /= g2;
        da /= g2;

        Self::from_i128(a * b, da * db)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }

        let mut a = self.num as i128;
        let mut b = rhs.den as i128;
        let mut da = self.den as i128;
        let mut db = rhs.num as i128;

        let g1 = gcd_u128(a.unsigned_abs(), db.unsigned_abs()) as i128;
        let g2 = gcd_u128(b as u128, da as u128) as i128;
        a /= g1;
        db /= g1;
        b /= g2;
        da /= g2;

        Self::from_i128(a * b, da * db)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        let n = (self.num as i128)
            .checked_neg()
            .ok_or(RationalError::Overflow)?;
        Self::from_i128(n, self.den as i128)
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

        let numerator_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == numerator_start {
            return Err(parse_err(i, ParseKind::InvalidByte));
        }

        let numerator_end = i;
        let mut denominator_start = None;
        let mut denominator_end = None;

        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            i += 1;
            denominator_start = Some(i);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == denominator_start.unwrap() {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            denominator_end = Some(i);
            if i < bytes.len() {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
        }

        if numerator_end - numerator_start >= 2 && bytes[numerator_start] == b'0' {
            return Err(parse_err(numerator_start + 1, ParseKind::NonCanonical));
        }
        if negative && numerator_end - numerator_start == 1 && bytes[numerator_start] == b'0' {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }

        if let (Some(ds), Some(de)) = (denominator_start, denominator_end) {
            if de - ds >= 2 && bytes[ds] == b'0' {
                return Err(parse_err(ds + 1, ParseKind::NonCanonical));
            }
        }

        let mut numerator_mag: u128 = 0;
        for &digit in &bytes[numerator_start..numerator_end] {
            numerator_mag = numerator_mag
                .saturating_mul(10)
                .saturating_add((digit - b'0') as u128);
        }
        let numerator_limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        if numerator_mag > numerator_limit {
            return Err(parse_err(0, ParseKind::OutOfRange));
        }

        let mut denominator_value: u128 = 1;
        if let (Some(ds), Some(de)) = (denominator_start, denominator_end) {
            denominator_value = 0;
            for &digit in &bytes[ds..de] {
                denominator_value = denominator_value
                    .saturating_mul(10)
                    .saturating_add((digit - b'0') as u128);
            }
            if denominator_value > i64::MAX as u128 {
                return Err(parse_err(ds, ParseKind::OutOfRange));
            }
            if denominator_value == 0 {
                return Err(parse_err(ds, ParseKind::ZeroDenominator));
            }
        }

        if let (Some(ds), Some(_)) = (denominator_start, denominator_end) {
            if denominator_value == 1 {
                return Err(parse_err(ds - 1, ParseKind::NonCanonical));
            }
            if numerator_mag == 0 {
                return Err(parse_err(ds - 1, ParseKind::NonCanonical));
            }
            if gcd_u128(numerator_mag, denominator_value) > 1 {
                return Err(parse_err(ds - 1, ParseKind::NonCanonical));
            }
        }

        let num = if negative {
            if numerator_mag == (1u128 << 63) {
                i64::MIN
            } else {
                -(numerator_mag as i64)
            }
        } else {
            numerator_mag as i64
        };

        if denominator_start.is_none() {
            Ok(Self { num, den: 1 })
        } else {
            Ok(Self {
                num,
                den: denominator_value as i64,
            })
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
        let len = self
            .rows
            .checked_mul(rhs.cols)
            .ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        let zero = Rational { num: 0, den: 1 };

        for row in 0..self.rows {
            for col in 0..rhs.cols {
                let mut acc = zero;
                for k in 0..self.cols {
                    let left = self.data[row * self.cols + k];
                    let right = rhs.data[k * rhs.cols + col];
                    let term = left.mul(&right).map_err(|source| MatrixError::Arithmetic {
                        row,
                        col,
                        step: k,
                        source,
                    })?;
                    acc = acc.add(&term).map_err(|source| MatrixError::Arithmetic {
                        row,
                        col,
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
        update_cols: usize,
    ) -> Result<Reduction, MatrixError> {
        let mut work = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0;

        for pivot_col in 0..candidate_cols {
            if p == work.rows {
                break;
            }

            let mut chosen: Option<usize> = None;
            for row in p..work.rows {
                let value = work.data[row * work.cols + pivot_col];
                if value.num == 0 {
                    continue;
                }
                let key = (value.den, value.num.unsigned_abs(), row);
                if chosen
                    .map(|current| {
                        let current_value = work.data[current * work.cols + pivot_col];
                        key < (
                            current_value.den,
                            current_value.num.unsigned_abs(),
                            current,
                        )
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
                for col in 0..update_cols {
                    work.data.swap(p * work.cols + col, q * work.cols + col);
                }
                swaps.push((p, q));
            }

            let pivot = work.data[p * work.cols + pivot_col];
            for col in 0..update_cols {
                let current = work.data[p * work.cols + col];
                work.data[p * work.cols + col] =
                    current
                        .div(&pivot)
                        .map_err(|source| MatrixError::Arithmetic {
                            row: p,
                            col,
                            step: p,
                            source,
                        })?;
            }

            for row in 0..work.rows {
                if row == p {
                    continue;
                }
                let factor = work.data[row * work.cols + pivot_col];
                if factor.num == 0 {
                    continue;
                }
                for col in 0..update_cols {
                    let product = factor
                        .mul(&work.data[p * work.cols + col])
                        .map_err(|source| MatrixError::Arithmetic {
                            row,
                            col,
                            step: p,
                            source,
                        })?;
                    let current = work.data[row * work.cols + col];
                    work.data[row * work.cols + col] =
                        current
                            .sub(&product)
                            .map_err(|source| MatrixError::Arithmetic {
                                row,
                                col,
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
        let mut data = Vec::with_capacity(len);
        let zero = Rational { num: 0, den: 1 };
        let one = Rational { num: 1, den: 1 };

        for row in 0..n {
            for col in 0..n {
                data.push(self.data[row * n + col]);
            }
            for col in 0..n {
                data.push(if row == col { one } else { zero });
            }
        }

        let augmented = Self {
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

        let mut inverse = Vec::with_capacity(n * n);
        for row in 0..n {
            for col in 0..n {
                inverse.push(reduction.matrix.data[row * width + n + col]);
            }
        }

        Ok(Self {
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

        let n = self.cols;
        let k = rhs.cols;
        let width = n.checked_add(k).ok_or(SolveError::AugmentedShapeOverflow)?;
        let len = self
            .rows
            .checked_mul(width)
            .ok_or(SolveError::AugmentedShapeOverflow)?;

        let mut data = Vec::with_capacity(len);
        for row in 0..self.rows {
            for col in 0..n {
                data.push(self.data[row * n + col]);
            }
            for col in 0..k {
                data.push(rhs.data[row * k + col]);
            }
        }

        let augmented = Self {
            rows: self.rows,
            cols: width,
            data,
        };
        let reduction = augmented
            .eliminate(n, width)
            .map_err(SolveError::Matrix)?;

        for row in 0..self.rows {
            let coeff_zero = (0..n).all(|col| {
                reduction.matrix.data[row * width + col].num == 0
            });
            if coeff_zero {
                for rhs_col in 0..k {
                    if reduction.matrix.data[row * width + n + rhs_col].num != 0 {
                        let witness = reduction.matrix.data
                            [row * width..row * width + width]
                            .to_vec();
                        return Ok(Solution::None {
                            row,
                            rhs_col,
                            witness,
                        });
                    }
                }
            }
        }

        let pivots = reduction.pivots;
        let mut free = Vec::new();
        let mut pivot_present = vec![false; n];
        for &pivot in &pivots {
            if pivot < n {
                pivot_present[pivot] = true;
            }
        }
        for col in 0..n {
            if !pivot_present[col] {
                free.push(col);
            }
        }

        let particular_len = n.checked_mul(k).ok_or_else(|| {
            SolveError::Matrix(MatrixError::ShapeOverflow)
        })?;
        let zero = Rational { num: 0, den: 1 };
        let mut particular = vec![zero; particular_len];
        for (pivot_row, &pivot_col) in pivots.iter().enumerate() {
            for rhs_col in 0..k {
                particular[pivot_col * k + rhs_col] =
                    reduction.matrix.data[pivot_row * width + n + rhs_col];
            }
        }
        let particular = Self {
            rows: n,
            cols: k,
            data: particular,
        };

        if pivots.len() == n {
            return Ok(Solution::Unique {
                values: particular,
                pivots,
            });
        }

        let basis_len = n.checked_mul(free.len()).ok_or_else(|| {
            SolveError::Matrix(MatrixError::ShapeOverflow)
        })?;
        let mut basis = vec![zero; basis_len];
        let one = Rational { num: 1, den: 1 };

        for (basis_col, &free_col) in free.iter().enumerate() {
            basis[free_col * free.len() + basis_col] = one;
            for (pivot_row, &pivot_col) in pivots.iter().enumerate() {
                let coefficient =
                    reduction.matrix.data[pivot_row * width + free_col];
                basis[pivot_col * free.len() + basis_col] =
                    coefficient.neg().map_err(|source| {
                        SolveError::Matrix(MatrixError::Arithmetic {
                            row: pivot_col,
                            col: basis_col,
                            step: pivots.len(),
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
                cols: basis_len.checked_div(n.max(1)).unwrap_or(0),
                data: basis,
            },
            pivots,
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