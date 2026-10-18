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

fn abs_i128(x: i128) -> u128 {
    if x < 0 {
        (-x) as u128
    } else {
        x as u128
    }
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
        let g = gcd_u128(abs_i128(n), d as u128) as i128;
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
        let n = self.num as i128 * rhs.den as i128
            + rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        Self::from_i128(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = self.num as i128 * rhs.den as i128
            - rhs.num as i128 * self.den as i128;
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
        let err = |at, kind| RationalError::Parse { at, kind };
        if bytes.is_empty() {
            return Err(err(0, ParseKind::Empty));
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
            return Err(err(i, ParseKind::InvalidByte));
        }

        let num_end = i;
        let slash = if i < bytes.len() && bytes[i] == b'/' {
            let s = i;
            i += 1;
            Some(s)
        } else {
            None
        };

        let den_start = if slash.is_some() { i } else { bytes.len() };
        if slash.is_some() {
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start {
                return Err(err(i, ParseKind::InvalidByte));
            }
        }
        if i < bytes.len() {
            return Err(err(i, ParseKind::InvalidByte));
        }

        let negative = bytes[0] == b'-';
        let num_digits = &bytes[num_start..num_end];
        let den_digits = if slash.is_some() {
            &bytes[den_start..]
        } else {
            &[][..]
        };

        if num_digits.len() >= 2 && num_digits[0] == b'0' {
            return Err(err(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_digits == b"0" {
            return Err(err(0, ParseKind::NonCanonical));
        }
        if slash.is_some() && den_digits.len() >= 2 && den_digits[0] == b'0' {
            return Err(err(den_start + 1, ParseKind::NonCanonical));
        }

        let limit: u128 = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let mut nm = 0u128;
        for &b in num_digits {
            nm = nm.saturating_mul(10).saturating_add((b - b'0') as u128);
            if nm > limit {
                return Err(err(0, ParseKind::OutOfRange));
            }
        }

        let mut dm = 1u128;
        if slash.is_some() {
            dm = 0;
            for &b in den_digits {
                dm = dm.saturating_mul(10).saturating_add((b - b'0') as u128);
                if dm > i64::MAX as u128 {
                    return Err(err(den_start, ParseKind::OutOfRange));
                }
            }
            if den_digits == b"0" {
                return Err(err(den_start, ParseKind::ZeroDenominator));
            }
            if dm == 1 {
                return Err(err(slash.unwrap(), ParseKind::NonCanonical));
            }
            if nm == 0 {
                return Err(err(slash.unwrap(), ParseKind::NonCanonical));
            }
            if gcd_u128(nm, dm) > 1 {
                return Err(err(slash.unwrap(), ParseKind::NonCanonical));
            }
        }

        let n = if negative {
            if nm == (1u128 << 63) {
                i64::MIN
            } else {
                -(nm as i64)
            }
        } else {
            nm as i64
        };
        Ok(Self {
            num: n,
            den: dm as i64,
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
        if self.data.is_empty() {
            return Self {
                rows: self.cols,
                cols: self.rows,
                data: Vec::new(),
            };
        }

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
        if len == 0 {
            return Ok(Self {
                rows: self.rows,
                cols: rhs.cols,
                data: Vec::new(),
            });
        }

        let mut data = Vec::with_capacity(len);
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = Rational { num: 0, den: 1 };
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
        update_cols: usize,
    ) -> Result<Reduction, MatrixError> {
        let mut matrix = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;

        for col in 0..candidate_cols {
            if p == matrix.rows {
                break;
            }
            let mut chosen: Option<usize> = None;
            for r in p..matrix.rows {
                let value = matrix.data[r * matrix.cols + col];
                if value.num == 0 {
                    continue;
                }
                let key = (value.den, abs_i128(value.num as i128), r);
                if chosen
                    .map(|q| {
                        let v = matrix.data[q * matrix.cols + col];
                        key < (v.den, abs_i128(v.num as i128), q)
                    })
                    .unwrap_or(true)
                {
                    chosen = Some(r);
                }
            }
            let Some(q) = chosen else {
                continue;
            };

            if q != p {
                for c in 0..update_cols {
                    matrix.data.swap(p * matrix.cols + c, q * matrix.cols + c);
                }
                swaps.push((p, q));
            }

            let pivot = matrix.data[p * matrix.cols + col];
            for c in 0..update_cols {
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
                let factor = matrix.data[r * matrix.cols + col];
                if factor.num == 0 {
                    continue;
                }
                for c in 0..update_cols {
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
        for r in 0..n {
            for c in 0..width {
                data.push(if c < n {
                    self.data[r * n + c]
                } else if c - n == r {
                    Rational { num: 1, den: 1 }
                } else {
                    Rational { num: 0, den: 1 }
                });
            }
        }
        let augmented = Self {
            rows: n,
            cols: width,
            data,
        };
        let reduced = augmented.eliminate(n, width)?;
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
        if len != 0 {
            for r in 0..self.rows {
                for c in 0..n {
                    data.push(self.data[r * n + c]);
                }
                for c in 0..k {
                    data.push(rhs.data[r * k + c]);
                }
            }
        }
        let augmented = Self {
            rows: self.rows,
            cols: width,
            data,
        };
        let reduced = augmented
            .eliminate(n, width)
            .map_err(SolveError::Matrix)?;

        if k != 0 {
            for r in 0..self.rows {
                if (0..n).all(|c| reduced.matrix.data[r * width + c].num == 0) {
                    if let Some(rhs_col) =
                        (0..k).find(|&c| reduced.matrix.data[r * width + n + c].num != 0)
                    {
                        let witness = reduced.matrix.data[r * width..(r + 1) * width].to_vec();
                        return Ok(Solution::None {
                            row: r,
                            rhs_col,
                            witness,
                        });
                    }
                }
            }
        }

        let rank = reduced.pivots.len();
        let particular_len = n.checked_mul(k).ok_or(SolveError::Matrix(
            MatrixError::ShapeOverflow,
        ))?;

        let free_count = n - rank;
        if rank < n {
            n.checked_mul(free_count).ok_or(SolveError::Matrix(
                MatrixError::ShapeOverflow,
            ))?;
        }

        let mut particular_data = vec![Rational { num: 0, den: 1 }; particular_len];
        for (j, &pivot_col) in reduced.pivots.iter().enumerate() {
            for c in 0..k {
                particular_data[pivot_col * k + c] =
                    reduced.matrix.data[j * width + n + c];
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

        let mut pivot_present = vec![false; n];
        for &c in &reduced.pivots {
            pivot_present[c] = true;
        }
        let mut free = Vec::with_capacity(free_count);
        for c in 0..n {
            if !pivot_present[c] {
                free.push(c);
            }
        }

        let f = free.len();
        let basis_len = n * f;
        let mut basis_data = vec![Rational { num: 0, den: 1 }; basis_len];
        for (j, &free_col) in free.iter().enumerate() {
            for (i, &pivot_col) in reduced.pivots.iter().enumerate() {
                let coefficient = reduced.matrix.data[i * width + free_col];
                basis_data[pivot_col * f + j] =
                    coefficient.neg().map_err(|source| {
                        SolveError::Matrix(MatrixError::Arithmetic {
                            row: pivot_col,
                            col: j,
                            step: rank,
                            source,
                        })
                    })?;
            }
            basis_data[free_col * f + j] = Rational { num: 1, den: 1 };
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