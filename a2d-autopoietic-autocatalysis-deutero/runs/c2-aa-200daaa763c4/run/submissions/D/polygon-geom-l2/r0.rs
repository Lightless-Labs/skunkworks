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

const MAX_COORD: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

fn in_range(point: Point) -> bool {
    (-MAX_COORD..=MAX_COORD).contains(&point.x)
        && (-MAX_COORD..=MAX_COORD).contains(&point.y)
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

    let effective_len = points.len()
        - usize::from(points.len() >= 2 && points.last() == points.first());

    if effective_len < 3 {
        return Err(GeomError::TooFewVertices { len: effective_len });
    }

    Ok(&points[..effective_len])
}

// Returns floor(sqrt(value)) without overflowing intermediate arithmetic.
fn integer_sqrt(value: u128) -> u128 {
    if value < 2 {
        return value;
    }

    let mut low = 1_u128;
    let mut high = value / 2 + 1;

    while low < high {
        let mid = low + (high - low + 1) / 2;
        if mid <= value / mid {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    low
}

fn edge_length_milli(a: Point, b: Point) -> u64 {
    let dx = (i128::from(b.x) - i128::from(a.x)).unsigned_abs();
    let dy = (i128::from(b.y) - i128::from(a.y)).unsigned_abs();
    let squared = dx * dx + dy * dy;
    let scaled = squared * 1_000_000;

    let root = integer_sqrt(scaled);
    // The fractional part is at least 1/2 exactly when
    // scaled - root^2 > root.
    let round_up = scaled - root * root > root;
    (root + u128::from(round_up)) as u64
}

fn edge_key(a: Point, b: Point) -> (Point, Point) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare(points)?;
    let n = polygon.len();

    let mut signed_area2 = 0_i128;
    let mut edge_milli = Vec::with_capacity(n);
    let mut perimeter_milli = 0_u64;
    let mut longest_edge = None;

    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];

        signed_area2 += i128::from(a.x) * i128::from(b.y)
            - i128::from(a.y) * i128::from(b.x);

        let length = edge_length_milli(a, b);
        perimeter_milli += length;
        edge_milli.push(length);

        longest_edge = match longest_edge {
            None => Some(i),
            Some(best) if length > edge_milli[best] => Some(i),
            Some(best) if length == edge_milli[best] => {
                if (edge_key(a, b), i)
                    < (
                        edge_key(polygon[best], polygon[(best + 1) % n]),
                        best,
                    )
                {
                    Some(i)
                } else {
                    Some(best)
                }
            }
            Some(best) => Some(best),
        };
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

fn cross(a: Point, b: Point, q: Point) -> i128 {
    (i128::from(b.x) - i128::from(a.x))
        * (i128::from(q.y) - i128::from(a.y))
        - (i128::from(b.y) - i128::from(a.y))
            * (i128::from(q.x) - i128::from(a.x))
}

fn on_segment(a: Point, b: Point, q: Point) -> bool {
    cross(a, b, q) == 0
        && q.x >= a.x.min(b.x)
        && q.x <= a.x.max(b.x)
        && q.y >= a.y.min(b.y)
        && q.y <= a.y.max(b.y)
}

fn locate(polygon: &[Point], q: Point) -> Location {
    let n = polygon.len();

    for i in 0..n {
        if on_segment(polygon[i], polygon[(i + 1) % n], q) {
            return Location::Boundary { edge: i };
        }
    }

    let mut inside = false;
    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];

        if (a.y > q.y) != (b.y > q.y) {
            let side = cross(a, b, q);
            if (b.y > a.y && side > 0) || (b.y < a.y && side < 0) {
                inside = !inside;
            }
        }
    }

    if inside {
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

    Ok(queries.iter().map(|&query| locate(polygon, query)).collect())
}