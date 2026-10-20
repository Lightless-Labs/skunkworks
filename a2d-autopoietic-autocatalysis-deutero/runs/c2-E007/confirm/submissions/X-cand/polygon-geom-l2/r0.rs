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

const COORD_LIMIT: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

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

    let effective = if points.len() >= 2 && points.last() == points.first() {
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

fn edge_distance_milli(a: Point, b: Point) -> u64 {
    let dx = (b.x as i128 - a.x as i128).unsigned_abs();
    let dy = (b.y as i128 - a.y as i128).unsigned_abs();
    let scaled_squared = (dx * dx + dy * dy) * 1_000_000_u128;

    let mut lo = 0_u128;
    let mut hi = 3_000_000_000_000_u128;
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if mid * mid <= scaled_squared {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }

    let remainder = scaled_squared - lo * lo;
    if remainder * 2 >= lo * 2 + 1 {
        (lo + 1) as u64
    } else {
        lo as u64
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let points = prepare(points)?;
    let n = points.len();

    let mut signed_area2 = 0_i128;
    let mut edge_milli = Vec::with_capacity(n);
    let mut perimeter_milli = 0_u64;
    let mut longest_edge: Option<usize> = None;

    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        signed_area2 += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;

        let length = edge_distance_milli(a, b);
        edge_milli.push(length);
        perimeter_milli += length;

        let replace = match longest_edge {
            None => true,
            Some(current) if length > edge_milli[current] => true,
            Some(current) if length < edge_milli[current] => false,
            Some(current) => {
                let pair = (a.min(b), a.max(b));
                let old_a = points[current];
                let old_b = points[(current + 1) % n];
                let old_pair = (old_a.min(old_b), old_a.max(old_b));
                pair < old_pair || (pair == old_pair && i < current)
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
    let points = prepare(points)?;

    if queries.len() > MAX_VERTICES {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (index, query) in queries.iter().enumerate() {
        if query.x < -COORD_LIMIT
            || query.x > COORD_LIMIT
            || query.y < -COORD_LIMIT
            || query.y > COORD_LIMIT
        {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    let n = points.len();
    let mut results = Vec::with_capacity(queries.len());

    for &query in queries {
        let mut boundary = None;
        for i in 0..n {
            if on_segment(points[i], points[(i + 1) % n], query) {
                boundary = Some(i);
                break;
            }
        }

        if let Some(edge) = boundary {
            results.push(Location::Boundary { edge });
            continue;
        }

        let mut inside = false;
        for i in 0..n {
            let a = points[i];
            let b = points[(i + 1) % n];
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