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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collinear {
    Exclude,
    Include,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hull {
    pub vertices: Vec<Point>,
    pub source_indices: Vec<usize>,
    pub area2: u128,
    pub perimeter_milli: u64,
}

const LIMIT: i64 = 1_000_000_000;
const MAX_INPUT: usize = 4096;

fn in_range(p: Point) -> bool {
    (-LIMIT..=LIMIT).contains(&p.x) && (-LIMIT..=LIMIT).contains(&p.y)
}

fn prepare_polygon(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > MAX_INPUT {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut effective = points.to_vec();
    if effective.len() >= 2 && effective[0] == *effective.last().unwrap() {
        effective.pop();
    }
    if effective.len() < 3 {
        return Err(GeomError::TooFewVertices {
            len: effective.len(),
        });
    }
    Ok(effective)
}

fn cross(a: Point, b: Point, c: Point) -> i128 {
    let abx = b.x as i128 - a.x as i128;
    let aby = b.y as i128 - a.y as i128;
    let acx = c.x as i128 - a.x as i128;
    let acy = c.y as i128 - a.y as i128;
    abx * acy - aby * acx
}

fn edge_mill(a: Point, b: Point) -> u64 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    let squared = (dx * dx + dy * dy) as u128;
    let scaled = squared * 1_000_000;
    let root = integer_sqrt(scaled);
    let remainder = scaled - root * root;
    let rounded = if remainder * 2 >= root * 2 + 1 {
        root + 1
    } else {
        root
    };
    rounded as u64
}

fn integer_sqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut low = 1u128;
    let mut high = 1u128 << 64;
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if mid <= n / mid {
            low = mid;
        } else {
            high = mid;
        }
    }
    low
}

fn shoelace(points: &[Point]) -> i128 {
    if points.is_empty() {
        return 0;
    }
    let mut sum = 0i128;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }
    sum
}

fn perimeter_and_longest(points: &[Point]) -> (Vec<u64>, u64, Option<usize>) {
    if points.is_empty() {
        return (Vec::new(), 0, None);
    }
    let mut lengths = Vec::with_capacity(points.len());
    let mut perimeter = 0u64;
    let mut longest = None;

    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let length = edge_mill(a, b);
        lengths.push(length);
        perimeter += length;

        let replace = match longest {
            None => true,
            Some(old_index) => {
                let old_length = lengths[old_index];
                if length != old_length {
                    length > old_length
                } else {
                    let pair = if a <= b { (a, b) } else { (b, a) };
                    let old_a = points[old_index];
                    let old_b = points[(old_index + 1) % points.len()];
                    let old_pair = if old_a <= old_b {
                        (old_a, old_b)
                    } else {
                        (old_b, old_a)
                    };
                    pair < old_pair || (pair == old_pair && i < old_index)
                }
            }
        };
        if replace {
            longest = Some(i);
        }
    }

    (lengths, perimeter, longest)
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare_polygon(points)?;
    let signed_area2 = shoelace(&polygon);
    let (edge_milli, perimeter_milli, longest_edge) = perimeter_and_longest(&polygon);
    let orientation = if signed_area2 > 0 {
        Orientation::CounterClockwise
    } else if signed_area2 < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
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

fn classify_one(polygon: &[Point], q: Point) -> Location {
    let mut first_boundary = None;
    let mut inside = false;

    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];

        if on_segment(a, b, q) {
            if first_boundary.is_none() {
                first_boundary = Some(i);
            }
            continue;
        }

        if (a.y > q.y) != (b.y > q.y) {
            let side = cross(a, b, q);
            if (b.y > a.y && side > 0) || (b.y < a.y && side < 0) {
                inside = !inside;
            }
        }
    }

    if let Some(edge) = first_boundary {
        Location::Boundary { edge }
    } else if inside {
        Location::Inside
    } else {
        Location::Outside
    }
}

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let polygon = prepare_polygon(points)?;
    if queries.len() > MAX_INPUT {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (index, &query) in queries.iter().enumerate() {
        if !in_range(query) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    Ok(queries
        .iter()
        .map(|&query| classify_one(&polygon, query))
        .collect())
}

fn all_collinear(points: &[Point]) -> bool {
    points.len() <= 2 || points[2..].iter().all(|&p| cross(points[0], points[1], p) == 0)
}

fn between_on_segment(a: Point, b: Point, p: Point) -> bool {
    cross(a, b, p) == 0
        && p.x >= a.x.min(b.x)
        && p.x <= a.x.max(b.x)
        && p.y >= a.y.min(b.y)
        && p.y <= a.y.max(b.y)
}

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    if points.len() > MAX_INPUT {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (index, &point) in points.iter().enumerate() {
        if let Some(existing) = unique.iter_mut().find(|entry| entry.0 == point) {
            existing.1 = existing.1.min(index);
        } else {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|entry| entry.0);

    let mut hull: Vec<(Point, usize)> = if unique.len() <= 1 {
        unique.clone()
    } else if all_collinear(&unique.iter().map(|entry| entry.0).collect::<Vec<_>>()) {
        if mode == Collinear::Include {
            unique.clone()
        } else {
            vec![unique[0], *unique.last().unwrap()]
        }
    } else {
        let mut lower: Vec<(Point, usize)> = Vec::new();
        for &point in &unique {
            while lower.len() >= 2 {
                let n = lower.len();
                let turn = cross(lower[n - 2].0, lower[n - 1].0, point.0);
                if if mode == Collinear::Include {
                    turn < 0
                } else {
                    turn <= 0
                } {
                    lower.pop();
                } else {
                    break;
                }
            }
            lower.push(point);
        }

        let mut upper: Vec<(Point, usize)> = Vec::new();
        for &point in unique.iter().rev() {
            while upper.len() >= 2 {
                let n = upper.len();
                let turn = cross(upper[n - 2].0, upper[n - 1].0, point.0);
                if if mode == Collinear::Include {
                    turn < 0
                } else {
                    turn <= 0
                } {
                    upper.pop();
                } else {
                    break;
                }
            }
            upper.push(point);
        }

        lower.pop();
        upper.pop();
        lower.extend(upper);
        lower
    };

    // Monotone chain in Include mode can revisit collinear points in a fully
    // collinear cloud; that case was handled separately above.
    let vertices: Vec<Point> = hull.iter().map(|entry| entry.0).collect();
    let source_indices: Vec<usize> = hull.iter().map(|entry| entry.1).collect();
    let signed = shoelace(&vertices);
    let (_, perimeter_milli, _) = perimeter_and_longest(&vertices);

    Ok(Hull {
        vertices,
        source_indices,
        area2: signed.unsigned_abs(),
        perimeter_milli,
    })
}

fn main() {}