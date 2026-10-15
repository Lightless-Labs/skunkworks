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

    if n < i64::MIN as i128
        || n > i64::MAX as i128
        || d < 1
        || d > i64::MAX as i128
    {
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
        let n = self.num as i128 * rhs.den as i128
            + rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        rational_from_i128(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = self.num as i128 * rhs.den as i128
            - rhs.num as i128 * self.den as i128;
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

        let numerator_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == numerator_start {
            return Err(RationalError::Parse {
                at: i,
                kind: ParseKind::InvalidByte,
            });
        }

        let numerator_end = i;
        let mut slash = None;
        let mut denominator_start = 0;
        let mut denominator_end = 0;

        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
            slash = Some(i);
            i += 1;
            denominator_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == denominator_start {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
            denominator_end = i;
            if i < bytes.len() {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
        }

        if numerator_end - numerator_start >= 2 && bytes[numerator_start] == b'0' {
            return Err(RationalError::Parse {
                at: numerator_start + 1,
                kind: ParseKind::NonCanonical,
            });
        }
        if negative && numerator_end == 2 && bytes[1] == b'0' {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::NonCanonical,
            });
        }
        if slash.is_some()
            && denominator_end - denominator_start >= 2
            && bytes[denominator_start] == b'0'
        {
            return Err(RationalError::Parse {
                at: denominator_start + 1,
                kind: ParseKind::NonCanonical,
            });
        }

        let parse_error = |at| RationalError::Parse {
            at,
            kind: ParseKind::OutOfRange,
        };

        let mut numerator_magnitude: u128 = 0;
        for &b in &bytes[numerator_start..numerator_end] {
            numerator_magnitude = numerator_magnitude * 10 + (b - b'0') as u128;
            let limit = if negative {
                1u128 << 63
            } else {
                i64::MAX as u128
            };
            if numerator_magnitude > limit {
                return Err(parse_error(0));
            }
        }

        let numerator = if negative {
            if numerator_magnitude == 1u128 << 63 {
                i64::MIN
            } else {
                -(numerator_magnitude as i64)
            }
        } else {
            numerator_magnitude as i64
        };

        let Some(slash_at) = slash else {
            return Ok(Rational {
                num: numerator,
                den: 1,
            });
        };

        let mut denominator: u128 = 0;
        for &b in &bytes[denominator_start..denominator_end] {
            denominator = denominator * 10 + (b - b'0') as u128;
            if denominator > i64::MAX as u128 {
                return Err(parse_error(denominator_start));
            }
        }

        if denominator == 0 {
            return Err(RationalError::Parse {
                at: denominator_start,
                kind: ParseKind::ZeroDenominator,
            });
        }
        if denominator == 1 || numerator == 0 {
            return Err(RationalError::Parse {
                at: slash_at,
                kind: ParseKind::NonCanonical,
            });
        }
        if gcd_u128(numerator_magnitude, denominator) > 1 {
            return Err(RationalError::Parse {
                at: slash_at,
                kind: ParseKind::NonCanonical,
            });
        }

        Ok(Rational {
            num: numerator,
            den: denominator as i64,
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

fn arithmetic_error(
    row: usize,
    col: usize,
    step: usize,
    source: RationalError,
) -> MatrixError {
    MatrixError::Arithmetic {
        row,
        col,
        step,
        source,
    }
}

impl Matrix {
    pub fn new(
        rows: usize,
        cols: usize,
        data: Vec<Rational>,
    ) -> Result<Self, MatrixError> {
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
            .and_then(|x| x.checked_add(col))
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
        let result_len = self
            .rows
            .checked_mul(rhs.cols)
            .ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(result_len);

        for row in 0..self.rows {
            for col in 0..rhs.cols {
                let mut acc = zero();
                for k in 0..self.cols {
                    let left = self.data[row * self.cols + k];
                    let right = rhs.data[k * rhs.cols + col];
                    let term = left
                        .mul(&right)
                        .map_err(|e| arithmetic_error(row, col, k, e))?;
                    acc = acc
                        .add(&term)
                        .map_err(|e| arithmetic_error(row, col, k, e))?;
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
        pivot_columns: usize,
    ) -> Result<Reduction, MatrixError> {
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;

        for column in 0..pivot_columns {
            if p == matrix.rows {
                break;
            }

            let mut chosen: Option<usize> = None;
            for row in p..matrix.rows {
                let value = matrix.data[row * matrix.cols + column];
                if value.num == 0 {
                    continue;
                }
                let key = (value.den, value.num.unsigned_abs(), row);
                let replace = match chosen {
                    None => true,
                    Some(old_row) => {
                        let old = matrix.data[old_row * matrix.cols + column];
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
                for col in 0..matrix.cols {
                    matrix.data.swap(p * matrix.cols + col, q * matrix.cols + col);
                }
                swaps.push((p, q));
            }

            let pivot = matrix.data[p * matrix.cols + column];
            let row_start = p * matrix.cols;
            for col in 0..matrix.cols {
                let original = matrix.data[row_start + col];
                matrix.data[row_start + col] = original
                    .div(&pivot)
                    .map_err(|e| arithmetic_error(p, col, p, e))?;
            }

            for row in 0..matrix.rows {
                if row == p {
                    continue;
                }
                let factor = matrix.data[row * matrix.cols + column];
                if factor.num == 0 {
                    continue;
                }
                let pivot_start = p * matrix.cols;
                let row_start = row * matrix.cols;
                for col in 0..matrix.cols {
                    let product = factor
                        .mul(&matrix.data[pivot_start + col])
                        .map_err(|e| arithmetic_error(row, col, p, e))?;
                    matrix.data[row_start + col] = matrix.data[row_start + col]
                        .sub(&product)
                        .map_err(|e| arithmetic_error(row, col, p, e))?;
                }
            }

            pivots.push(column);
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
        let total = n.checked_mul(width).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(total);

        for row in 0..n {
            for col in 0..width {
                if col < n {
                    data.push(self.data[row * n + col]);
                } else if col - n == row {
                    data.push(one());
                } else {
                    data.push(zero());
                }
            }
        }

        let augmented = Matrix {
            rows: n,
            cols: width,
            data,
        };
        let reduction = Self::eliminate(augmented, n)?;
        let rank = reduction.pivots.len();
        if rank < n {
            return Err(MatrixError::Singular { rank });
        }

        let mut inverse_data = Vec::with_capacity(total);
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
}

fn main() {}