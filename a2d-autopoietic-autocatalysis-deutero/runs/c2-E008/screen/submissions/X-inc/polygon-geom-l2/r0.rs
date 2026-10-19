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

fn in_range(p: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&p.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&p.y)
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

    let effective_len = points.len()
        - usize::from(points.len() >= 2 && points.first() == points.last());

    if effective_len < 3 {
        return Err(GeomError::TooFewVertices { len: effective_len });
    }

    Ok(points[..effective_len].to_vec())
}

fn cross(a: Point, b: Point, q: Point) -> i128 {
    let abx = b.x as i128 - a.x as i128;
    let aby = b.y as i128 - a.y as i128;
    let aqx = q.x as i128 - a.x as i128;
    let aqy = q.y as i128 - a.y as i128;
    abx * aqy - aby * aqx
}

fn squared_distance(a: Point, b: Point) -> u128 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    (dx * dx + dy * dy) as u128
}

fn rounded_sqrt(value: u128) -> u64 {
    let mut lo = 0u128;
    let mut hi = value.min(u64::MAX as u128).saturating_add(1);

    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if mid <= value / mid {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    let remainder = value - lo * lo;
    if remainder >= lo + 1 {
        (lo + 1) as u64
    } else {
        lo as u64
    }
}

fn edge_length_milli(a: Point, b: Point) -> u64 {
    rounded_sqrt(squared_distance(a, b) * 1_000_000)
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

        let length = edge_length_milli(a, b);
        edge_milli.push(length);
        perimeter_milli += length;

        let replace = match longest_edge {
            None => true,
            Some(best) if length > edge_milli[best] => true,
            Some(best) if length < edge_milli[best] => false,
            Some(best) => {
                let current_pair = if a <= b { (a, b) } else { (b, a) };
                let best_a = polygon[best];
                let best_b = polygon[(best + 1) % n];
                let best_pair = if best_a <= best_b {
                    (best_a, best_b)
                } else {
                    (best_b, best_a)
                };
                current_pair < best_pair || (current_pair == best_pair && i < best)
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
        let mut inside = false;

        for i in 0..n {
            let a = polygon[i];
            let b = polygon[(i + 1) % n];

            if on_segment(a, b, query) {
                boundary = Some(i);
                break;
            }

            if (a.y > query.y) != (b.y > query.y) {
                let side = cross(a, b, query);
                if (b.y > a.y && side > 0) || (b.y < a.y && side < 0) {
                    inside = !inside;
                }
            }
        }

        results.push(match boundary {
            Some(edge) => Location::Boundary { edge },
            None if inside => Location::Inside,
            None => Location::Outside,
        });
    }

    Ok(results)
}