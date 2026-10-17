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

fn prepare(points: &[Point]) -> Result<Vec<Point>, GeomError> {
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

fn rounded_sqrt(n: u128) -> u64 {
    let mut lo = 0u128;
    let mut hi = 1u128;
    while hi * hi < n {
        hi *= 2;
    }
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if mid * mid <= n {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let remainder = n - lo * lo;
    if remainder >= lo + 1 {
        (lo + 1) as u64
    } else {
        lo as u64
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let vertices = prepare(points)?;
    let n = vertices.len();
    let mut signed_area2 = 0i128;
    let mut edge_milli = Vec::with_capacity(n);
    let mut perimeter_milli = 0u64;

    for i in 0..n {
        let a = vertices[i];
        let b = vertices[(i + 1) % n];
        signed_area2 += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;

        let scaled_d2 = squared_distance(a, b) * 1_000_000;
        let length = rounded_sqrt(scaled_d2);
        edge_milli.push(length);
        perimeter_milli += length;
    }

    let longest_edge = (0..n).max_by(|&i, &j| {
        edge_milli[i]
            .cmp(&edge_milli[j])
            .then_with(|| {
                let edge_pair = |k: usize| {
                    let a = vertices[k];
                    let b = vertices[(k + 1) % n];
                    if a <= b { (a, b) } else { (b, a) }
                };
                edge_pair(j).cmp(&edge_pair(i))
            })
            .then_with(|| j.cmp(&i))
    });

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

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let vertices = prepare(points)?;

    if queries.len() > MAX_VERTICES {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (index, &query) in queries.iter().enumerate() {
        if !in_range(query) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    let n = vertices.len();
    let mut results = Vec::with_capacity(queries.len());

    for &q in queries {
        let mut boundary_edge = None;
        let mut inside = false;

        for i in 0..n {
            let a = vertices[i];
            let b = vertices[(i + 1) % n];

            if cross(a, b, q) == 0
                && (a.x.min(b.x)..=a.x.max(b.x)).contains(&q.x)
                && (a.y.min(b.y)..=a.y.max(b.y)).contains(&q.y)
            {
                boundary_edge = Some(i);
                break;
            }

            if (a.y > q.y) != (b.y > q.y) {
                let side = cross(a, b, q);
                if (b.y > a.y && side > 0) || (b.y < a.y && side < 0) {
                    inside = !inside;
                }
            }
        }

        results.push(match boundary_edge {
            Some(edge) => Location::Boundary { edge },
            None if inside => Location::Inside,
            None => Location::Outside,
        });
    }

    Ok(results)
}