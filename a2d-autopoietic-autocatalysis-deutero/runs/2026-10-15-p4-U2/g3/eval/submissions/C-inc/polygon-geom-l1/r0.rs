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

fn prepare(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > 4096 {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }

    for (index, point) in points.iter().enumerate() {
        if !(-1_000_000_000..=1_000_000_000).contains(&point.x)
            || !(-1_000_000_000..=1_000_000_000).contains(&point.y)
        {
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

    Ok(points[..effective_len].to_vec())
}

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let dx = (b.x as i128 - a.x as i128).unsigned_abs();
    let dy = (b.y as i128 - a.y as i128).unsigned_abs();
    let squared = dx * dx + dy * dy;
    let target = squared * 1_000_000;

    // Find floor(sqrt(target)) exactly with integer arithmetic.
    let mut low = 0_u64;
    let mut high = 2_000_000_001_u64;
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if (mid as u128) * (mid as u128) <= target {
            low = mid;
        } else {
            high = mid;
        }
    }

    let floor = low as u128;
    let remainder = target - floor * floor;
    let round_up = remainder * 4 >= floor * 4 + 1;
    (floor + u128::from(round_up)) as u64
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare(points)?;
    let n = polygon.len();
    let mut signed_area2 = 0_i128;
    let mut edge_milli = Vec::with_capacity(n);
    let mut perimeter_milli = 0_u64;
    let mut longest_edge: Option<usize> = None;

    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];

        signed_area2 += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;

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
                    let (low, high) = if a <= b { (a, b) } else { (b, a) };
                    let best_a = polygon[best_index];
                    let best_b = polygon[(best_index + 1) % n];
                    let (best_low, best_high) = if best_a <= best_b {
                        (best_a, best_b)
                    } else {
                        (best_b, best_a)
                    };
                    (low, high, i) < (best_low, best_high, best_index)
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