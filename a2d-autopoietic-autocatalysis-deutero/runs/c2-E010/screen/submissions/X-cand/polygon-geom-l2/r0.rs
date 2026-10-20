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
const SCALE: u128 = 1_000_000;

fn in_range(p: Point) -> bool {
    (-LIMIT..=LIMIT).contains(&p.x) && (-LIMIT..=LIMIT).contains(&p.y)
}

fn prepare(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > 4096 {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let end = if points.len() >= 2 && points.last() == points.first() {
        points.len() - 1
    } else {
        points.len()
    };
    if end < 3 {
        return Err(GeomError::TooFewVertices { len: end });
    }
    Ok(points[..end].to_vec())
}

fn rounded_sqrt(n: u128) -> u128 {
    // Integer square root, rounded to nearest with exact half ties upward.
    let mut lo = 0u128;
    let mut hi = 1u128;
    while hi <= n / hi {
        hi *= 2;
    }
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if mid <= n / mid {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    // Compare n with (lo + 1/2)^2 without floating point.
    let threshold = lo * lo + lo;
    if n >= threshold + 1 {
        lo + 1
    } else {
        lo
    }
}

fn edge_length_milli(a: Point, b: Point) -> u64 {
    let dx = (b.x as i128 - a.x as i128).unsigned_abs();
    let dy = (b.y as i128 - a.y as i128).unsigned_abs();
    let scaled_sq = (dx * dx + dy * dy) * SCALE;
    rounded_sqrt(scaled_sq) as u64
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare(points)?;
    let n = polygon.len();
    let mut signed_area2 = 0i128;
    let mut edge_milli = Vec::with_capacity(n);

    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];
        signed_area2 += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
        edge_milli.push(edge_length_milli(a, b));
    }

    let perimeter_milli = edge_milli.iter().copied().sum();
    let mut longest_edge = Some(0usize);
    for i in 1..n {
        let best = longest_edge.unwrap();
        let endpoints_i = if polygon[i] <= polygon[(i + 1) % n] {
            (polygon[i], polygon[(i + 1) % n])
        } else {
            (polygon[(i + 1) % n], polygon[i])
        };
        let endpoints_best = if polygon[best] <= polygon[(best + 1) % n] {
            (polygon[best], polygon[(best + 1) % n])
        } else {
            (polygon[(best + 1) % n], polygon[best])
        };
        if edge_milli[i] > edge_milli[best]
            || (edge_milli[i] == edge_milli[best]
                && (endpoints_i < endpoints_best
                    || (endpoints_i == endpoints_best && i < best)))
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
    if queries.len() > 4096 {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (index, &query) in queries.iter().enumerate() {
        if !in_range(query) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    let n = polygon.len();
    let mut results = Vec::with_capacity(queries.len());
    for &q in queries {
        let mut boundary = None;
        for i in 0..n {
            if on_segment(polygon[i], polygon[(i + 1) % n], q) {
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
            let a = polygon[i];
            let b = polygon[(i + 1) % n];
            if (a.y > q.y) != (b.y > q.y) {
                let c = cross(a, b, q);
                if (b.y > a.y && c > 0) || (b.y < a.y && c < 0) {
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
