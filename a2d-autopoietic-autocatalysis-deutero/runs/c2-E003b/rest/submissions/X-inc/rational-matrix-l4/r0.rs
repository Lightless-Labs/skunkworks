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
        let g = gcd_u128(self.den as u128, rhs.den as u128) as i128;
        let a = rhs.den as i128 / g;
        let b = self.den as i128 / g;
        Self::from_i128(
            self.num as i128 * a + rhs.num as i128 * b,
            self.den as i128 * a,
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g = gcd_u128(self.den as u128, rhs.den as u128) as i128;
        let a = rhs.den as i128 / g;
        let b = self.den as i128 / g;
        Self::from_i128(
            self.num as i128 * a - rhs.num as i128 * b,
            self.den as i128 * a,
        )
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g1 = gcd_u128(self.num.unsigned_abs() as u128, rhs.den as u128) as i128;
        let g2 = gcd_u128(rhs.num.unsigned_abs() as u128, self.den as u128) as i128;
        Self::from_i128(
            (self.num as i128 / g1) * (rhs.num as i128 / g2),
            (self.den as i128 / g2) * (rhs.den as i128 / g1),
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        let g1 = gcd_u128(self.num.unsigned_abs() as u128, rhs.num.unsigned_abs() as u128) as i128;
        let g2 = gcd_u128(self.den as u128, rhs.den as u128) as i128;
        Self::from_i128(
            (self.num as i128 / g1) * (rhs.den as i128 / g2),
            (self.den as i128 / g2) * (rhs.num as i128 / g1),
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        let n = -(self.num as i128);
        if n < i64::MIN as i128 || n > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }
        Ok(Self {
            num: n as i64,
            den: self.den,
        })
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
        if bytes[0] == b'-' {
            i = 1;
        }
        let num_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return Err(RationalError::Parse {
                at: i,
                kind: if i == bytes.len() {
                    ParseKind::InvalidByte
                } else {
                    ParseKind::InvalidByte
                },
            });
        }
        let num_end = i;
        let mut den_start = None;
        let mut den_end = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
            i += 1;
            den_start = Some(i);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start.unwrap() {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
            den_end = Some(i);
            if i < bytes.len() {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
        }

        if num_end - num_start >= 2 && bytes[num_start] == b'0' {
            return Err(RationalError::Parse {
                at: num_start + 1,
                kind: ParseKind::NonCanonical,
            });
        }
        if bytes[0] == b'-' && &text[..num_end] == "-0" {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::NonCanonical,
            });
        }
        if let (Some(s), Some(e)) = (den_start, den_end) {
            if e - s >= 2 && bytes[s] == b'0' {
                return Err(RationalError::Parse {
                    at: s + 1,
                    kind: ParseKind::NonCanonical,
                });
            }
        }

        let negative = bytes[0] == b'-';
        let num_digits = &text[num_start..num_end];
        let num_limit: u128 = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let num_mag = parse_bounded_digits(num_digits, num_limit).ok_or(
            RationalError::Parse {
                at: 0,
                kind: ParseKind::OutOfRange,
            },
        )?;
        let num = if negative {
            if num_mag == 1u128 << 63 {
                i64::MIN
            } else {
                -(num_mag as i64)
            }
        } else {
            num_mag as i64
        };

        let mut den = 1i64;
        if let (Some(s), Some(e)) = (den_start, den_end) {
            let den_mag = parse_bounded_digits(&text[s..e], i64::MAX as u128).ok_or(
                RationalError::Parse {
                    at: s,
                    kind: ParseKind::OutOfRange,
                },
            )?;
            if den_mag == 0 {
                return Err(RationalError::Parse {
                    at: s,
                    kind: ParseKind::ZeroDenominator,
                });
            }
            den = den_mag as i64;
            let slash = s - 1;
            if den == 1 || num == 0 {
                return Err(RationalError::Parse {
                    at: slash,
                    kind: ParseKind::NonCanonical,
                });
            }
            if gcd_u128(num.unsigned_abs() as u128, den as u128) > 1 {
                return Err(RationalError::Parse {
                    at: slash,
                    kind: ParseKind::NonCanonical,
                });
            }
        }
        Ok(Self { num, den })
    }
}

