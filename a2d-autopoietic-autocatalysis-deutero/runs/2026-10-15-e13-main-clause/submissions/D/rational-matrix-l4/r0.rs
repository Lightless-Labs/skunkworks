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
        n = n.checked_neg().ok_or(RationalError::Overflow)?;
        d = d.checked_neg().ok_or(RationalError::Overflow)?;
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
        let n = (self.num as i128) * (rhs.den as i128)
            + (rhs.num as i128) * (self.den as i128);
        let d = (self.den as i128) * (rhs.den as i128);
        rational_from_i128(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = (self.num as i128) * (rhs.den as i128)
            - (rhs.num as i128) * (self.den as i128);
        let d = (self.den as i128) * (rhs.den as i128);
        rational_from_i128(n, d)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_i128(
            (self.num as i128) * (rhs.num as i128),
            (self.den as i128) * (rhs.den as i128),
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        rational_from_i128(
            (self.num as i128) * (rhs.den as i128),
            (self.den as i128) * (rhs.num as i128),
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        let n = -(self.num as i128);
        rational_from_i128(n, self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::Empty,
            });
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
            return Err(RationalError::Parse {
                at: i,
                kind: ParseKind::InvalidByte,
            });
        }
        let num_end = i;
        let mut den_start = None;
        let mut den_end = None;
        let mut slash = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
            slash = Some(i);
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
            den_start = Some(start);
            den_end = Some(i);
            if i < bytes.len() {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
        }

        let num_digits = &bytes[num_start..num_end];
        if num_digits.len() >= 2 && num_digits[0] == b'0' {
            return Err(RationalError::Parse {
                at: num_start + 1,
                kind: ParseKind::NonCanonical,
            });
        }
        if negative && num_digits == b"0" {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::NonCanonical,
            });
        }
        if let (Some(ds), Some(de)) = (den_start, den_end) {
            let digits = &bytes[ds..de];
            if digits.len() >= 2 && digits[0] == b'0' {
                return Err(RationalError::Parse {
                    at: ds + 1,
                    kind: ParseKind::NonCanonical,
                });
            }
        }

        let parse_mag = |digits: &[u8], limit: u128| -> Option<u128> {
            let mut value = 0u128;
            for &b in digits {
                let digit = (b - b'0') as u128;
                value = value.checked_mul(10)?.checked_add(digit)?;
                if value > limit {
                    return None;
                }
            }
            Some(value)
        };

        let nlimit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let nmag = parse_mag(num_digits, nlimit).ok_or(RationalError::Parse {
            at: 0,
            kind: ParseKind::OutOfRange,
        })?;

        let dmag = if let (Some(ds), Some(de)) = (den_start, den_end) {
            Some(
                parse_mag(&bytes[ds..de], i64::MAX as u128).ok_or(
                    RationalError::Parse {
                        at: ds,
                        kind: ParseKind::OutOfRange,
                    },
                )?,
            )
        } else {
            None
        };

        if let (Some(ds), Some(dm)) = (den_start, dmag) {
            if dm == 0 {
                return Err(RationalError::Parse {
                    at: ds,
                    kind: ParseKind::ZeroDenominator,
                });
            }
        }

        if let (Some(slash_at), Some(dm)) = (slash, dmag) {
            if dm == 1 {
                return Err(RationalError::Parse {
                    at: slash_at,
                    kind: ParseKind::NonCanonical,
                });
            }
            if nmag == 0 {
                return Err(RationalError::Parse {
                    at: slash_at,
                    kind: ParseKind::NonCanonical,
                });
            }
            if gcd_u128(nmag, dm) > 1 {
                return Err(RationalError::Parse {
                    at: slash_at,
                    kind: ParseKind::NonCanonical,
                });
            }
        }

        let signed_num = if negative {
            if nmag == (1u128 << 63) {
                i64::MIN
            } else {
                -(nmag as i64)
            }
        } else {
            nmag as i64
        };
        let den = dmag.unwrap_or(1) as i64;
        Ok(Rational {
            num: signed_num,
            den,
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
        let mut data = vec![zero(); self.data.len()];
        for r in 0..self.rows {
            for c in 0..self.cols {
                data[c * self.rows + r] = self.data[r * self.cols + c];
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
                let mut acc = zero();
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
            for row in p..matrix.rows {
                let value = matrix.data[row * matrix.cols + col];
                if value.num == 0 {
                    continue;
                }
                let key = (value.den, value.num.unsigned_abs(), row);
                if chosen.map_or(true, |old| {
                    let old_value = matrix.data[old * matrix.cols + col];
                    key < (old_value.den, old_value.num.unsigned_abs(), old)
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
            for c in 0..matrix.cols {
                let original = matrix.data[p * matrix.cols + c];
                matrix.data[p * matrix.cols + c] =
                    original.div(&pivot).map_err(|source| MatrixError::Arithmetic {
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
        let augmented = Matrix {
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
        let mut inverse = Vec::with_capacity(n * n);
        for r in 0..n {
            for c in 0..n {
                inverse.push(reduced.matrix.data[r * width + n + c]);
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
        let mut augmented_data = Vec::with_capacity(len);
        for r in 0..m {
            for c in 0..n {
                augmented_data.push(self.data[r * n + c]);
            }
            for c in 0..k {
                augmented_data.push(rhs.data[r * k + c]);
            }
        }
        let augmented = Matrix {
            rows: m,
            cols: width,
            data: augmented_data,
        };
        let reduced = augmented
            .eliminate(n)
            .map_err(SolveError::Matrix)?;

        for row in 0..m {
            let coeff_zero = (0..n).all(|c| reduced.matrix.data[row * width + c].num == 0);
            if coeff_zero {
                if let Some(rhs_col) =
                    (0..k).find(|&c| reduced.matrix.data[row * width + n + c].num != 0)
                {
                    let witness = reduced.matrix.data[row * width..(row + 1) * width].to_vec();
                    return Ok(Solution::None {
                        row,
                        rhs_col,
                        witness,
                    });
                }
            }
        }

        let pivots = reduced.pivots;
        let rank = pivots.len();
        let mut is_pivot = vec![false; n];
        for &col in &pivots {
            is_pivot[col] = true;
        }
        let free: Vec<usize> = (0..n).filter(|&c| !is_pivot[c]).collect();

        let particular_len = n.checked_mul(k).ok_or_else(|| {
            SolveError::Matrix(MatrixError::ShapeOverflow)
        })?;
        let mut particular_data = vec![zero(); particular_len];
        for (pivot_row, &pivot_col) in pivots.iter().enumerate() {
            for rhs_col in 0..k {
                particular_data[pivot_col * k + rhs_col] =
                    reduced.matrix.data[pivot_row * width + n + rhs_col];
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
                pivots,
            });
        }

        let basis_len = n
            .checked_mul(free.len())
            .ok_or_else(|| SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut basis_data = vec![zero(); basis_len];
        for (basis_col, &free_col) in free.iter().enumerate() {
            basis_data[free_col * free.len() + basis_col] = one();
            for (pivot_row, &pivot_col) in pivots.iter().enumerate() {
                let coefficient = reduced.matrix.data[pivot_row * width + free_col];
                let negative = coefficient.neg().map_err(|source| {
                    SolveError::Matrix(MatrixError::Arithmetic {
                        row: pivot_col,
                        col: basis_col,
                        step: rank,
                        source,
                    })
                })?;
                basis_data[pivot_col * free.len() + basis_col] = negative;
            }
        }

        Ok(Solution::Infinite {
            particular,
            free,
            basis: Matrix {
                rows: n,
                cols: (basis_len / n.max(1)),
                data: basis_data,
            },
            pivots,
        })
    }
}