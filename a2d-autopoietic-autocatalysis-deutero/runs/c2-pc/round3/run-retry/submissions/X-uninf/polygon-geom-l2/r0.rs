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

const MAX_VERTICES: usize = 4096;
const COORD_LIMIT: i64 = 1_000_000_000;

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

    let effective_len = if points.len() >= 2 && points[0] == points[points.len() - 1] {
        points.len() - 1
    } else {
        points.len()
    };
    if effective_len < 3 {
        return Err(GeomError::TooFewVertices { len: effective_len });
    }
    Ok(&points[..effective_len])
}

fn squared_distance(a: Point, b: Point) -> u128 {
    let dx = (i128::from(b.x) - i128::from(a.x)).unsigned_abs();
    let dy = (i128::from(b.y) - i128::from(a.y)).unsigned_abs();
    dx * dx + dy * dy
}

fn rounded_edge_milli(d2: u128) -> u64 {
    let scaled = d2 * 1_000_000;
    let mut root = isqrt(scaled);
    let remainder = scaled - root * root;
    if remainder >= root + 1 {
        root += 1;
    }
    root as u64
}

fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
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

        let length = rounded_edge_milli(squared_distance(a, b));
        edge_milli.push(length);
        perimeter_milli += length;

        let replace = match longest_edge {
            None => true,
            Some(old) if length > edge_milli[old] => true,
            Some(old) if length < edge_milli[old] => false,
            Some(old) => {
                let old_a = polygon[old];
                let old_b = polygon[(old + 1) % n];
                let pair = (a.min(b), a.max(b));
                let old_pair = (old_a.min(old_b), old_a.max(old_b));
                pair < old_pair || (pair == old_pair && i < old)
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

fn cross(a: Point, b: Point, q: Point) -> i128 {
    (i128::from(b.x) - i128::from(a.x)) * (i128::from(q.y) - i128::from(a.y))
        - (i128::from(b.y) - i128::from(a.y)) * (i128::from(q.x) - i128::from(a.x))
}

fn on_segment(a: Point, b: Point, q: Point) -> bool {
    cross(a, b, q) == 0
        && i128::from(q.x) >= i128::from(a.x.min(b.x))
        && i128::from(q.x) <= i128::from(a.x.max(b.x))
        && i128::from(q.y) >= i128::from(a.y.min(b.y))
        && i128::from(q.y) <= i128::from(a.y.max(b.y))
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
    let mut result = Vec::with_capacity(queries.len());
    for &q in queries {
        let mut boundary = None;
        for i in 0..n {
            if on_segment(polygon[i], polygon[(i + 1) % n], q) {
                boundary = Some(i);
                break;
            }
        }
        if let Some(edge) = boundary {
            result.push(Location::Boundary { edge });
            continue;
        }

        let mut inside = false;
        for i in 0..n {
            let a = polygon[i];
            let b = polygon[(i + 1) % n];
            if (a.y > q.y) != (b.y > q.y) {
                let determinant = cross(a, b, q);
                if (b.y > a.y && determinant > 0) || (b.y < a.y && determinant < 0) {
                    inside = !inside;
                }
            }
        }
        result.push(if inside {
            Location::Inside
        } else {
            Location::Outside
        });
    }
    Ok(result)
}