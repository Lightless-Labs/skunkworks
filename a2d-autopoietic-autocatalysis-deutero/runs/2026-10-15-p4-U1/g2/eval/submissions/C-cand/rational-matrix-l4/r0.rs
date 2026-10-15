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
        let (mut n, mut d) = (num, den);
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

    pub fn numerator(&self) -> i64 { self.num }
    pub fn denominator(&self) -> i64 { self.den }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128(
            self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128(
            self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
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
        let b = text.as_bytes();
        if b.is_empty() {
            return Err(parse_err(0, ParseKind::Empty));
        }
        let mut i = 0;
        let neg = b[0] == b'-';
        if neg { i += 1; }
        let num_start = i;
        while i < b.len() && b[i].is_ascii_digit() { i += 1; }
        if i == num_start {
            return Err(parse_err(i, ParseKind::InvalidByte));
        }
        let num_end = i;
        let mut den_start = None;
        let mut den_end = None;
        if i < b.len() && b[i] == b'/' {
            i += 1;
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() { i += 1; }
            if i == start {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            den_start = Some(start);
            den_end = Some(i);
        }
        if i < b.len() {
            return Err(parse_err(i, ParseKind::InvalidByte));
        }

        let num_digits = &b[num_start..num_end];
        if num_digits.len() >= 2 && num_digits[0] == b'0' {
            return Err(parse_err(if neg { 2 } else { 1 }, ParseKind::NonCanonical));
        }
        if neg && num_digits == b"0" {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }
        if let (Some(s), Some(e)) = (den_start, den_end) {
            if e - s >= 2 && b[s] == b'0' {
                return Err(parse_err(s + 1, ParseKind::NonCanonical));
            }
        }

        let nmag = parse_magnitude(num_digits);
        let nlimit = if neg { 1u128 << 63 } else { i64::MAX as u128 };
        if nmag.is_none() || nmag.unwrap() > nlimit {
            return Err(parse_err(0, ParseKind::OutOfRange));
        }
        let nmag = nmag.unwrap();
        let num = if neg {
            if nmag == 1u128 << 63 { i64::MIN } else { -(nmag as i64) }
        } else { nmag as i64 };

        let (den, slash) = if let (Some(s), Some(e)) = (den_start, den_end) {
            let dmag = parse_magnitude(&b[s..e]);
            if dmag.is_none() || dmag.unwrap() > i64::MAX as u128 {
                return Err(parse_err(s, ParseKind::OutOfRange));
            }
            (Some(dmag.unwrap() as i64), Some(s - 1))
        } else { (None, None) };

        if let (Some(d), Some(s)) = (den, den_start) {
            if d == 0 { return Err(parse_err(s, ParseKind::ZeroDenominator)); }
            if d == 1 { return Err(parse_err(s - 1, ParseKind::NonCanonical)); }
            if num == 0 { return Err(parse_err(s - 1, ParseKind::NonCanonical)); }
            if gcd_u128(num.unsigned_abs() as u128, d as u128) > 1 {
                return Err(parse_err(s - 1, ParseKind::NonCanonical));
            }
            let _ = slash;
            return Ok(Self { num, den: d });
        }
        Ok(Self { num, den: 1 })
    }
}

fn parse_err(at: usize, kind: ParseKind) -> RationalError {
    RationalError::Parse { at, kind }
}

fn parse_magnitude(bytes: &[u8]) -> Option<u128> {
    let mut value = 0u128;
    for &c in bytes {
        value = value.checked_mul(10)?.checked_add((c - b'0') as u128)?;
    }
    Some(value)
}

impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 { write!(f, "{}", self.num) }
        else { write!(f, "{}/{}", self.num, self.den) }
    }
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
pub struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<Rational>,
}

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<Rational>) -> Result<Self, MatrixError> {
        let expected = rows.checked_mul(cols).ok_or(MatrixError::ShapeOverflow)?;
        if data.len() != expected {
            return Err(MatrixError::DataLength { expected, actual: data.len() });
        }
        Ok(Self { rows, cols, data })
    }
    pub fn rows(&self) -> usize { self.rows }
    pub fn cols(&self) -> usize { self.cols }
    pub fn data(&self) -> &[Rational] { &self.data }

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
        Self { rows: self.cols, cols: self.rows, data }
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, MatrixError> {
        if self.cols != rhs.rows {
            return Err(MatrixError::DimensionMismatch {
                left: (self.rows, self.cols), right: (rhs.rows, rhs.cols),
            });
        }
        let len = self.rows.checked_mul(rhs.cols).ok_or(MatrixError::ShapeOverflow)?;
        let mut out = Vec::with_capacity(len);
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut sum = Rational { num: 0, den: 1 };
                for k in 0..self.cols {
                    let a = self.data[r * self.cols + k];
                    let b = rhs.data[k * rhs.cols + c];
                    let term = a.mul(&b).map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: k, source })?;
                    sum = sum.add(&term).map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: k, source })?;
                }
                out.push(sum);
            }
        }
        Ok(Self { rows: self.rows, cols: rhs.cols, data: out })
    }

    fn eliminate(&self, candidate_cols: usize) -> Result<Reduction, MatrixError> {
        let mut m = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0;
        for col in 0..candidate_cols {
            if p == m.rows { break; }
            let mut best: Option<(i64, u128, usize)> = None;
            for row in p..m.rows {
                let v = m.data[row * m.cols + col];
                if v.num == 0 { continue; }
                let key = (v.den, v.num.unsigned_abs() as u128, row);
                if best.map_or(true, |b| key < b) { best = Some(key); }
            }
            let Some((_, _, q)) = best else { continue; };
            if q != p {
                for c in 0..m.cols { m.data.swap(p * m.cols + c, q * m.cols + c); }
                swaps.push((p, q));
            }
            let pivot = m.data[p * m.cols + col];
            for c in 0..m.cols {
                let idx = p * m.cols + c;
                m.data[idx] = m.data[idx].div(&pivot).map_err(|source| MatrixError::Arithmetic { row: p, col: c, step: p, source })?;
            }
            for row in 0..m.rows {
                if row == p { continue; }
                let factor = m.data[row * m.cols + col];
                if factor.num == 0 { continue; }
                for c in 0..m.cols {
                    let product = factor.mul(&m.data[p * m.cols + c]).map_err(|source| MatrixError::Arithmetic { row, col: c, step: p, source })?;
                    let idx = row * m.cols + c;
                    m.data[idx] = m.data[idx].sub(&product).map_err(|source| MatrixError::Arithmetic { row, col: c, step: p, source })?;
                }
            }
            pivots.push(col);
            p += 1;
        }
        Ok(Reduction { matrix: m, pivots, swaps })
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> { self.eliminate(self.cols) }
    pub fn rank(&self) -> Result<usize, MatrixError> { Ok(self.rref()?.rank()) }

    pub fn inverse(&self) -> Result<Self, MatrixError> {
        if self.rows != self.cols {
            return Err(MatrixError::NotSquare { rows: self.rows, cols: self.cols });
        }
        let n = self.rows;
        let width = n.checked_mul(2).ok_or(MatrixError::ShapeOverflow)?;
        let len = n.checked_mul(width).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        for r in 0..n {
            data.extend_from_slice(&self.data[r * n..(r + 1) * n]);
            for c in 0..n { data.push(Rational { num: (r == c) as i64, den: 1 }); }
        }
        let red = Matrix { rows: n, cols: width, data }.eliminate(n)?;
        if red.pivots.len() < n { return Err(MatrixError::Singular { rank: red.pivots.len() }); }
        let mut out = Vec::with_capacity(n * n);
        for r in 0..n {
            out.extend_from_slice(&red.matrix.data[r * width + n..r * width + width]);
        }
        Ok(Self { rows: n, cols: n, data: out })
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
        let red = Matrix { rows: self.rows, cols: width, data }
            .eliminate(n).map_err(SolveError::Matrix)?;

        for r in 0..self.rows {
            if red.matrix.data[r * width..r * width + n].iter().all(|v| v.num == 0) {
                if let Some(j) = (0..k).find(|&j| red.matrix.data[r * width + n + j].num != 0) {
                    return Ok(Solution::None {
                        row: r, rhs_col: j,
                        witness: red.matrix.data[r * width..(r + 1) * width].to_vec(),
                    });
                }
            }
        }

        let rank = red.pivots.len();
        let plen = n.checked_mul(k).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut particular = vec![Rational { num: 0, den: 1 }; plen];
        for (i, &pc) in red.pivots.iter().enumerate() {
            for j in 0..k {
                particular[pc * k + j] = red.matrix.data[i * width + n + j];
            }
        }
        let particular = Matrix { rows: n, cols: k, data: particular };
        if rank == n {
            return Ok(Solution::Unique { values: particular, pivots: red.pivots });
        }

        let free: Vec<usize> = (0..n).filter(|c| !red.pivots.contains(c)).collect();
        let blen = n.checked_mul(free.len()).ok_or(SolveError::Matrix(MatrixError::ShapeOverflow))?;
        let mut basis = vec![Rational { num: 0, den: 1 }; blen];
        for (j, &fc) in free.iter().enumerate() {
            basis[fc * free.len() + j] = Rational { num: 1, den: 1 };
            for (i, &pc) in red.pivots.iter().enumerate() {
                let v = red.matrix.data[i * width + fc].neg().map_err(|source| {
                    SolveError::Matrix(MatrixError::Arithmetic { row: pc, col: j, step: rank, source })
                })?;
                basis[pc * free.len() + j] = v;
            }
        }
        Ok(Solution::Infinite {
            particular, free,
            basis: Matrix { rows: n, cols: (blen / n.max(1)), data: basis },
            pivots: red.pivots,
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
    pub fn rank(&self) -> usize { self.pivots.len() }
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

fn main() {}