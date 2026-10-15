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

fn rational_from_i128(num: i128, den: i128) -> Result<Rational, RationalError> {
    if den == 0 {
        return Err(RationalError::ZeroDenominator);
    }
    if num == 0 {
        return Ok(Rational { num: 0, den: 1 });
    }

    let (mut n, mut d) = (num, den);
    if d < 0 {
        n = -n;
        d = -d;
    }

    let gcd = gcd_u128(n.unsigned_abs(), d as u128) as i128;
    n /= gcd;
    d /= gcd;

    if n < i64::MIN as i128 || n > i64::MAX as i128 || d > i64::MAX as i128 {
        return Err(RationalError::Overflow);
    }

    Ok(Rational {
        num: n as i64,
        den: d as i64,
    })
}

fn magnitude(digits: &str) -> u128 {
    digits.bytes().fold(0u128, |value, byte| {
        value
            .saturating_mul(10)
            .saturating_add((byte - b'0') as u128)
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
        rational_from_i128(
            self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_i128(
            self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
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
        let fail = |at, kind| RationalError::Parse { at, kind };
        if text.is_empty() {
            return Err(fail(0, ParseKind::Empty));
        }

        let bytes = text.as_bytes();
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
            return Err(fail(i, ParseKind::InvalidByte));
        }

        let numerator_end = i;
        let mut denominator_start: Option<usize> = None;
        let mut denominator_end: Option<usize> = None;

        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(fail(i, ParseKind::InvalidByte));
            }
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(fail(i, ParseKind::InvalidByte));
            }
            denominator_start = Some(start);
            denominator_end = Some(i);

            if i < bytes.len() {
                return Err(fail(i, ParseKind::InvalidByte));
            }
        }

        let numerator_digits = &text[numerator_start..numerator_end];
        let slash = denominator_start.map(|start| start - 1);

        if numerator_digits.len() >= 2 && bytes[numerator_start] == b'0' {
            return Err(fail(numerator_start + 1, ParseKind::NonCanonical));
        }
        if negative && numerator_digits == "0" {
            return Err(fail(0, ParseKind::NonCanonical));
        }
        if let (Some(start), Some(end)) = (denominator_start, denominator_end) {
            if end - start >= 2 && bytes[start] == b'0' {
                return Err(fail(start + 1, ParseKind::NonCanonical));
            }
        }

        let numerator_magnitude = magnitude(numerator_digits);
        let numerator_limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        if numerator_magnitude > numerator_limit {
            return Err(fail(0, ParseKind::OutOfRange));
        }

        let denominator_magnitude =
            if let (Some(start), Some(end)) = (denominator_start, denominator_end) {
                let value = magnitude(&text[start..end]);
                if value > i64::MAX as u128 {
                    return Err(fail(start, ParseKind::OutOfRange));
                }
                Some(value)
            } else {
                None
            };

        if let (Some(start), Some(value)) = (denominator_start, denominator_magnitude) {
            if value == 0 {
                return Err(fail(start, ParseKind::ZeroDenominator));
            }
        }

        if let Some(value) = denominator_magnitude {
            if value == 1 {
                return Err(fail(slash.unwrap(), ParseKind::NonCanonical));
            }
            if numerator_magnitude == 0 {
                return Err(fail(slash.unwrap(), ParseKind::NonCanonical));
            }
            if gcd_u128(numerator_magnitude, value) > 1 {
                return Err(fail(slash.unwrap(), ParseKind::NonCanonical));
            }
        }

        let signed_numerator = if negative {
            if numerator_magnitude == (1u128 << 63) {
                i64::MIN
            } else {
                -(numerator_magnitude as i64)
            }
        } else {
            numerator_magnitude as i64
        };

        match denominator_magnitude {
            Some(den) => Ok(Rational {
                num: signed_numerator,
                den: den as i64,
            }),
            None => Ok(Rational {
                num: signed_numerator,
                den: 1,
            }),
        }
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

        let size = self
            .rows
            .checked_mul(rhs.cols)
            .ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(size);

        for row in 0..self.rows {
            for col in 0..rhs.cols {
                let mut acc = Rational { num: 0, den: 1 };
                for step in 0..self.cols {
                    let product = self.data[row * self.cols + step]
                        .mul(&rhs.data[step * rhs.cols + col])
                        .map_err(|source| MatrixError::Arithmetic {
                            row,
                            col,
                            step,
                            source,
                        })?;
                    acc = acc.add(&product).map_err(|source| MatrixError::Arithmetic {
                        row,
                        col,
                        step,
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
        data: &mut [Rational],
        rows: usize,
        cols: usize,
        pivot_limit: usize,
    ) -> Result<(Vec<usize>, Vec<(usize, usize)>), MatrixError> {
        let mut pivots: Vec<usize> = Vec::new();
        let mut swaps: Vec<(usize, usize)> = Vec::new();
        let mut pivot_row: usize = 0;

        for pivot_col in 0..pivot_limit {
            if pivot_row == rows {
                break;
            }

            let mut chosen: Option<usize> = None;
            for row in pivot_row..rows {
                let value = data[row * cols + pivot_col];
                if value.num == 0 {
                    continue;
                }
                let key = (value.den, value.num.unsigned_abs(), row);
                if chosen
                    .map(|previous| {
                        let old: Rational = data[previous * cols + pivot_col];
                        key < (old.den, old.num.unsigned_abs(), previous)
                    })
                    .unwrap_or(true)
                {
                    chosen = Some(row);
                }
            }

            let Some(chosen_row) = chosen else {
                continue;
            };

            if chosen_row != pivot_row {
                for col in 0..cols {
                    data.swap(pivot_row * cols + col, chosen_row * cols + col);
                }
                swaps.push((pivot_row, chosen_row));
            }

            let pivot = data[pivot_row * cols + pivot_col];
            for col in 0..cols {
                let original = data[pivot_row * cols + col];
                data[pivot_row * cols + col] =
                    original
                        .div(&pivot)
                        .map_err(|source| MatrixError::Arithmetic {
                            row: pivot_row,
                            col,
                            step: pivot_row,
                            source,
                        })?;
            }

            for row in 0..rows {
                if row == pivot_row {
                    continue;
                }
                let factor = data[row * cols + pivot_col];
                if factor.num == 0 {
                    continue;
                }

                for col in 0..cols {
                    let product = factor
                        .mul(&data[pivot_row * cols + col])
                        .map_err(|source| MatrixError::Arithmetic {
                            row,
                            col,
                            step: pivot_row,
                            source,
                        })?;
                    data[row * cols + col] = data[row * cols + col]
                        .sub(&product)
                        .map_err(|source| MatrixError::Arithmetic {
                            row,
                            col,
                            step: pivot_row,
                            source,
                        })?;
                }
            }

            pivots.push(pivot_col);
            pivot_row += 1;
        }

        Ok((pivots, swaps))
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        let mut data = self.data.clone();
        let (pivots, swaps) = Self::eliminate(&mut data, self.rows, self.cols, self.cols)?;
        Ok(Reduction {
            matrix: Self {
                rows: self.rows,
                cols: self.cols,
                data,
            },
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
        let size = n.checked_mul(width).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(size);

        for row in 0..n {
            data.extend_from_slice(&self.data[row * n..row * n + n]);
            for col in 0..n {
                data.push(Rational {
                    num: if row == col { 1 } else { 0 },
                    den: 1,
                });
            }
        }

        let (pivots, _) = Self::eliminate(&mut data, n, width, n)?;
        if pivots.len() < n {
            return Err(MatrixError::Singular { rank: pivots.len() });
        }

        let mut inverse = Vec::with_capacity(n * n);
        for row in 0..n {
            inverse.extend_from_slice(&data[row * width + n..row * width + width]);
        }

        Ok(Self {
            rows: n,
            cols: n,
            data: inverse,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn q(n: i64, d: i64) -> Rational {
        Rational::new(n, d).unwrap()
    }

    #[test]
    fn rational_boundaries_and_errors() {
        assert_eq!(Rational::new(0, 0), Err(RationalError::ZeroDenominator));
        assert_eq!(Rational::new(i64::MIN, -1), Err(RationalError::Overflow));
        assert_eq!(Rational::new(i64::MIN, 1).unwrap().neg(), Err(RationalError::Overflow));
        assert_eq!(q(0, i64::MIN), q(0, 1));
        assert_eq!(q(2, -4), q(-1, 2));
        assert_eq!(q(1, 2).div(&q(0, 1)), Err(RationalError::DivideByZero));
        assert_eq!(Rational::parse("-9223372036854775808").unwrap().numerator(), i64::MIN);
    }

    #[test]
    fn strict_parse_errors() {
        let parse_error = |at, kind| RationalError::Parse { at, kind };
        assert_eq!(Rational::parse(""), Err(parse_error(0, ParseKind::Empty)));
        assert_eq!(Rational::parse("01/2"), Err(parse_error(1, ParseKind::NonCanonical)));
        assert_eq!(Rational::parse("-0"), Err(parse_error(0, ParseKind::NonCanonical)));
        assert_eq!(Rational::parse("1/0"), Err(parse_error(2, ParseKind::ZeroDenominator)));
        assert_eq!(Rational::parse("1/2x"), Err(parse_error(3, ParseKind::InvalidByte)));
        assert_eq!(Rational::parse("1/"), Err(parse_error(2, ParseKind::InvalidByte)));
        assert_eq!(Rational::parse("1/2/3"), Err(parse_error(3, ParseKind::InvalidByte)));
        assert_eq!(Rational::parse("2/4"), Err(parse_error(1, ParseKind::NonCanonical)));
    }

    #[test]
    fn matrix_boundaries_and_errors() {
        assert_eq!(
            Matrix::new(usize::MAX, 2, Vec::new()).unwrap_err(),
            MatrixError::ShapeOverflow
        );
        assert_eq!(
            Matrix::new(2, 2, vec![q(1, 1)]).unwrap_err(),
            MatrixError::DataLength { expected: 4, actual: 1 }
        );
        let empty = Matrix::new(0, usize::MAX, Vec::new()).unwrap();
        assert_eq!(empty.transpose().transpose(), empty);
        assert_eq!(empty.get(0, 0), Err(MatrixError::Index { row: 0, col: 0 }));
    }

    #[test]
    fn elimination_and_inverse() {
        let matrix = Matrix::new(
            2,
            3,
            vec![q(1, 1), q(2, 1), q(3, 1), q(2, 1), q(4, 1), q(6, 1)],
        )
        .unwrap();
        let reduction = matrix.rref().unwrap();
        assert_eq!(reduction.pivots, vec![0]);
        assert_eq!(reduction.rank(), 1);

        let invertible =
            Matrix::new(2, 2, vec![q(1, 1), q(2, 1), q(0, 1), q(1, 1)]).unwrap();
        let inverse = invertible.inverse().unwrap();
        assert_eq!(inverse.data(), &[q(1, 1), q(-2, 1), q(0, 1), q(1, 1)]);
    }
}

fn main() {}