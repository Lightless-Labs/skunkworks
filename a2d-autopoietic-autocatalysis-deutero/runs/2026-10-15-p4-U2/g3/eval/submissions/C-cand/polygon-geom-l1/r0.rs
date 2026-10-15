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

fn is_valid_coordinate(value: i64) -> bool {
    (-1_000_000_000..=1_000_000_000).contains(&value)
}

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let dx = (b.x as i128 - a.x as i128).unsigned_abs();
    let dy = (b.y as i128 - a.y as i128).unsigned_abs();
    let squared = dx * dx + dy * dy;
    let scaled = squared * 1_000_000;

    // The valid coordinate range bounds the rounded result below 3e9.
    let mut low = 0u128;
    let mut high = 3_000_000_001u128;
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if mid * mid <= scaled {
            low = mid;
        } else {
            high = mid;
        }
    }

    // Compare against (low + 1/2)^2 without floating-point arithmetic.
    if scaled - low * low >= low + 1 {
        (low + 1) as u64
    } else {
        low as u64
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    if points.len() > 4096 {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }

    for (index, point) in points.iter().enumerate() {
        if !is_valid_coordinate(point.x) || !is_valid_coordinate(point.y) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let effective_len = if points.len() >= 2 && points.last() == points.first() {
        points.len() - 1
    } else {
        points.len()
    };

    if effective_len < 3 {
        return Err(GeomError::TooFewVertices { len: effective_len });
    }

    let polygon = &points[..effective_len];
    let mut signed_area2 = 0i128;
    let mut edge_milli = Vec::with_capacity(effective_len);
    let mut perimeter_milli = 0u64;
    let mut longest_edge: Option<usize> = None;

    for i in 0..effective_len {
        let a = polygon[i];
        let b = polygon[(i + 1) % effective_len];

        signed_area2 += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;

        let length = rounded_edge_milli(a, b);
        edge_milli.push(length);
        perimeter_milli += length;

        let replace = match longest_edge {
            None => true,
            Some(best_index) => {
                if length > edge_milli[best_index] {
                    true
                } else if length < edge_milli[best_index] {
                    false
                } else {
                    let current_pair = if a <= b { (a, b) } else { (b, a) };
                    let best_a = polygon[best_index];
                    let best_b = polygon[(best_index + 1) % effective_len];
                    let best_pair = if best_a <= best_b {
                        (best_a, best_b)
                    } else {
                        (best_b, best_a)
                    };
                    current_pair < best_pair || (current_pair == best_pair && i < best_index)
                }
            }
        };

        if replace {
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