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

const MAX_VERTICES: usize = 4096;
const COORD_LIMIT: i64 = 1_000_000_000;

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

    let effective_len = points.len()
        - usize::from(points.len() >= 2 && points.first() == points.last());
    if effective_len < 3 {
        return Err(GeomError::TooFewVertices { len: effective_len });
    }

    Ok(&points[..effective_len])
}

fn rounded_sqrt(value: u128) -> u128 {
    // Integer square root, rounded to nearest with exact half ties upward.
    let mut lo = 0u128;
    let mut hi = value.min(u64::MAX as u128) + 1;

    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if mid <= value / mid {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    let lower_square = lo * lo;
    let upper = lo + 1;
    let upper_square = upper * upper;

    if value - lower_square >= upper_square - value {
        upper
    } else {
        lo
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare(points)?;
    let n = polygon.len();

    let mut signed_area2 = 0i128;
    let mut edge_milli = Vec::with_capacity(n);
    let mut perimeter_milli = 0u64;
    let mut longest_edge: Option<usize> = None;

    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];

        signed_area2 += (a.x as i128) * (b.y as i128)
            - (a.y as i128) * (b.x as i128);

        let dx = (b.x as i128) - (a.x as i128);
        let dy = (b.y as i128) - (a.y as i128);
        let d2 = (dx * dx + dy * dy) as u128;
        let rounded = rounded_sqrt(d2 * 1_000_000) as u64;

        edge_milli.push(rounded);
        perimeter_milli += rounded;

        let should_replace = match longest_edge {
            None => true,
            Some(best_index) => {
                let best_length = edge_milli[best_index];
                if rounded != best_length {
                    rounded > best_length
                } else {
                    let endpoints = if a <= b { (a, b) } else { (b, a) };
                    let best_a = polygon[best_index];
                    let best_b = polygon[(best_index + 1) % n];
                    let best_endpoints = if best_a <= best_b {
                        (best_a, best_b)
                    } else {
                        (best_b, best_a)
                    };
                    endpoints < best_endpoints
                        || (endpoints == best_endpoints && i < best_index)
                }
            }
        };

        if should_replace {
            longest_edge = Some(i);
        }
    }

    let orientation = if signed_area2 > 0 {
        Orientation::CounterClockwise
    } else if signed_area2 < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
    };

    Ok(Measures {
        signed_area2,
        area2: signed_area2.unsigned_abs(),
        orientation,
        edge_milli,
        perimeter_milli,
        longest_edge,
    })
}

fn main() {}