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

const MAX_COORD: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

fn in_range(p: Point) -> bool {
    (-MAX_COORD..=MAX_COORD).contains(&p.x) && (-MAX_COORD..=MAX_COORD).contains(&p.y)
}

fn prepare(points: &[Point]) -> Result<&[Point], GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
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

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let dx = (b.x as i128 - a.x as i128).unsigned_abs();
    let dy = (b.y as i128 - a.y as i128).unsigned_abs();
    let squared = dx * dx + dy * dy;
    let scaled = squared * 1_000_000;

    // Integer square root, rounded to nearest with exact half ties upward.
    let mut lo = 0u128;
    let mut hi = 2_000_000_000_001u128;
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if mid * mid <= scaled {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let remainder = scaled - lo * lo;
    if remainder >= lo + 1 {
        (lo + 1) as u64
    } else {
        lo as u64
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

        let length = rounded_edge_milli(a, b);
        edge_milli.push(length);
        perimeter_milli += length;

        let replace = match longest_edge {
            None => true,
            Some(current) if length > edge_milli[current] => true,
            Some(current) if length < edge_milli[current] => false,
            Some(current) => {
                let current_a = polygon[current];
                let current_b = polygon[(current + 1) % n];
                let candidate_pair = (a.min(b), a.max(b));
                let current_pair = (current_a.min(current_b), current_a.max(current_b));
                candidate_pair < current_pair
                    || (candidate_pair == current_pair && i < current)
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

fn cross(a: Point, b: Point, q: Point) -> i128 {
    (b.x as i128 - a.x as i128) * (q.y as i128 - a.y as i128)
        - (b.y as i128 - a.y as i128) * (q.x as i128 - a.x as i128)
}

fn on_segment(a: Point, b: Point, q: Point) -> bool {
    cross(a, b, q) == 0
        && q.x >= a.x.min(b.x)
        && q.x <= a.x.max(b.x)
        && q.y >= a.y.min(b.y)
        && q.y <= a.y.max(b.y)
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

    let n = polygon.len();
    let mut results = Vec::with_capacity(queries.len());

    for &query in queries {
        let mut boundary = None;
        for i in 0..n {
            if on_segment(polygon[i], polygon[(i + 1) % n], query) {
                boundary = Some(i);
                break;
            }
        }
        if let Some(edge) = boundary {
            results.push(Location::Boundary { edge });
            continue;
        }

        let mut odd = false;
        for i in 0..n {
            let a = polygon[i];
            let b = polygon[(i + 1) % n];
            if (a.y > query.y) != (b.y > query.y) {
                let c = cross(a, b, query);
                if (b.y > a.y && c > 0) || (b.y < a.y && c < 0) {
                    odd = !odd;
                }
            }
        }
        results.push(if odd {
            Location::Inside
        } else {
            Location::Outside
        });
    }

    Ok(results)
}