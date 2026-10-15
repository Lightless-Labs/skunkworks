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

    let effective_len = if points.len() >= 2 && points[0] == points[points.len() - 1] {
        points.len() - 1
    } else {
        points.len()
    };

    if effective_len < 3 {
        return Err(GeomError::TooFewVertices { len: effective_len });
    }

    Ok(&points[..effective_len])
}

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let dx = (i128::from(b.x) - i128::from(a.x)).unsigned_abs();
    let dy = (i128::from(b.y) - i128::from(a.y)).unsigned_abs();
    let squared_milli = (dx * dx + dy * dy) * 1_000_000;

    // Find floor(sqrt(squared_milli)) using integer arithmetic.
    let mut low = 0u128;
    let mut high = 3_000_000_000_000u128;
    while low < high {
        let mid = low + (high - low + 1) / 2;
        if mid <= squared_milli / mid.max(1) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    let floor = low;
    let remainder = squared_milli - floor * floor;
    if remainder >= floor * 2 + 1 {
        (floor + 1) as u64
    } else {
        floor as u64
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

        signed_area2 += i128::from(a.x) * i128::from(b.y)
            - i128::from(a.y) * i128::from(b.x);

        let length = rounded_edge_milli(a, b);
        edge_milli.push(length);
        perimeter_milli += length;

        let replace = match longest_edge {
            None => true,
            Some(best) => {
                if length != edge_milli[best] {
                    length > edge_milli[best]
                } else {
                    let (low, high) = if a <= b { (a, b) } else { (b, a) };
                    let best_a = polygon[best];
                    let best_b = polygon[(best + 1) % n];
                    let (best_low, best_high) = if best_a <= best_b {
                        (best_a, best_b)
                    } else {
                        (best_b, best_a)
                    };
                    (low, high, i) < (best_low, best_high, best)
                }
            }
        };
        if replace {
            longest_edge = Some(i);
        }
    }

    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
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