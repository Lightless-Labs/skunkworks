#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Clockwise,
    Degenerate,
    CounterClockwise,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measures {
    pub signed_area2: i128,
    pub area2: u128,
    pub orientation: Orientation,
    pub edge_milli: Vec<u64>,
    pub perimeter_milli: u64,
    pub longest_edge: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeomError {
    TooManyVertices { len: usize },
    OutOfRange { index: usize },
    TooFewVertices { len: usize },
}

const COORD_LIMIT: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

fn prepare(points: &[Point]) -> Result<&[Point], GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }

    for (index, point) in points.iter().enumerate() {
        if point.x < -COORD_LIMIT
            || point.x > COORD_LIMIT
            || point.y < -COORD_LIMIT
            || point.y > COORD_LIMIT
        {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let effective = if points.len() >= 2 && points.first() == points.last() {
        &points[..points.len() - 1]
    } else {
        points
    };

    if effective.len() < 3 {
        return Err(GeomError::TooFewVertices {
            len: effective.len(),
        });
    }

    Ok(effective)
}

fn rounded_sqrt(n: u128) -> u128 {
    // Find floor(sqrt(n)) without multiplication overflow.
    let mut low = 0_u128;
    let mut high = n.min(u64::MAX as u128) + 1;

    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if mid <= n / mid {
            low = mid;
        } else {
            high = mid;
        }
    }

    let remainder = n - low * low;
    // Round up iff sqrt(n) is at least low + 1/2:
    // n >= low^2 + low + 1, expressed without risky intermediate products.
    if remainder > low {
        low + 1
    } else {
        low
    }
}

fn edge_length_milli(a: Point, b: Point) -> u64 {
    let dx = (a.x as i128 - b.x as i128).unsigned_abs();
    let dy = (a.y as i128 - b.y as i128).unsigned_abs();
    let squared = dx * dx + dy * dy;
    rounded_sqrt(squared * 1_000_000) as u64
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let points = prepare(points)?;
    let n = points.len();

    let mut signed_area2 = 0_i128;
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        signed_area2 += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }

    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
    };

    let mut edge_milli = Vec::with_capacity(n);
    let mut perimeter_milli = 0_u64;
    let mut longest_edge: Option<usize> = None;

    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        let length = edge_length_milli(a, b);
        edge_milli.push(length);
        perimeter_milli += length;

        let replace = match longest_edge {
            None => true,
            Some(current) => {
                if length > edge_milli[current] {
                    true
                } else if length < edge_milli[current] {
                    false
                } else {
                    let (low, high) = if a <= b { (a, b) } else { (b, a) };
                    let ca = points[current];
                    let cb = points[(current + 1) % n];
                    let (clow, chigh) = if ca <= cb { (ca, cb) } else { (cb, ca) };
                    (low, high, i) < (clow, chigh, current)
                }
            }
        };

        if replace {
            longest_edge = Some(i);
        }
    }

    Ok(Measures {
        signed_area2,
        area2: signed_area2.unsigned_abs(),
        orientation,
        edge_milli,
        perimeter_milli,
        longest_edge,
    })
}