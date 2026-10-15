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

fn magnitude_i64(n: i64) -> u128 {
    if n < 0 {
        (-(n as i128)) as u128
    } else {
        n as u128
    }
}

fn rational_from_parts(num: i128, den: i128) -> Result<Rational, RationalError> {
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
        rational_from_parts(num as i128, den as i128)
    }

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_parts(
            self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_parts(
            self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_parts(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        rational_from_parts(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        rational_from_parts(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::Empty,
            });
        }

        let mut slash = None;
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'/' {
                if slash.is_some() {
                    return Err(RationalError::Parse {
                        at: i,
                        kind: ParseKind::InvalidByte,
                    });
                }
                slash = Some(i);
            }
        }

        let num_end = slash.unwrap_or(bytes.len());
        let negative = bytes[0] == b'-';
        let num_start = usize::from(negative);

        if num_start == num_end {
            return Err(RationalError::Parse {
                at: num_end,
                kind: ParseKind::InvalidByte,
            });
        }

        for i in num_start..num_end {
            if !bytes[i].is_ascii_digit() {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
        }

        if let Some(s) = slash {
            if s + 1 == bytes.len() {
                return Err(RationalError::Parse {
                    at: bytes.len(),
                    kind: ParseKind::InvalidByte,
                });
            }
            for i in s + 1..bytes.len() {
                if !bytes[i].is_ascii_digit() {
                    return Err(RationalError::Parse {
                        at: i,
                        kind: ParseKind::InvalidByte,
                    });
                }
            }
        }

        let num_digits = num_end - num_start;
        if num_digits >= 2 && bytes[num_start] == b'0' {
            return Err(RationalError::Parse {
                at: num_start + 1,
                kind: ParseKind::NonCanonical,
            });
        }
        if negative && num_digits == 1 && bytes[num_start] == b'0' {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::NonCanonical,
            });
        }

        if let Some(s) = slash {
            let den_start = s + 1;
            let den_digits = bytes.len() - den_start;
            if den_digits >= 2 && bytes[den_start] == b'0' {
                return Err(RationalError::Parse {
                    at: den_start + 1,
                    kind: ParseKind::NonCanonical,
                });
            }
        }

        let parse_magnitude = |range: std::ops::Range<usize>, limit: u128| -> Option<u128> {
            let mut value = 0u128;
            for &b in &bytes[range] {
                value = value.checked_mul(10)?.checked_add((b - b'0') as u128)?;
                if value > limit {
                    return None;
                }
            }
            Some(value)
        };

        let num_limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let num_mag = parse_magnitude(num_start..num_end, num_limit).ok_or(
            RationalError::Parse {
                at: 0,
                kind: ParseKind::OutOfRange,
            },
        )?;
        let num = if negative {
            if num_mag == (1u128 << 63) {
                i64::MIN
            } else {
                -(num_mag as i64)
            }
        } else {
            num_mag as i64
        };

        let (den, den_start) = if let Some(s) = slash {
            let start = s + 1;
            let mag = parse_magnitude(start..bytes.len(), i64::MAX as u128).ok_or(
                RationalError::Parse {
                    at: start,
                    kind: ParseKind::OutOfRange,
                },
            )?;
            (mag as i64, start)
        } else {
            (1, 0)
        };

        if slash.is_some() && den == 0 {
            return Err(RationalError::Parse {
                at: den_start,
                kind: ParseKind::ZeroDenominator,
            });
        }
        if slash.is_some() && den == 1 {
            return Err(RationalError::Parse {
                at: slash.unwrap(),
                kind: ParseKind::NonCanonical,
            });
        }
        if slash.is_some() && num == 0 {
            return Err(RationalError::Parse {
                at: slash.unwrap(),
                kind: ParseKind::NonCanonical,
            });
        }
        if slash.is_some() && gcd_u128(magnitude_i64(num), den as u128) > 1 {
            return Err(RationalError::Parse {
                at: slash.unwrap(),
                kind: ParseKind::NonCanonical,
            });
        }

        Ok(Rational { num, den })
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

