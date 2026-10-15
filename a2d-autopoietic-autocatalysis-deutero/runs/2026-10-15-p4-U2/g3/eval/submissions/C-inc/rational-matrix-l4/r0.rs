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

fn abs_u128(n: i64) -> u128 {
    if n < 0 {
        (-(n as i128)) as u128
    } else {
        n as u128
    }
}

impl Rational {
    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        if num == 0 {
            return Ok(Self { num: 0, den: 1 });
        }
        let mut n = num as i128;
        let mut d = den as i128;
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

    fn from_i128(n: i128, d: i128) -> Result<Self, RationalError> {
        if d == 0 {
            return Err(RationalError::DivideByZero);
        }
        if n == 0 {
            return Ok(Self { num: 0, den: 1 });
        }
        let (mut n, mut d) = (n, d);
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
        .map_err(|_| RationalError::Overflow)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128(
            self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
        .map_err(|_| RationalError::Overflow)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
        .map_err(|_| RationalError::Overflow)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        Self::from_i128(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
        .map_err(|_| RationalError::Overflow)
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
        let b = text.as_bytes();
        if b.is_empty() {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::Empty,
            });
        }

        let mut slash = None;
        for (i, &c) in b.iter().enumerate() {
            if c == b'/' {
                if slash.is_some() {
                    return Err(parse_err(i, ParseKind::InvalidByte));
                }
                slash = Some(i);
            } else if c.is_ascii_digit() {
                continue;
            } else if c == b'-' && i == 0 {
                continue;
            } else {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
        }

        let split = slash.unwrap_or(b.len());
        let num_start = usize::from(b[0] == b'-');
        if split == num_start {
            return Err(parse_err(split, ParseKind::InvalidByte));
        }
        if let Some(s) = slash {
            if s + 1 == b.len() {
                return Err(parse_err(b.len(), ParseKind::InvalidByte));
            }
        }

        let num_digits = &b[num_start..split];
        let den_digits = slash.map(|s| &b[s + 1..]);

        if num_digits.len() >= 2 && num_digits[0] == b'0' {
            return Err(parse_err(num_start + 1, ParseKind::NonCanonical));
        }
        if b[0] == b'-' && num_digits == b"0" {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }
        if let Some(ds) = den_digits {
            if ds.len() >= 2 && ds[0] == b'0' {
                return Err(parse_err(split + 2, ParseKind::NonCanonical));
            }
        }

        let negative = b[0] == b'-';
        let nmag = parse_magnitude(num_digits);
        let nlimit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        if nmag > nlimit {
            return Err(parse_err(0, ParseKind::OutOfRange));
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

        let Some(ds) = den_digits else {
            return Ok(Self { num: n, den: 1 });
        };
        let dmag = parse_magnitude(ds);
        if dmag > i64::MAX as u128 {
            return Err(parse_err(split + 1, ParseKind::OutOfRange));
        }
        if dmag == 0 {
            return Err(parse_err(split + 1, ParseKind::ZeroDenominator));
        }
        if dmag == 1 {
            return Err(parse_err(split, ParseKind::NonCanonical));
        }
        if n == 0 {
            return Err(parse_err(split, ParseKind::NonCanonical));
        }
        if gcd_u128(abs_u128(n), dmag) > 1 {
            return Err(parse_err(split, ParseKind::NonCanonical));
        }
        Ok(Self {
            num: n,
            den: dmag as i64,
        })
    }
}

fn parse_err(at: usize, kind: ParseKind) -> RationalError {
    RationalError::Parse { at, kind }
}

fn parse_magnitude(digits: &[u8]) -> u128 {
    let mut n = 0u128;
    for &c in digits {
        n = n.saturating_mul(10).saturating_add((c - b'0') as u128);
    }
    n
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

fn checked_len(rows: usize, cols: usize) -> Result<usize, MatrixError> {
    rows.checked_mul(cols).ok_or(MatrixError::ShapeOverflow)
}

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<Rational>) -> Result<Self, MatrixError> {
        let expected = checked_len(rows, cols)?;
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
        let len = checked_len(self.rows, rhs.cols)?;
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
    ) -> Result<Reduction, MatrixError> {
        let mut work = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;

        for col in 0..candidate_cols {
            if p == work.rows {
                break;
            }
            let mut best: Option<usize> = None;
            for r in p..work.rows {
                let value = work.data[r * work.cols + col];
                if value.num == 0 {
                    continue;
                }
                let key = (value.den, abs_u128(value.num), r);
                if best.map_or(true, |br| {
                    let bv = work.data[br * work.cols + col];
                    key < (bv.den, abs_u128(bv.num), br)
                }) {
                    best = Some(r);
                }
            }
            let Some(q) = best else {
                continue;
            };
            if q != p {
                for c in 0..work.cols {
                    work.data.swap(p * work.cols + c, q * work.cols + c);
                }
                swaps.push((p, q));
            }
            let pivot = work.data[p * work.cols + col];
            for c in 0..work.cols {
                let idx = p * work.cols + c;
                work.data[idx] = work.data[idx]
                    .div(&pivot)
                    .map_err(|source| MatrixError::Arithmetic {
                        row: p,
                        col: c,
                        step: p,
                        source,
                    })?;
            }
            for r in 0..work.rows {
                if r == p {
                    continue;
                }
                let factor = work.data[r * work.cols + col];
                if factor.num == 0 {
                    continue;
                }
                for c in 0..work.cols {
                    let pivot_entry = work.data[p * work.cols + c];
                    let product = factor.mul(&pivot_entry).map_err(|source| {
                        MatrixError::Arithmetic {
                            row: r,
                            col: c,
                            step: p,
                            source,
                        }
                    })?;
                    let idx = r * work.cols + c;
                    work.data[idx] = work.data[idx].sub(&product).map_err(|source| {
                        MatrixError::Arithmetic {
                            row: r,
                            col: c,
                            step: p,
                            source,
                        }
                    })?;
                }
            }
            pivots.push(col);
            p += 1;
        }
        Ok(Reduction {
            matrix: work,
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
        let len = checked_len(n, width)?;
        let mut data = Vec::with_capacity(len);
        for r in 0..n {
            for c in 0..n {
                data.push(self.data[r * n + c]);
            }
            for c in 0..n {
                data.push(if c == r {
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
        let reduction = augmented.eliminate(n)?;
        let rank = reduction.pivots.len();
        if rank < n {
            return Err(MatrixError::Singular { rank });
        }
        let mut result = Vec::with_capacity(n * n);
        for r in 0..n {
            for c in 0..n {
                result.push(reduction.matrix.data[r * width + n + c]);
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
        let m = self.rows;
        let n = self.cols;
        let k = rhs.cols;
        let width = n.checked_add(k).ok_or(SolveError::AugmentedShapeOverflow)?;
        let len = m
            .checked_mul(width)
            .ok_or(SolveError::AugmentedShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        for r in 0..m {
            for c in 0..n {
                data.push(self.data[r * n + c]);
            }
            for c in 0..k {
                data.push(rhs.data[r * k + c]);
            }
        }
        let augmented = Self {
            rows: m,
            cols: width,
            data,
        };
        let reduction = augmented
            .eliminate(n)
            .map_err(SolveError::Matrix)?;
        let rank = reduction.pivots.len();

        for r in 0..m {
            let coefficients_zero = (0..n)
                .all(|c| reduction.matrix.data[r * width + c].num == 0);
            if coefficients_zero {
                if let Some(rhs_col) = (0..k)
                    .find(|&c| reduction.matrix.data[r * width + n + c].num != 0)
                {
                    let witness = reduction.matrix.data[r * width..(r + 1) * width].to_vec();
                    return Ok(Solution::None {
                        row: r,
                        rhs_col,
                        witness,
                    });
                }
            }
        }

        let particular_len = n
            .checked_mul(k)
            .ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut particular_data = vec![Rational { num: 0, den: 1 }; particular_len];
        for (pivot_row, &pivot_col) in reduction.pivots.iter().enumerate() {
            for c in 0..k {
                particular_data[pivot_col * k + c] =
                    reduction.matrix.data[pivot_row * width + n + c];
            }
        }
        let particular = Self {
            rows: n,
            cols: k,
            data: particular_data,
        };

        let mut free = Vec::new();
        let mut is_pivot = vec![false; n];
        for &c in &reduction.pivots {
            is_pivot[c] = true;
        }
        for c in 0..n {
            if !is_pivot[c] {
                free.push(c);
            }
        }

        if rank == n {
            return Ok(Solution::Unique {
                values: particular,
                pivots: reduction.pivots,
            });
        }

        let basis_len = n
            .checked_mul(free.len())
            .ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut basis_data = vec![Rational { num: 0, den: 1 }; basis_len];
        for (j, &free_col) in free.iter().enumerate() {
            basis_data[free_col * free.len() + j] = Rational { num: 1, den: 1 };
            for (i, &pivot_col) in reduction.pivots.iter().enumerate() {
                let coefficient = reduction.matrix.data[i * width + free_col];
                let negated = coefficient.neg().map_err(|source| {
                    SolveError::Matrix(MatrixError::Arithmetic {
                        row: pivot_col,
                        col: j,
                        step: rank,
                        source,
                    })
                })?;
                basis_data[pivot_col * free.len() + j] = negated;
            }
        }
        Ok(Solution::Infinite {
            particular,
            free,
            basis: Self {
                rows: n,
                cols: is_pivot.len() - rank,
                data: basis_data,
            },
            pivots: reduction.pivots,
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

fn main() {}