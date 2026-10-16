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

const LIMIT: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

fn prepare(points: &[Point]) -> Result<&[Point], GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }

    for (index, point) in points.iter().enumerate() {
        if point.x < -LIMIT || point.x > LIMIT || point.y < -LIMIT || point.y > LIMIT {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let effective = if points.len() >= 2 && points[0] == points[points.len() - 1] {
        &points[..points.len() - 1]
    } else {
        points
    };

    if effective.len() < 3 {
        return Err(GeomError::TooFewVertices { len: effective.len() });
    }

    Ok(effective)
}

fn squared_distance(a: Point, b: Point) -> u128 {
    let dx = (i128::from(b.x) - i128::from(a.x)).unsigned_abs();
    let dy = (i128::from(b.y) - i128::from(a.y)).unsigned_abs();
    dx * dx + dy * dy
}

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let target = squared_distance(a, b) * 1_000_000;
    let mut lo = 0u128;
    let mut hi = 3_000_000_000_000u128;

    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if mid * mid < target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }

    let ceil = lo;
    if ceil * ceil == target {
        return ceil as u64;
    }

    let floor = ceil - 1;
    let threshold = floor * floor + floor + 1;
    if target >= threshold {
        ceil as u64
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

        let endpoints = if a <= b { (a, b) } else { (b, a) };
        let should_replace = match longest_edge {
            None => true,
            Some(old_index) => {
                if length != edge_milli[old_index] {
                    length > edge_milli[old_index]
                } else {
                    let old_a = polygon[old_index];
                    let old_b = polygon[(old_index + 1) % n];
                    let old_endpoints = if old_a <= old_b {
                        (old_a, old_b)
                    } else {
                        (old_b, old_a)
                    };
                    endpoints < old_endpoints
                        || (endpoints == old_endpoints && i < old_index)
                }
            }
        };
        if should_replace {
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
    (i128::from(b.x) - i128::from(a.x)) * (i128::from(q.y) - i128::from(a.y))
        - (i128::from(b.y) - i128::from(a.y)) * (i128::from(q.x) - i128::from(a.x))
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
    for (index, query) in queries.iter().enumerate() {
        if query.x < -LIMIT || query.x > LIMIT || query.y < -LIMIT || query.y > LIMIT {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    let mut results = Vec::with_capacity(queries.len());
    for &query in queries {
        let mut boundary_edge = None;
        for i in 0..polygon.len() {
            if on_segment(polygon[i], polygon[(i + 1) % polygon.len()], query) {
                boundary_edge = Some(i);
                break;
            }
        }

        if let Some(edge) = boundary_edge {
            results.push(Location::Boundary { edge });
            continue;
        }

        let mut inside = false;
        for i in 0..polygon.len() {
            let a = polygon[i];
            let b = polygon[(i + 1) % polygon.len()];
            if (a.y > query.y) != (b.y > query.y) {
                let side = cross(a, b, query);
                if (b.y > a.y && side > 0) || (b.y < a.y && side < 0) {
                    inside = !inside;
                }
            }
        }

        results.push(if inside {
            Location::Inside
        } else {
            Location::Outside
        });
    }

    Ok(results)
}