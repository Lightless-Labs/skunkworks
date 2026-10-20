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

const COORD_LIMIT: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

fn in_range(p: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&p.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&p.y)
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

fn cross(a: Point, b: Point, q: Point) -> i128 {
    let ab_x = b.x as i128 - a.x as i128;
    let ab_y = b.y as i128 - a.y as i128;
    let aq_x = q.x as i128 - a.x as i128;
    let aq_y = q.y as i128 - a.y as i128;
    ab_x * aq_y - ab_y * aq_x
}

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    let scaled_squared = ((dx * dx + dy * dy) as u128) * 1_000_000;

    let mut root = scaled_squared.isqrt();
    let lower_error = scaled_squared - root * root;
    let next = root + 1;
    let upper_error = next * next - scaled_squared;
    if lower_error >= upper_error {
        root = next;
    }

    root as u64
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

        let pair = if a <= b { (a, b) } else { (b, a) };
        let replace = match longest_edge {
            None => true,
            Some(current) => {
                let current_a = polygon[current];
                let current_b = polygon[(current + 1) % n];
                let current_pair = if current_a <= current_b {
                    (current_a, current_b)
                } else {
                    (current_b, current_a)
                };

                length > edge_milli[current]
                    || (length == edge_milli[current]
                        && (pair < current_pair || (pair == current_pair && i < current)))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Outside,
    Inside,
    Boundary { edge: usize },
}

fn on_segment(a: Point, b: Point, q: Point) -> bool {
    cross(a, b, q) == 0
        && (a.x.min(b.x)..=a.x.max(b.x)).contains(&q.x)
        && (a.y.min(b.y)..=a.y.max(b.y)).contains(&q.y)
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
                let edge_cross = cross(a, b, query);
                if (b.y > a.y && edge_cross > 0) || (b.y < a.y && edge_cross < 0) {
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