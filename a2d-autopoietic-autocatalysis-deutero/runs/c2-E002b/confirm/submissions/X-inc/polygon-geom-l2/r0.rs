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
    TooManyQueries { len: usize },
    QueryOutOfRange { index: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Outside,
    Inside,
    Boundary { edge: usize },
}

const MAX_VERTICES: usize = 4096;
const COORD_LIMIT: i64 = 1_000_000_000;

fn in_range(point: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&point.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&point.y)
}

fn prepare(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }

    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut effective = points.to_vec();
    if effective.len() >= 2 && effective.last() == effective.first() {
        effective.pop();
    }

    if effective.len() < 3 {
        return Err(GeomError::TooFewVertices {
            len: effective.len(),
        });
    }

    Ok(effective)
}

fn cross(a: Point, b: Point, q: Point) -> i128 {
    let abx = b.x as i128 - a.x as i128;
    let aby = b.y as i128 - a.y as i128;
    let aqx = q.x as i128 - a.x as i128;
    let aqy = q.y as i128 - a.y as i128;
    abx * aqy - aby * aqx
}

fn squared_distance(a: Point, b: Point) -> u128 {
    let dx = (b.x as i128 - a.x as i128).unsigned_abs();
    let dy = (b.y as i128 - a.y as i128).unsigned_abs();
    dx * dx + dy * dy
}

// Return the rounded value of sqrt(d2 * 1_000_000). The maximum possible
// result is less than u64::MAX for accepted coordinates.
fn rounded_edge_milli(d2: u128) -> u64 {
    let scaled = d2 * 1_000_000;
    let mut low = 0u128;
    let mut high = 3_000_000_000u128;

    while low < high {
        let mid = low + (high - low + 1) / 2;
        if mid * mid <= scaled {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    let remainder = scaled - low * low;
    if remainder >= 2 * low + 1 {
        (low + 1) as u64
    } else {
        low as u64
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

        signed_area2 += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;

        let length = rounded_edge_milli(squared_distance(a, b));
        edge_milli.push(length);
        perimeter_milli += length;

        let replace = match longest_edge {
            None => true,
            Some(current) if length > edge_milli[current] => true,
            Some(current) if length == edge_milli[current] => {
                let pair = if a <= b { (a, b) } else { (b, a) };
                let old_a = polygon[current];
                let old_b = polygon[(current + 1) % n];
                let old_pair = if old_a <= old_b {
                    (old_a, old_b)
                } else {
                    (old_b, old_a)
                };
                pair < old_pair || (pair == old_pair && i < current)
            }
            _ => false,
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

fn on_segment(a: Point, b: Point, q: Point) -> bool {
    cross(a, b, q) == 0
        && q.x >= a.x.min(b.x)
        && q.x <= a.x.max(b.x)
        && q.y >= a.y.min(b.y)
        && q.y <= a.y.max(b.y)
}

fn classify_one(polygon: &[Point], q: Point) -> Location {
    let n = polygon.len();
    let mut parity = false;

    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];

        if on_segment(a, b, q) {
            return Location::Boundary { edge: i };
        }

        if (a.y > q.y) != (b.y > q.y) {
            let side = cross(a, b, q);
            if (b.y > a.y && side > 0) || (b.y < a.y && side < 0) {
                parity = !parity;
            }
        }
    }

    if parity {
        Location::Inside
    } else {
        Location::Outside
    }
}

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let polygon = prepare(points)?;

    if queries.len() > MAX_VERTICES {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }

    for (index, &query) in queries.iter().enumerate() {
        if !in_range(query) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    Ok(queries
        .iter()
        .map(|&query| classify_one(&polygon, query))
        .collect())
}