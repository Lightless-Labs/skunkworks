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

const LIMIT: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

fn in_range(p: Point) -> bool {
    (-LIMIT..=LIMIT).contains(&p.x) && (-LIMIT..=LIMIT).contains(&p.y)
}

fn prepare_polygon(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (i, &p) in points.iter().enumerate() {
        if !in_range(p) {
            return Err(GeomError::OutOfRange { index: i });
        }
    }
    let mut effective = points.to_vec();
    if effective.len() >= 2 && effective[effective.len() - 1] == effective[0] {
        effective.pop();
    }
    if effective.len() < 3 {
        return Err(GeomError::TooFewVertices {
            len: effective.len(),
        });
    }
    Ok(effective)
}

fn cross_vec(ax: i128, ay: i128, bx: i128, by: i128) -> i128 {
    ax * by - ay * bx
}

fn cross_points(a: Point, b: Point, q: Point) -> i128 {
    cross_vec(
        b.x as i128 - a.x as i128,
        b.y as i128 - a.y as i128,
        q.x as i128 - a.x as i128,
        q.y as i128 - a.y as i128,
    )
}

fn squared_distance(a: Point, b: Point) -> u128 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    (dx * dx + dy * dy) as u128
}

fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut lo = 1u128;
    let mut hi = 1u128 << 64;
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if mid <= n / mid {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

fn edge_length_milli(a: Point, b: Point) -> u64 {
    let scaled = squared_distance(a, b) * 1_000_000u128;
    let root = isqrt(scaled);
    let remainder = scaled - root * root;
    let rounded = if remainder >= root + 1 { root + 1 } else { root };
    rounded as u64
}

fn polygon_measures(points: &[Point]) -> Measures {
    let n = points.len();
    let mut signed_area2 = 0i128;
    let mut edge_milli = Vec::with_capacity(n);
    let mut perimeter_milli = 0u64;
    let mut longest_edge = None;

    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        signed_area2 += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;

        let length = edge_length_milli(a, b);
        edge_milli.push(length);
        perimeter_milli += length;

        let replace = match longest_edge {
            None => true,
            Some(j) => {
                if length != edge_milli[j] {
                    length > edge_milli[j]
                } else {
                    let old_a = points[j];
                    let old_b = points[(j + 1) % n];
                    let (new_low, new_high) = if a <= b { (a, b) } else { (b, a) };
                    let (old_low, old_high) = if old_a <= old_b {
                        (old_a, old_b)
                    } else {
                        (old_b, old_a)
                    };
                    (new_low, new_high, i) < (old_low, old_high, j)
                }
            }
        };
        if replace {
            longest_edge = Some(i);
        }
    }

    let area2 = signed_area2.unsigned_abs();
    let orientation = if signed_area2 > 0 {
        Orientation::CounterClockwise
    } else if signed_area2 < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
    };

    Measures {
        signed_area2,
        area2,
        orientation,
        edge_milli,
        perimeter_milli,
        longest_edge,
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let effective = prepare_polygon(points)?;
    Ok(polygon_measures(&effective))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Outside,
    Inside,
    Boundary { edge: usize },
}

fn point_on_segment(a: Point, b: Point, q: Point) -> bool {
    cross_points(a, b, q) == 0
        && (q.x as i128) >= (a.x.min(b.x) as i128)
        && (q.x as i128) <= (a.x.max(b.x) as i128)
        && (q.y as i128) >= (a.y.min(b.y) as i128)
        && (q.y as i128) <= (a.y.max(b.y) as i128)
}

fn classify_one(points: &[Point], q: Point) -> Location {
    let n = points.len();

    for i in 0..n {
        if point_on_segment(points[i], points[(i + 1) % n], q) {
            return Location::Boundary { edge: i };
        }
    }

    let mut odd = false;
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        if (a.y > q.y) != (b.y > q.y) {
            let cross = cross_points(a, b, q);
            if (b.y > a.y && cross > 0) || (b.y < a.y && cross < 0) {
                odd = !odd;
            }
        }
    }
    if odd {
        Location::Inside
    } else {
        Location::Outside
    }
}

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let effective = prepare_polygon(points)?;
    if queries.len() > MAX_VERTICES {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (i, &q) in queries.iter().enumerate() {
        if !in_range(q) {
            return Err(GeomError::QueryOutOfRange { index: i });
        }
    }
    Ok(queries
        .iter()
        .copied()
        .map(|q| classify_one(&effective, q))
        .collect())
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

fn turn(a: Point, b: Point, c: Point) -> i128 {
    cross_points(a, b, c)
}

fn all_collinear(sorted: &[Point]) -> bool {
    sorted.len() < 3 || sorted[0..1]
        .iter()
        .all(|_| turn(sorted[0], sorted[sorted.len() - 1], sorted[1]) == 0)
}

fn monotone_chain(sorted: &[Point], include_collinear: bool) -> Vec<Point> {
    if sorted.len() <= 1 {
        return sorted.to_vec();
    }
    let mut lower = Vec::new();
    for &p in sorted {
        while lower.len() >= 2 {
            let t = turn(lower[lower.len() - 2], lower[lower.len() - 1], p);
            if if include_collinear { t < 0 } else { t <= 0 } {
                lower.pop();
            } else {
                break;
            }
        }
        lower.push(p);
    }

    let mut upper = Vec::new();
    for &p in sorted.iter().rev() {
        while upper.len() >= 2 {
            let t = turn(upper[upper.len() - 2], upper[upper.len() - 1], p);
            if if include_collinear { t < 0 } else { t <= 0 } {
                upper.pop();
            } else {
                break;
            }
        }
        upper.push(p);
    }

    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (i, &p) in points.iter().enumerate() {
        if !in_range(p) {
            return Err(GeomError::OutOfRange { index: i });
        }
    }

    let mut first_seen = std::collections::BTreeMap::<Point, usize>::new();
    for (i, &p) in points.iter().enumerate() {
        first_seen.entry(p).or_insert(i);
    }
    let sorted: Vec<Point> = first_seen.keys().copied().collect();

    let vertices = if sorted.len() <= 2 {
        sorted.clone()
    } else if all_collinear(&sorted) {
        match mode {
            Collinear::Include => sorted.clone(),
            Collinear::Exclude => vec![sorted[0], sorted[sorted.len() - 1]],
        }
    } else {
        monotone_chain(&sorted, mode == Collinear::Include)
    };

    let source_indices = vertices.iter().map(|p| first_seen[p]).collect();
    let n = vertices.len();
    let mut signed_area = 0i128;
    let mut perimeter_milli = 0u64;
    if n >= 2 {
        for i in 0..n {
            let a = vertices[i];
            let b = vertices[(i + 1) % n];
            signed_area += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
            perimeter_milli += edge_length_milli(a, b);
        }
    }
    Ok(Hull {
        vertices,
        source_indices,
        area2: signed_area.unsigned_abs(),
        perimeter_milli,
    })
}
