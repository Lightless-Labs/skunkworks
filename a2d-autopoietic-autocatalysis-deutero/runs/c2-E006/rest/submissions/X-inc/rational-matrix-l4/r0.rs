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

impl Rational {
    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        Self::from_i128(num as i128, den as i128)
    }

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
            n = -n;
            d = -d;
        }
        let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
        n /= g;
        d /= g;

        if n < i64::MIN as i128 || n > i64::MAX as i128 || d < 1 || d > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }
        Ok(Self { num: n as i64, den: d as i64 })
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
        let n = self.num as i128 * a + rhs.num as i128 * b;
        let d = self.den as i128 * a;
        Self::from_i128(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g = gcd_u128(self.den as u128, rhs.den as u128) as i128;
        let a = rhs.den as i128 / g;
        let b = self.den as i128 / g;
        let n = self.num as i128 * a - rhs.num as i128 * b;
        let d = self.den as i128 * a;
        Self::from_i128(n, d)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g1 = gcd_u128(self.num.unsigned_abs() as u128, rhs.den as u128) as i128;
        let g2 = gcd_u128(rhs.num.unsigned_abs() as u128, self.den as u128) as i128;
        let n = (self.num as i128 / g1) * (rhs.num as i128 / g2);
        let d = (self.den as i128 / g2) * (rhs.den as i128 / g1);
        Self::from_i128(n, d)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        let g1 = gcd_u128(self.num.unsigned_abs() as u128, rhs.num.unsigned_abs() as u128) as i128;
        let g2 = gcd_u128(self.den as u128, rhs.den as u128) as i128;
        let mut n = (self.num as i128 / g1) * (rhs.den as i128 / g2);
        let mut d = (self.den as i128 / g2) * (rhs.num as i128 / g1);
        if d < 0 {
            n = -n;
            d = -d;
        }
        Self::from_i128(n, d)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::from_i128(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(RationalError::Parse { at: 0, kind: ParseKind::Empty });
        }

        let mut i = 0;
        if bytes[i] == b'-' {
            i += 1;
        }
        let num_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
        }

        let mut slash = None;
        let mut den_start = 0;
        let mut den_end = 0;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
            }
            slash = Some(i);
            i += 1;
            den_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start {
                return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
            }
            den_end = i;
        }
        if i < bytes.len() {
            return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
        }

        let parse_err = |at, kind| RationalError::Parse { at, kind };
        let negative = bytes[0] == b'-';
        let num_digits = &bytes[num_start..slash.unwrap_or(bytes.len())];
        if num_digits.len() >= 2 && num_digits[0] == b'0' {
            return Err(parse_err(if negative { 2 } else { 1 }, ParseKind::NonCanonical));
        }
        if negative && num_digits == b"0" {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }
        if slash.is_some() && den_end - den_start >= 2 && bytes[den_start] == b'0' {
            return Err(parse_err(den_start + 1, ParseKind::NonCanonical));
        }

        let num_mag = parse_magnitude(num_digits);
        let num_limit = if negative { 1u128 << 63 } else { i64::MAX as u128 };
        let num_mag = match num_mag {
            Some(v) if v <= num_limit => v,
            _ => return Err(parse_err(0, ParseKind::OutOfRange)),
        };

        let den_mag = if slash.is_some() {
            match parse_magnitude(&bytes[den_start..den_end]) {
                Some(v) if v <= i64::MAX as u128 => v,
                _ => return Err(parse_err(den_start, ParseKind::OutOfRange)),
            }
        } else {
            1
        };

        if slash.is_some() && den_mag == 0 {
            return Err(parse_err(den_start, ParseKind::ZeroDenominator));
        }
        if slash.is_some() && den_mag == 1 {
            return Err(parse_err(slash.unwrap(), ParseKind::NonCanonical));
        }
        if slash.is_some() && num_mag == 0 {
            return Err(parse_err(slash.unwrap(), ParseKind::NonCanonical));
        }
        if slash.is_some() && gcd_u128(num_mag, den_mag) > 1 {
            return Err(parse_err(slash.unwrap(), ParseKind::NonCanonical));
        }

        let n = if negative {
            if num_mag == (1u128 << 63) { i64::MIN } else { -(num_mag as i64) }
        } else {
            num_mag as i64
        };
        Ok(Self { num: n, den: den_mag as i64 })
    }
}

