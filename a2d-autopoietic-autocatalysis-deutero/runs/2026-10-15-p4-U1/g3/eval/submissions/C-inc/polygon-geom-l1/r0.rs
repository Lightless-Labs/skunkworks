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

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let dx = (b.x as i128 - a.x as i128).unsigned_abs();
    let dy = (b.y as i128 - a.y as i128).unsigned_abs();

    // Since each coordinate is within ±1e9, this value is at most 8e24.
    let squared_milli = (dx * dx + dy * dy) * 1_000_000;

    // Find floor(sqrt(squared_milli)) without overflowing when squaring
    // candidate values.
    let mut low = 0u128;
    let mut high = 3_000_000_000u128;
    while low < high {
        let mid = low + (high - low + 1) / 2;
        if mid <= squared_milli / mid {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    // Round to nearest integer, with exact half ties upward. For integer
    // squared_milli, compare against (low + 1/2)^2 without fractions.
    let midpoint_threshold = low * low + low + 1;
    if squared_milli >= midpoint_threshold {
        (low + 1) as u64
    } else {
        low as u64
    }
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

    let effective_len = if points.len() >= 2 && points[points.len() - 1] == points[0] {
        points.len() - 1
    } else {
        points.len()
    };

    if effective_len < 3 {
        return Err(GeomError::TooFewVertices { len: effective_len });
    }

    let polygon = &points[..effective_len];
    let mut signed_area2 = 0i128;

    for i in 0..effective_len {
        let current = polygon[i];
        let next = polygon[(i + 1) % effective_len];
        signed_area2 += current.x as i128 * next.y as i128
            - current.y as i128 * next.x as i128;
    }

    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
    };

    let mut edge_milli = Vec::with_capacity(effective_len);
    let mut perimeter_milli = 0u64;
    let mut longest_edge: Option<usize> = None;

    for i in 0..effective_len {
        let a = polygon[i];
        let b = polygon[(i + 1) % effective_len];
        let length = rounded_edge_milli(a, b);
        edge_milli.push(length);
        perimeter_milli += length;

        let replace = match longest_edge {
            None => true,
            Some(previous_index) => {
                let previous_length = edge_milli[previous_index];
                if length != previous_length {
                    length > previous_length
                } else {
                    let (low, high) = if a <= b { (a, b) } else { (b, a) };
                    let prev_a = polygon[previous_index];
                    let prev_b = polygon[(previous_index + 1) % effective_len];
                    let (prev_low, prev_high) = if prev_a <= prev_b {
                        (prev_a, prev_b)
                    } else {
                        (prev_b, prev_a)
                    };
                    (low, high, i) < (prev_low, prev_high, previous_index)
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

fn main() {}