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

fn abs_i64_u128(value: i64) -> u128 {
    value.unsigned_abs() as u128
}

impl Rational {
    fn from_i128(num: i128, den: i128) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        if num == 0 {
            return Ok(Self { num: 0, den: 1 });
        }

        let (mut n, mut d) = (num, den);
        if d < 0 {
            n = -n;
            d = -d;
        }
        let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
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
        let a = self.num as i128;
        let b = self.den as i128;
        let c = rhs.num as i128;
        let d = rhs.den as i128;
        let g = gcd_u128(b as u128, d as u128) as i128;
        Self::from_i128(a * (d / g) + c * (b / g), (b / g) * d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let a = self.num as i128;
        let b = self.den as i128;
        let c = rhs.num as i128;
        let d = rhs.den as i128;
        let g = gcd_u128(b as u128, d as u128) as i128;
        Self::from_i128(a * (d / g) - c * (b / g), (b / g) * d)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let a = self.num as i128;
        let b = self.den as i128;
        let c = rhs.num as i128;
        let d = rhs.den as i128;
        let g1 = gcd_u128(a.unsigned_abs(), d as u128) as i128;
        let g2 = gcd_u128(c.unsigned_abs(), b as u128) as i128;
        Self::from_i128((a / g1) * (c / g2), (b / g2) * (d / g1))
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        let a = self.num as i128;
        let b = self.den as i128;
        let c = rhs.num as i128;
        let d = rhs.den as i128;
        let g1 = gcd_u128(a.unsigned_abs(), c.unsigned_abs()) as i128;
        let g2 = gcd_u128(d as u128, b as u128) as i128;
        Self::from_i128((a / g1) * (d / g2), (b / g2) * (c / g1))
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

        let negative = bytes[0] == b'-';
        let num_start = usize::from(negative);
        let mut i = num_start;

        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return Err(parse_err(i, ParseKind::InvalidByte));
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
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
        }
        if i < bytes.len() {
            return Err(parse_err(i, ParseKind::InvalidByte));
        }

