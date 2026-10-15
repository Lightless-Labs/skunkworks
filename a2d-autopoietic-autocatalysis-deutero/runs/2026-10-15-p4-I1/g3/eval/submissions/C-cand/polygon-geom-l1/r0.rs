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

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let dx = (b.x as i128 - a.x as i128).unsigned_abs();
    let dy = (b.y as i128 - a.y as i128).unsigned_abs();

    // The coordinate bounds make this multiplication and sum fit in u128.
    let squared = (dx * dx + dy * dy) * 1_000_000;

    // Integer square root, rounded to nearest with exact half ties upward.
    let mut low = 0u128;
    let mut high = squared + 1;
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if mid <= squared / mid {
            low = mid;
        } else {
            high = mid;
        }
    }

    let remainder = squared - low * low;
    if remainder >= low + 1 {
        (low + 1) as u64
    } else {
        low as u64
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let raw_len = points.len();
    if raw_len > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: raw_len });
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

    let effective_len = if raw_len >= 2 && points[raw_len - 1] == points[0] {
        raw_len - 1
    } else {
        raw_len
    };
    if effective_len < 3 {
        return Err(GeomError::TooFewVertices { len: effective_len });
    }

    let polygon = &points[..effective_len];
    let mut signed_area2 = 0i128;
    let mut edge_milli = Vec::with_capacity(effective_len);

    for i in 0..effective_len {
        let a = polygon[i];
        let b = polygon[(i + 1) % effective_len];
        signed_area2 += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
        edge_milli.push(rounded_edge_milli(a, b));
    }

    let mut perimeter_milli = 0u64;
    let mut longest_edge = None;
    for (i, &length) in edge_milli.iter().enumerate() {
        perimeter_milli += length;
        let Some(current) = longest_edge else {
            longest_edge = Some(i);
            continue;
        };

        let current_length = edge_milli[current];
        let endpoints = {
            let a = polygon[i];
            let b = polygon[(i + 1) % effective_len];
            if a <= b { (a, b) } else { (b, a) }
        };
        let current_endpoints = {
            let a = polygon[current];
            let b = polygon[(current + 1) % effective_len];
            if a <= b { (a, b) } else { (b, a) }
        };

        if length > current_length
            || (length == current_length
                && (endpoints < current_endpoints
                    || (endpoints == current_endpoints && i < current)))
        {
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