fn shape_len(rows: usize, cols: usize) -> Result<usize, MatrixError> {
    rows.checked_mul(cols).ok_or(MatrixError::ShapeOverflow)
}

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<Rational>) -> Result<Self, MatrixError> {
        let expected = shape_len(rows, cols)?;
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
        Ok(self.data[row * self.cols + col])
    }

    pub fn transpose(&self) -> Self {
        let mut data = Vec::with_capacity(self.data.len());
        for r in 0..self.cols {
            for c in 0..self.rows {
                data.push(self.data[c * self.cols + r]);
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
        let out_len = shape_len(self.rows, rhs.cols)?;
        let mut data = Vec::with_capacity(out_len);
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

    fn eliminate(
        matrix: &mut Matrix,
        candidate_cols: usize,
    ) -> Result<(Vec<usize>, Vec<(usize, usize)>), MatrixError> {
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
                let key = (value.den, magnitude_i64(value.num), row);
                if chosen
                    .map(|q| {
                        let v = matrix.data[q * matrix.cols + col];
                        key < (v.den, magnitude_i64(v.num), q)
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
                    let index = row * matrix.cols + c;
                    matrix.data[index] = matrix.data[index]
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
        let total = shape_len(n, width)?;
        let mut data = Vec::with_capacity(total);
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
        let mut augmented = Matrix {
            rows: n,
            cols: width,
            data,
        };
        let (pivots, _) = Self::eliminate(&mut augmented, n)?;
        if pivots.len() < n {
            return Err(MatrixError::Singular { rank: pivots.len() });
        }
        let mut inverse = Vec::with_capacity(n * n);
        for r in 0..n {
            for c in 0..n {
                inverse.push(augmented.data[r * width + n + c]);
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
        let n = self.cols;
        let k = rhs.cols;
        let width = n.checked_add(k).ok_or(SolveError::AugmentedShapeOverflow)?;
        let total = self
            .rows
            .checked_mul(width)
            .ok_or(SolveError::AugmentedShapeOverflow)?;
        let mut data = Vec::with_capacity(total);
        for r in 0..self.rows {
            for c in 0..n {
                data.push(self.data[r * n + c]);
            }
            for c in 0..k {
                data.push(rhs.data[r * k + c]);
            }
        }
        let mut augmented = Matrix {
            rows: self.rows,
            cols: width,
            data,
        };
        let (pivots, _) =
            Self::eliminate(&mut augmented, n).map_err(SolveError::Matrix)?;

        for r in 0..self.rows {
            let coeff_zero = (0..n).all(|c| augmented.data[r * width + c].num == 0);
            if coeff_zero {
                for c in 0..k {
                    if augmented.data[r * width + n + c].num != 0 {
                        let witness = augmented.data[r * width..(r + 1) * width].to_vec();
                        return Ok(Solution::None {
                            row: r,
                            rhs_col: c,
                            witness,
                        });
                    }
                }
            }
        }

        let mut free = Vec::new();
        let mut pivot_mask = vec![false; n];
        for &col in &pivots {
            pivot_mask[col] = true;
        }
        for c in 0..n {
            if !pivot_mask[c] {
                free.push(c);
            }
        }

        let particular_len = n.checked_mul(k).ok_or(SolveError::Matrix(
            MatrixError::ShapeOverflow,
        ))?;
        let mut particular_data = vec![Rational { num: 0, den: 1 }; particular_len];
        for (i, &pivot_col) in pivots.iter().enumerate() {
            for c in 0..k {
                particular_data[pivot_col * k + c] = augmented.data[i * width + n + c];
            }
        }
        let particular = Matrix {
            rows: n,
            cols: k,
            data: particular_data,
        };

        if pivots.len() == n {
            return Ok(Solution::Unique {
                values: particular,
                pivots,
            });
        }

        let f = free.len();
        let basis_len = n.checked_mul(f).ok_or(SolveError::Matrix(
            MatrixError::ShapeOverflow,
        ))?;
        let mut basis_data = vec![Rational { num: 0, den: 1 }; basis_len];
        for (j, &free_col) in free.iter().enumerate() {
            basis_data[free_col * f + j] = Rational { num: 1, den: 1 };
            for (i, &pivot_col) in pivots.iter().enumerate() {
                let coefficient = augmented.data[i * width + free_col];
                basis_data[pivot_col * f + j] =
                    coefficient.neg().map_err(|source| {
                        SolveError::Matrix(MatrixError::Arithmetic {
                            row: pivot_col,
                            col: j,
                            step: pivots.len(),
                            source,
                        })
                    })?;
            }
        }
        let basis = Matrix {
            rows: n,
            cols: f,
            data: basis_data,
        };
        Ok(Solution::Infinite {
            particular,
            free,
            basis,
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