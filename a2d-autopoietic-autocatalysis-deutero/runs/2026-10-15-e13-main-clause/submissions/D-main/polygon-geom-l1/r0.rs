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
const SCALE_SQUARED: u128 = 1_000_000;

fn integer_sqrt(value: u128) -> u128 {
    let mut low = 0u128;
    let mut high = value.min(u64::MAX as u128);

    while low < high {
        let mid = low + (high - low + 1) / 2;
        if mid <= value / mid {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    low
}

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let dx = (b.x as i128 - a.x as i128).unsigned_abs();
    let dy = (b.y as i128 - a.y as i128).unsigned_abs();
    let squared = dx * dx + dy * dy;
    let scaled = squared * SCALE_SQUARED;

    let floor = integer_sqrt(scaled);
    // Compare sqrt(scaled) with floor + 1/2 without floating point:
    // 4*scaled >= (2*floor+1)^2. Values are bounded by the coordinate
    // contract, so these operations fit in u128.
    let threshold = 2 * floor + 1;
    let rounded = if 4 * scaled >= threshold * threshold {
        floor + 1
    } else {
        floor
    };

    rounded as u64
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
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

    let polygon = &points[..effective_len];
    let mut signed_area2 = 0i128;
    let mut edge_milli = Vec::with_capacity(effective_len);
    let mut perimeter_milli = 0u64;
    let mut longest_edge: Option<usize> = None;

    for index in 0..effective_len {
        let a = polygon[index];
        let b = polygon[(index + 1) % effective_len];

        signed_area2 +=
            a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;

        let length = rounded_edge_milli(a, b);
        edge_milli.push(length);
        perimeter_milli += length;

        let replace = match longest_edge {
            None => true,
            Some(best_index) => {
                let best_length = edge_milli[best_index];
                if length != best_length {
                    length > best_length
                } else {
                    let pair = if a <= b { (a, b) } else { (b, a) };
                    let best_a = polygon[best_index];
                    let best_b = polygon[(best_index + 1) % effective_len];
                    let best_pair = if best_a <= best_b {
                        (best_a, best_b)
                    } else {
                        (best_b, best_a)
                    };
                    pair < best_pair || (pair == best_pair && index < best_index)
                }
            }
        };

        if replace {
            longest_edge = Some(index);
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