fn parse_bounded_digits(s: &str, limit: u128) -> Option<u128> {
    let mut value = 0u128;
    for b in s.bytes() {
        let d = (b - b'0') as u128;
        value = value.checked_mul(10)?.checked_add(d)?;
        if value > limit {
            return None;
        }
    }
    Some(value)
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
        let mut out = Vec::with_capacity(len);
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
        &self,
        candidate_cols: usize,
    ) -> Result<(Self, Vec<usize>, Vec<(usize, usize)>), MatrixError> {
        let mut m = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;

        for col in 0..candidate_cols {
            if p == m.rows {
                break;
            }
            let mut best: Option<usize> = None;
            for r in p..m.rows {
                let v = m.data[r * m.cols + col];
                if v.num == 0 {
                    continue;
                }
                let key = (v.den as u64, v.num.unsigned_abs(), r);
                if best
                    .map(|br| {
                        let b = m.data[br * m.cols + col];
                        key < (b.den as u64, b.num.unsigned_abs(), br)
                    })
                    .unwrap_or(true)
                {
                    best = Some(r);
                }
            }
            let Some(q) = best else {
                continue;
            };
            if q != p {
                for c in 0..m.cols {
                    m.data.swap(p * m.cols + c, q * m.cols + c);
                }
                swaps.push((p, q));
            }
            let pivot = m.data[p * m.cols + col];
            let mut normalized = Vec::with_capacity(m.cols);
            for c in 0..m.cols {
                let v = m.data[p * m.cols + c]
                    .div(&pivot)
                    .map_err(|source| MatrixError::Arithmetic {
                        row: p,
                        col: c,
                        step: p,
                        source,
                    })?;
                normalized.push(v);
            }
            for c in 0..m.cols {
                m.data[p * m.cols + c] = normalized[c];
            }
            for r in 0..m.rows {
                if r == p {
                    continue;
                }
                let factor = m.data[r * m.cols + col];
                if factor.num == 0 {
                    continue;
                }
                for c in 0..m.cols {
                    let product = factor
                        .mul(&normalized[c])
                        .map_err(|source| MatrixError::Arithmetic {
                            row: r,
                            col: c,
                            step: p,
                            source,
                        })?;
                    m.data[r * m.cols + c] = m.data[r * m.cols + c]
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
        Ok((m, pivots, swaps))
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        let (matrix, pivots, swaps) = self.eliminate(self.cols)?;
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
        let (reduced, pivots, _) = augmented.eliminate(n)?;
        if pivots.len() < n {
            return Err(MatrixError::Singular { rank: pivots.len() });
        }
        let mut right = Vec::with_capacity(n * n);
        for r in 0..n {
            right.extend_from_slice(&reduced.data[r * width + n..r * width + width]);
        }
        Ok(Self {
            rows: n,
            cols: n,
            data: right,
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
            data.extend_from_slice(&self.data[r * n..(r + 1) * n]);
            data.extend_from_slice(&rhs.data[r * k..(r + 1) * k]);
        }
        let augmented = Self {
            rows: self.rows,
            cols: width,
            data,
        };
        let (reduced, pivots, _) = augmented
            .eliminate(n)
            .map_err(SolveError::Matrix)?;

        for r in 0..self.rows {
            let coefficient_zero = (0..n).all(|c| reduced.data[r * width + c].num == 0);
            if coefficient_zero {
                if let Some(rhs_col) =
                    (0..k).find(|&c| reduced.data[r * width + n + c].num != 0)
                {
                    return Ok(Solution::None {
                        row: r,
                        rhs_col,
                        witness: reduced.data[r * width..(r + 1) * width].to_vec(),
                    });
                }
            }
        }

        let nk = n.checked_mul(k).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let rank = pivots.len();
        let mut particular_data = vec![Rational { num: 0, den: 1 }; nk];
        for (i, &pivot_col) in pivots.iter().enumerate() {
            for c in 0..k {
                particular_data[pivot_col * k + c] =
                    reduced.data[i * width + n + c];
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

        let free: Vec<usize> = (0..n).filter(|c| !pivots.contains(c)).collect();
        let nf = n
            .checked_mul(free.len())
            .ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut basis_data = vec![Rational { num: 0, den: 1 }; nf];
        for (j, &free_col) in free.iter().enumerate() {
            basis_data[free_col * free.len() + j] = Rational { num: 1, den: 1 };
            for (i, &pivot_col) in pivots.iter().enumerate() {
                basis_data[pivot_col * free.len() + j] =
                    reduced.data[i * width + free_col]
                        .neg()
                        .map_err(|source| {
                            SolveError::Matrix(MatrixError::Arithmetic {
                                row: pivot_col,
                                col: j,
                                step: rank,
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
                cols: nf.checked_div(n).unwrap_or(0).min(usize::MAX),
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