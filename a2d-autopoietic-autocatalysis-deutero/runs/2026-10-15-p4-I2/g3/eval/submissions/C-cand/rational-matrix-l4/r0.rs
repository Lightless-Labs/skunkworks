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

    let mut n = num;
    let mut d = den;
    if d < 0 {
        n = -n;
        d = -d;
    }

    if n == 0 {
        return Ok(Rational { num: 0, den: 1 });
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
        let fail = |at, kind| RationalError::Parse { at, kind };

        if bytes.is_empty() {
            return Err(fail(0, ParseKind::Empty));
        }

        let mut i = 0;
        let negative = bytes[0] == b'-';
        if negative {
            i += 1;
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

        let numerator_token = &bytes[..num_end];
        let numerator_digits = &bytes[num_start..num_end];
        if numerator_digits.len() >= 2 && numerator_digits[0] == b'0' {
            return Err(fail(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && numerator_digits == b"0" {
            return Err(fail(0, ParseKind::NonCanonical));
        }

        if let (Some(start), Some(end)) = (den_start, den_end) {
            if end - start >= 2 && bytes[start] == b'0' {
                return Err(fail(start + 1, ParseKind::NonCanonical));
            }
        }

        let mut nmag: u128 = 0;
        let nlimit = if negative { 1u128 << 63 } else { i64::MAX as u128 };
        for &b in numerator_digits {
            nmag = nmag
                .saturating_mul(10)
                .saturating_add((b - b'0') as u128);
            if nmag > nlimit {
                return Err(fail(0, ParseKind::OutOfRange));
            }
        }

        let mut dmag: u128 = 1;
        if let (Some(start), Some(end)) = (den_start, den_end) {
            dmag = 0;
            for &b in &bytes[start..end] {
                dmag = dmag
                    .saturating_mul(10)
                    .saturating_add((b - b'0') as u128);
                if dmag > i64::MAX as u128 {
                    return Err(fail(start, ParseKind::OutOfRange));
                }
            }
            if dmag == 0 {
                return Err(fail(start, ParseKind::ZeroDenominator));
            }
            if dmag == 1 {
                return Err(fail(start - 1, ParseKind::NonCanonical));
            }
            if nmag == 0 {
                return Err(fail(start - 1, ParseKind::NonCanonical));
            }
            if gcd_u128(nmag, dmag) > 1 {
                return Err(fail(start - 1, ParseKind::NonCanonical));
            }
        }

        let _ = numerator_token;
        let n = if negative {
            if nmag == 1u128 << 63 {
                i64::MIN
            } else {
                -(nmag as i64)
            }
        } else {
            nmag as i64
        };
        Ok(Rational {
            num: n,
            den: dmag as i64,
        })
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

fn zero() -> Rational {
    Rational { num: 0, den: 1 }
}

fn one() -> Rational {
    Rational { num: 1, den: 1 }
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
        let index = row
            .checked_mul(self.cols)
            .and_then(|x| x.checked_add(col))
            .ok_or(MatrixError::Index { row, col })?;
        self.data
            .get(index)
            .copied()
            .ok_or(MatrixError::Index { row, col })
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
        let mut data = Vec::with_capacity(len);
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = zero();
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

    fn eliminate(&self, candidate_cols: usize) -> Result<Reduction, MatrixError> {
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
                let key = (value.den, value.num.unsigned_abs(), r);
                if chosen.map_or(true, |q| {
                    let old = matrix.data[q * matrix.cols + col];
                    key < (old.den, old.num.unsigned_abs(), q)
                }) {
                    chosen = Some(r);
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
                let value = matrix.data[p * matrix.cols + c];
                normalized.push(value.div(&pivot).map_err(|source| {
                    MatrixError::Arithmetic {
                        row: p,
                        col: c,
                        step: p,
                        source,
                    }
                })?);
            }
            for (c, value) in normalized.into_iter().enumerate() {
                matrix.data[p * matrix.cols + c] = value;
            }

            for r in 0..matrix.rows {
                if r == p {
                    continue;
                }
                let factor = matrix.data[r * matrix.cols + col];
                if factor.num == 0 {
                    continue;
                }
                for c in 0..matrix.cols {
                    let product = factor
                        .mul(&matrix.data[p * matrix.cols + c])
                        .map_err(|source| MatrixError::Arithmetic {
                            row: r,
                            col: c,
                            step: p,
                            source,
                        })?;
                    let updated = matrix.data[r * matrix.cols + c]
                        .sub(&product)
                        .map_err(|source| MatrixError::Arithmetic {
                            row: r,
                            col: c,
                            step: p,
                            source,
                        })?;
                    matrix.data[r * matrix.cols + c] = updated;
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
        for r in 0..n {
            for c in 0..n {
                data.push(self.data[r * n + c]);
            }
            for c in 0..n {
                data.push(if r == c { one() } else { zero() });
            }
        }
        let augmented = Self {
            rows: n,
            cols: width,
            data,
        };
        let reduction = augmented.eliminate(n)?;
        if reduction.pivots.len() < n {
            return Err(MatrixError::Singular {
                rank: reduction.pivots.len(),
            });
        }

        let out_len = n.checked_mul(n).ok_or(MatrixError::ShapeOverflow)?;
        let mut out = Vec::with_capacity(out_len);
        for r in 0..n {
            for c in 0..n {
                out.push(reduction.matrix.data[r * width + n + c]);
            }
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

        let n = self.cols;
        let k = rhs.cols;
        let width = n.checked_add(k).ok_or(SolveError::AugmentedShapeOverflow)?;
        let len = self
            .rows
            .checked_mul(width)
            .ok_or(SolveError::AugmentedShapeOverflow)?;

        let mut data = Vec::with_capacity(len);
        for r in 0..self.rows {
            for c in 0..n {
                data.push(self.data[r * n + c]);
            }
            for c in 0..k {
                data.push(rhs.data[r * k + c]);
            }
        }

        let augmented = Self {
            rows: self.rows,
            cols: width,
            data,
        };
        let reduction = augmented
            .eliminate(n)
            .map_err(SolveError::Matrix)?;

        for r in 0..self.rows {
            let coeff_zero = (0..n).all(|c| reduction.matrix.data[r * width + c].num == 0);
            if coeff_zero {
                if let Some(rhs_col) =
                    (0..k).find(|&c| reduction.matrix.data[r * width + n + c].num != 0)
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

        let pivots = reduction.pivots;
        let rank = pivots.len();
        let mut is_pivot = vec![false; n];
        for &c in &pivots {
            is_pivot[c] = true;
        }
        let free: Vec<usize> = (0..n).filter(|&c| !is_pivot[c]).collect();

        let particular_len = n.checked_mul(k).ok_or_else(|| {
            SolveError::Matrix(MatrixError::ShapeOverflow)
        })?;
        let mut particular = vec![zero(); particular_len];
        for (j, &pivot_col) in pivots.iter().enumerate() {
            for c in 0..k {
                particular[pivot_col * k + c] =
                    reduction.matrix.data[j * width + n + c];
            }
        }
        let particular_matrix = Self {
            rows: n,
            cols: k,
            data: particular,
        };

        if rank == n {
            return Ok(Solution::Unique {
                values: particular_matrix,
                pivots,
            });
        }

        let f = free.len();
        let basis_len = n
            .checked_mul(f)
            .ok_or_else(|| SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut basis = vec![zero(); basis_len];
        for (j, &free_col) in free.iter().enumerate() {
            basis[free_col * f + j] = one();
            for (pivot_row, &pivot_col) in pivots.iter().enumerate() {
                let coefficient = reduction.matrix.data[pivot_row * width + free_col];
                let negative = coefficient.neg().map_err(|source| {
                    SolveError::Matrix(MatrixError::Arithmetic {
                        row: pivot_col,
                        col: j,
                        step: rank,
                        source,
                    })
                })?;
                basis[pivot_col * f + j] = negative;
            }
        }

        Ok(Solution::Infinite {
            particular: particular_matrix,
            free,
            basis: Self {
                rows: n,
                cols: f,
                data: basis,
            },
            pivots,
        })
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