fn parse_magnitude(bytes: &[u8]) -> Option<u128> {
    let mut value = 0u128;
    for &b in bytes {
        value = value.checked_mul(10)?.checked_add((b - b'0') as u128)?;
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
pub struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<Rational>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatrixError {
    ShapeOverflow,
    DataLength { expected: usize, actual: usize },
    Index { row: usize, col: usize },
    DimensionMismatch { left: (usize, usize), right: (usize, usize) },
    Arithmetic { row: usize, col: usize, step: usize, source: RationalError },
    NotSquare { rows: usize, cols: usize },
    Singular { rank: usize },
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
    Unique { values: Matrix, pivots: Vec<usize> },
    None { row: usize, rhs_col: usize, witness: Vec<Rational> },
    Infinite { particular: Matrix, free: Vec<usize>, basis: Matrix, pivots: Vec<usize> },
}

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<Rational>) -> Result<Self, MatrixError> {
        let expected = rows.checked_mul(cols).ok_or(MatrixError::ShapeOverflow)?;
        if data.len() != expected {
            return Err(MatrixError::DataLength { expected, actual: data.len() });
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
        Self { rows: self.cols, cols: self.rows, data }
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, MatrixError> {
        if self.cols != rhs.rows {
            return Err(MatrixError::DimensionMismatch {
                left: (self.rows, self.cols),
                right: (rhs.rows, rhs.cols),
            });
        }
        let len = self.rows.checked_mul(rhs.cols).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = Rational { num: 0, den: 1 };
                for k in 0..self.cols {
                    let lhs = self.data[r * self.cols + k];
                    let rhsv = rhs.data[k * rhs.cols + c];
                    let term = lhs.mul(&rhsv).map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: k, source })?;
                    acc = acc.add(&term).map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: k, source })?;
                }
                data.push(acc);
            }
        }
        Ok(Self { rows: self.rows, cols: rhs.cols, data })
    }

    fn eliminate(&self, candidate_cols: usize) -> Result<Reduction, MatrixError> {
        let mut out = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;

        for col in 0..candidate_cols {
            if p == out.rows {
                break;
            }
            let mut best: Option<(i64, u128, usize)> = None;
            for row in p..out.rows {
                let v = out.data[row * out.cols + col];
                if v.num == 0 {
                    continue;
                }
                let key = (v.den, v.num.unsigned_abs() as u128, row);
                if best.map_or(true, |b| key < b) {
                    best = Some(key);
                }
            }
            let Some((_, _, q)) = best else { continue };
            if q != p {
                for c in 0..out.cols {
                    out.data.swap(p * out.cols + c, q * out.cols + c);
                }
                swaps.push((p, q));
            }

            let pivot = out.data[p * out.cols + col];
            for c in 0..out.cols {
                let old = out.data[p * out.cols + c];
                out.data[p * out.cols + c] = old.div(&pivot).map_err(|source| MatrixError::Arithmetic { row: p, col: c, step: p, source })?;
            }

            for row in 0..out.rows {
                if row == p {
                    continue;
                }
                let factor = out.data[row * out.cols + col];
                if factor.num == 0 {
                    continue;
                }
                for c in 0..out.cols {
                    let product = factor.mul(&out.data[p * out.cols + c]).map_err(|source| MatrixError::Arithmetic { row, col: c, step: p, source })?;
                    out.data[row * out.cols + c] = out.data[row * out.cols + c].sub(&product).map_err(|source| MatrixError::Arithmetic { row, col: c, step: p, source })?;
                }
            }
            pivots.push(col);
            p += 1;
        }
        Ok(Reduction { matrix: out, pivots, swaps })
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        self.eliminate(self.cols)
    }

    pub fn rank(&self) -> Result<usize, MatrixError> {
        Ok(self.rref()?.rank())
    }

    pub fn inverse(&self) -> Result<Self, MatrixError> {
        if self.rows != self.cols {
            return Err(MatrixError::NotSquare { rows: self.rows, cols: self.cols });
        }
        let n = self.rows;
        let width = n.checked_mul(2).ok_or(MatrixError::ShapeOverflow)?;
        let len = n.checked_mul(width).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = vec![Rational { num: 0, den: 1 }; len];
        for r in 0..n {
            for c in 0..n {
                data[r * width + c] = self.data[r * n + c];
            }
            data[r * width + n + r] = Rational { num: 1, den: 1 };
        }
        let augmented = Self { rows: n, cols: width, data };
        let reduction = augmented.eliminate(n)?;
        if reduction.pivots.len() < n {
            return Err(MatrixError::Singular { rank: reduction.pivots.len() });
        }
        let mut result = Vec::with_capacity(n * n);
        for r in 0..n {
            result.extend_from_slice(&reduction.matrix.data[r * width + n..r * width + width]);
        }
        Ok(Self { rows: n, cols: n, data: result })
    }

    pub fn solve(&self, rhs: &Self) -> Result<Solution, SolveError> {
        if self.rows != rhs.rows {
            return Err(SolveError::RowMismatch { coefficients: self.rows, rhs: rhs.rows });
        }
        let n = self.cols;
        let k = rhs.cols;
        let width = n.checked_add(k).ok_or(SolveError::AugmentedShapeOverflow)?;
        let len = self.rows.checked_mul(width).ok_or(SolveError::AugmentedShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        for r in 0..self.rows {
            data.extend_from_slice(&self.data[r * n..r * n + n]);
            data.extend_from_slice(&rhs.data[r * k..r * k + k]);
        }
        let augmented = Self { rows: self.rows, cols: width, data };
        let reduction = augmented.eliminate(n).map_err(SolveError::Matrix)?;

        for r in 0..self.rows {
            if (0..n).all(|c| reduction.matrix.data[r * width + c].num == 0) {
                if let Some(rhs_col) = (0..k).find(|&c| reduction.matrix.data[r * width + n + c].num != 0) {
                    let witness = reduction.matrix.data[r * width..(r + 1) * width].to_vec();
                    return Ok(Solution::None { row: r, rhs_col, witness });
                }
            }
        }

        let rank = reduction.pivots.len();
        let particular_len = n.checked_mul(k).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut particular = vec![Rational { num: 0, den: 1 }; particular_len];
        for (pivot_row, &pivot_col) in reduction.pivots.iter().enumerate() {
            for c in 0..k {
                particular[pivot_col * k + c] = reduction.matrix.data[pivot_row * width + n + c];
            }
        }
        let particular_matrix = Matrix { rows: n, cols: k, data: particular };
        let pivots = reduction.pivots;

        if rank == n {
            return Ok(Solution::Unique { values: particular_matrix, pivots });
        }

        let free: Vec<usize> = (0..n).filter(|c| !pivots.contains(c)).collect();
        let basis_len = n.checked_mul(free.len()).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut basis = vec![Rational { num: 0, den: 1 }; basis_len];
        for (j, &free_col) in free.iter().enumerate() {
            basis[free_col * free.len() + j] = Rational { num: 1, den: 1 };
            for (pivot_row, &pivot_col) in pivots.iter().enumerate() {
                let coefficient = reduction.matrix.data[pivot_row * width + free_col];
                basis[pivot_col * free.len() + j] = coefficient.neg().map_err(|source| {
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
            particular: particular_matrix,
            free,
            basis: Matrix { rows: n, cols: (basis_len / n.max(1)), data: basis },
            pivots,
        })
    }
}