        let num_len = num_end - num_start;
        if num_len >= 2 && bytes[num_start] == b'0' {
            return Err(parse_err(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_len == 1 && bytes[num_start] == b'0' {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }
        if let (Some(ds), Some(de)) = (den_start, den_end) {
            if de - ds >= 2 && bytes[ds] == b'0' {
                return Err(parse_err(ds + 1, ParseKind::NonCanonical));
            }
        }

        let mut num_mag = 0u128;
        for &b in &bytes[num_start..num_end] {
            num_mag = num_mag
                .saturating_mul(10)
                .saturating_add((b - b'0') as u128);
        }
        let num_limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        if num_mag > num_limit {
            return Err(parse_err(0, ParseKind::OutOfRange));
        }

        let mut den_value = 1u128;
        if let (Some(ds), Some(de)) = (den_start, den_end) {
            den_value = 0;
            for &b in &bytes[ds..de] {
                den_value = den_value
                    .saturating_mul(10)
                    .saturating_add((b - b'0') as u128);
            }
            if den_value > i64::MAX as u128 {
                return Err(parse_err(ds, ParseKind::OutOfRange));
            }
            if den_value == 0 {
                return Err(parse_err(ds, ParseKind::ZeroDenominator));
            }
            if den_value == 1 {
                return Err(parse_err(ds - 1, ParseKind::NonCanonical));
            }
            if num_mag == 0 {
                return Err(parse_err(ds - 1, ParseKind::NonCanonical));
            }
            if gcd_u128(num_mag, den_value) > 1 {
                return Err(parse_err(ds - 1, ParseKind::NonCanonical));
            }
        }

        let signed_num = if negative {
            -(num_mag as i128)
        } else {
            num_mag as i128
        };
        Self::from_i128(signed_num, den_value as i128)
    }
}

impl std::fmt::Display for Rational {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
        for col in 0..self.cols {
            for row in 0..self.rows {
                data.push(self.data[row * self.cols + col]);
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
        let count = self
            .rows
            .checked_mul(rhs.cols)
            .ok_or(MatrixError::ShapeOverflow)?;
        let mut out = Vec::with_capacity(count);
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
                out.push(acc);
            }
        }
        Ok(Self {
            rows: self.rows,
            cols: rhs.cols,
            data: out,
        })
    }

    fn eliminate(
        matrix: &mut Self,
        candidates: usize,
    ) -> Result<(Vec<usize>, Vec<(usize, usize)>), MatrixError> {
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;
        let limit = candidates.min(matrix.cols);

        for col in 0..limit {
            if p == matrix.rows {
                break;
            }
            let mut chosen: Option<usize> = None;
            for row in p..matrix.rows {
                let value = matrix.data[row * matrix.cols + col];
                if value.num == 0 {
                    continue;
                }
                let key = (value.den, abs_i64_u128(value.num), row);
                if chosen
                    .map(|old| {
                        let v = matrix.data[old * matrix.cols + col];
                        key < (v.den, abs_i64_u128(v.num), old)
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
                    let idx = row * matrix.cols + c;
                    matrix.data[idx] = matrix.data[idx]
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
        Ok((pivots, swaps))
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        let mut matrix = self.clone();
        let (pivots, swaps) = Self::eliminate(&mut matrix, self.cols)?;
        Ok(Reduction {
            matrix,
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

        let mut augmented = Self {
            rows: n,
            cols: width,
            data,
        };
        let (pivots, _) = Self::eliminate(&mut augmented, n)?;
        if pivots.len() < n {
            return Err(MatrixError::Singular { rank: pivots.len() });
        }

        let mut out = Vec::with_capacity(len / 2);
        for row in 0..n {
            out.extend_from_slice(&augmented.data[row * width + n..row * width + width]);
        }
        Ok(Self {
            rows: n,
            cols: n,
            data: out,
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
        let width = n.checked_add(k).ok_or(SolveError::AugmentedShapeOverflow)?;
        let len = m
            .checked_mul(width)
            .ok_or(SolveError::AugmentedShapeOverflow)?;

        let mut data = Vec::with_capacity(len);
        for row in 0..m {
            data.extend_from_slice(&self.data[row * n..row * n + n]);
            data.extend_from_slice(&rhs.data[row * k..row * k + k]);
        }
        let mut augmented = Self {
            rows: m,
            cols: width,
            data,
        };
        let (pivots, _) =
            Self::eliminate(&mut augmented, n).map_err(SolveError::Matrix)?;

        for row in 0..m {
            let coefficients_zero =
                (0..n).all(|col| augmented.data[row * width + col].num == 0);
            if coefficients_zero {
                for rhs_col in 0..k {
                    if augmented.data[row * width + n + rhs_col].num != 0 {
                        return Ok(Solution::None {
                            row,
                            rhs_col,
                            witness: augmented.data[row * width..row * width + width].to_vec(),
                        });
                    }
                }
            }
        }

        let rank = pivots.len();
        let mut pivot_flags = vec![false; n];
        for &col in &pivots {
            pivot_flags[col] = true;
        }
        let mut free = Vec::new();
        for col in 0..n {
            if !pivot_flags[col] {
                free.push(col);
            }
        }

        let particular_len = n
            .checked_mul(k)
            .ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let zero = Rational { num: 0, den: 1 };
        let mut particular_data = vec![zero; particular_len];
        for (pivot_row, &pivot_col) in pivots.iter().enumerate() {
            for rhs_col in 0..k {
                particular_data[pivot_col * k + rhs_col] =
                    augmented.data[pivot_row * width + n + rhs_col];
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
                pivots,
            });
        }

        let basis_len = n
            .checked_mul(free.len())
            .ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut basis_data = vec![zero; basis_len];
        let one = Rational { num: 1, den: 1 };

        for (basis_col, &free_col) in free.iter().enumerate() {
            basis_data[free_col * free.len() + basis_col] = one;
            for (pivot_row, &pivot_col) in pivots.iter().enumerate() {
                let coefficient = augmented.data[pivot_row * width + free_col];
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
            free: free.clone(),
            basis: Self {
                rows: n,
                cols: free.len(),
                data: basis_data,
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
    RowMismatch { coefficients: usize, rhs: usize },
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