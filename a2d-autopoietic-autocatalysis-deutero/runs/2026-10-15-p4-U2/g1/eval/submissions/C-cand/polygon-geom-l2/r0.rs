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
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    (dx * dx + dy * dy) as u128
}

// Returns floor(sqrt(n)) using integer arithmetic.
fn integer_sqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }

    let mut low = 1u128;
    let mut high = n / 2 + 1;
    while low <= high {
        let mid = low + (high - low) / 2;
        if mid <= n / mid {
            low = mid + 1;
        } else {
            high = mid - 1;
        }
    }
    high
}

// Round sqrt(n) to the nearest integer, with exact halves rounded upward.
// Compare n against (floor(sqrt(n)) + 1/2)^2 without overflowing u128.
fn rounded_sqrt(n: u128) -> u128 {
    let floor_root = integer_sqrt(n);
    let remainder = n - floor_root * floor_root;

    // The next integer is closer exactly when remainder > floor_root + 1/4.
    // Since remainder and floor_root are integers, this is equivalent to
    // remainder > floor_root. Equality is an exact half and rounds upward.
    if remainder >= floor_root + 1 {
        floor_root + 1
    } else {
        floor_root
    }
}

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    (rounded_sqrt(squared_distance(a, b) * 1_000_000)) as u64
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

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let points = prepare(points)?;
    let n = points.len();

    let mut signed_area2 = 0i128;
    let mut edge_milli = Vec::with_capacity(n);
    let mut perimeter_milli = 0u64;
    let mut longest_edge: Option<usize> = None;

    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];

        signed_area2 += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;

        let length = rounded_edge_milli(a, b);
        edge_milli.push(length);
        perimeter_milli += length;

        let endpoints = if a <= b { (a, b) } else { (b, a) };
        let replace = match longest_edge {
            None => true,
            Some(best) => {
                let best_a = points[best];
                let best_b = points[(best + 1) % n];
                let best_endpoints = if best_a <= best_b {
                    (best_a, best_b)
                } else {
                    (best_b, best_a)
                };

                length > edge_milli[best]
                    || (length == edge_milli[best]
                        && (endpoints < best_endpoints
                            || (endpoints == best_endpoints && i < best)))
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

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let points = prepare(points)?;

    if queries.len() > MAX_VERTICES {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (index, &query) in queries.iter().enumerate() {
        if !in_range(query) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    let n = points.len();
    let mut results = Vec::with_capacity(queries.len());

    for &query in queries {
        let mut boundary_edge = None;
        for i in 0..n {
            if on_segment(points[i], points[(i + 1) % n], query) {
                boundary_edge = Some(i);
                break;
            }
        }

        if let Some(edge) = boundary_edge {
            results.push(Location::Boundary { edge });
            continue;
        }

        let mut inside = false;
        for i in 0..n {
            let a = points[i];
            let b = points[(i + 1) % n];

            if (a.y > query.y) != (b.y > query.y) {
                let determinant = cross(a, b, query);
                let crosses_right = if b.y > a.y {
                    determinant > 0
                } else {
                    determinant < 0
                };
                if crosses_right {
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

fn main() {}