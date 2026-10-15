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
        let a = self.num as i128 * rhs.den as i128;
        let b = rhs.num as i128 * self.den as i128;
        Self::from_i128(a + b, self.den as i128 * rhs.den as i128)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let a = self.num as i128 * rhs.den as i128;
        let b = rhs.num as i128 * self.den as i128;
        Self::from_i128(a - b, self.den as i128 * rhs.den as i128)
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
        if bytes[0] == b'-' {
            i = 1;
        }

        let nstart = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == nstart {
            return Err(fail(i, ParseKind::InvalidByte));
        }
        let nend = i;

        let mut dstart = None;
        let mut dend = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(fail(i, ParseKind::InvalidByte));
            }
            i += 1;
            let ds = i;
            dstart = Some(ds);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == ds {
                return Err(fail(i, ParseKind::InvalidByte));
            }
            dend = Some(i);
            if i < bytes.len() {
                return Err(fail(i, ParseKind::InvalidByte));
            }
        }

        if nend - nstart >= 2 && bytes[nstart] == b'0' {
            return Err(fail(nstart + 1, ParseKind::NonCanonical));
        }
        if bytes[0] == b'-' && nend - nstart == 1 && bytes[nstart] == b'0' {
            return Err(fail(0, ParseKind::NonCanonical));
        }
        if let (Some(ds), Some(de)) = (dstart, dend) {
            if de - ds >= 2 && bytes[ds] == b'0' {
                return Err(fail(ds + 1, ParseKind::NonCanonical));
            }
        }

        let negative = bytes[0] == b'-';
        let mut mag = 0u128;
        for &b in &bytes[nstart..nend] {
            mag = mag
                .saturating_mul(10)
                .saturating_add((b - b'0') as u128);
        }
        let maxmag = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        if mag > maxmag {
            return Err(fail(0, ParseKind::OutOfRange));
        }

        let mut den = 1u128;
        if let (Some(ds), Some(de)) = (dstart, dend) {
            den = 0;
            for &b in &bytes[ds..de] {
                den = den
                    .saturating_mul(10)
                    .saturating_add((b - b'0') as u128);
            }
            if den > i64::MAX as u128 {
                return Err(fail(ds, ParseKind::OutOfRange));
            }
            if den == 0 {
                return Err(fail(ds, ParseKind::ZeroDenominator));
            }
            if den == 1 {
                return Err(fail(ds - 1, ParseKind::NonCanonical));
            }
            if mag == 0 {
                return Err(fail(ds - 1, ParseKind::NonCanonical));
            }
            if gcd_u128(mag, den) > 1 {
                return Err(fail(ds - 1, ParseKind::NonCanonical));
            }
        }

        let n = if negative {
            -(mag as i128)
        } else {
            mag as i128
        };
        Self::from_i128(n, den as i128)
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
        if expected != data.len() {
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
                    let term = self.data[r * self.cols + k]
                        .mul(&rhs.data[k * rhs.cols + c])
                        .map_err(|source| MatrixError::Arithmetic {
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
        mut matrix: Matrix,
        candidate_cols: usize,
    ) -> Result<Reduction, MatrixError> {
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;

        for col in 0..candidate_cols {
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
                let replace = match chosen {
                    None => true,
                    Some(old_row) => {
                        let old = matrix.data[old_row * matrix.cols + col];
                        key < (old.den, old.num.unsigned_abs(), old_row)
                    }
                };
                if replace {
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
                matrix.data[idx] = matrix.data[idx].div(&pivot).map_err(|source| {
                    MatrixError::Arithmetic {
                        row: p,
                        col: c,
                        step: p,
                        source,
                    }
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
                    matrix.data[idx] =
                        matrix.data[idx]
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

        for r in 0..n {
            for c in 0..n {
                data.push(self.data[r * n + c]);
            }
            for c in 0..n {
                data.push(Rational {
                    num: if r == c { 1 } else { 0 },
                    den: 1,
                });
            }
        }

        let augmented = Matrix {
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

        let mut inverse = Vec::with_capacity(len / 2);
        for r in 0..n {
            for c in 0..n {
                inverse.push(reduction.matrix.data[r * width + n + c]);
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

        let width = self
            .cols
            .checked_add(rhs.cols)
            .ok_or(SolveError::AugmentedShapeOverflow)?;
        let len = self
            .rows
            .checked_mul(width)
            .ok_or(SolveError::AugmentedShapeOverflow)?;

        let mut data = Vec::with_capacity(len);
        for r in 0..self.rows {
            let a_start = r * self.cols;
            let b_start = r * rhs.cols;
            data.extend_from_slice(&self.data[a_start..a_start + self.cols]);
            data.extend_from_slice(&rhs.data[b_start..b_start + rhs.cols]);
        }

        let augmented = Matrix {
            rows: self.rows,
            cols: width,
            data,
        };
        let reduction =
            Self::eliminate(augmented, self.cols).map_err(SolveError::Matrix)?;

        for row in 0..self.rows {
            let base = row * width;
            let coefficient_zero = reduction.matrix.data[base..base + self.cols]
                .iter()
                .all(|x| x.num == 0);
            if coefficient_zero {
                if let Some(rhs_col) = (0..rhs.cols)
                    .find(|&c| reduction.matrix.data[base + self.cols + c].num != 0)
                {
                    let witness = reduction.matrix.data[base..base + width].to_vec();
                    return Ok(Solution::None {
                        row,
                        rhs_col,
                        witness,
                    });
                }
            }
        }

        let rank = reduction.pivots.len();
        let mut is_pivot = vec![false; self.cols];
        for &p in &reduction.pivots {
            is_pivot[p] = true;
        }
        let free: Vec<usize> = (0..self.cols).filter(|&c| !is_pivot[c]).collect();

        let particular_len = self
            .cols
            .checked_mul(rhs.cols)
            .ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut particular_data = vec![
            Rational {
                num: 0,
                den: 1,
            };
            particular_len
        ];

        for (pivot_row, &pivot_col) in reduction.pivots.iter().enumerate() {
            for c in 0..rhs.cols {
                particular_data[pivot_col * rhs.cols + c] =
                    reduction.matrix.data[pivot_row * width + self.cols + c];
            }
        }

        let particular = Matrix {
            rows: self.cols,
            cols: rhs.cols,
            data: particular_data,
        };

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
        let mut basis_data = vec![
            Rational {
                num: 0,
                den: 1,
            };
            basis_len
        ];

        for (basis_col, &free_col) in free.iter().enumerate() {
            basis_data[free_col * free.len() + basis_col] = Rational { num: 1, den: 1 };

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
            basis: Matrix {
                rows: self.cols,
                cols: basis_data.len().checked_div(self.cols).unwrap_or(0),
                data: basis_data,
            },
            pivots: reduction.pivots,
        })
    }
}

fn main() {}