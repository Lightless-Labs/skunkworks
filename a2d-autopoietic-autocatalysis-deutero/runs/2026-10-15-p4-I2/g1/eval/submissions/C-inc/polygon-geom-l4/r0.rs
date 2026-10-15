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
    Duplicate { first: usize, second: usize },
    ZeroArea,
    AdjacentOverlap { vertex: usize },
    Intersection { edge_a: usize, edge_b: usize },
}

const LIMIT: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

fn in_range(p: Point) -> bool {
    (-LIMIT..=LIMIT).contains(&p.x) && (-LIMIT..=LIMIT).contains(&p.y)
}

fn prepare(points: &[Point]) -> Result<Vec<(Point, usize)>, GeomError> {
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

    Ok(points[..effective_len]
        .iter()
        .copied()
        .enumerate()
        .map(|(index, point)| (point, index))
        .collect())
}

fn cross(a: Point, b: Point, c: Point) -> i128 {
    (b.x as i128 - a.x as i128) * (c.y as i128 - a.y as i128)
        - (b.y as i128 - a.y as i128) * (c.x as i128 - a.x as i128)
}

fn signed_area2(points: &[Point]) -> i128 {
    let mut sum = 0i128;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }
    sum
}

fn rounded_sqrt_scaled(d2: u128) -> u64 {
    let value = d2 * 1_000_000;
    let mut lo = 0u128;
    let mut hi = 1u128 << 64;
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if mid * mid <= value {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let lower_error = value - lo * lo;
    let upper_error = hi * hi - value;
    let rounded = if upper_error <= lower_error { hi } else { lo };
    rounded as u64
}

fn edge_length_milli(a: Point, b: Point) -> u64 {
    let dx = a.x as i128 - b.x as i128;
    let dy = a.y as i128 - b.y as i128;
    rounded_sqrt_scaled((dx * dx + dy * dy) as u128)
}

fn measures_for(points: &[Point]) -> Measures {
    let signed = signed_area2(points);
    let orientation = if signed > 0 {
        Orientation::CounterClockwise
    } else if signed < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
    };

    let edge_milli: Vec<u64> = (0..points.len())
        .map(|i| edge_length_milli(points[i], points[(i + 1) % points.len()]))
        .collect();
    let perimeter_milli = edge_milli.iter().copied().sum();

    let longest_edge = if edge_milli.is_empty() {
        None
    } else {
        let mut best = 0usize;
        for i in 1..edge_milli.len() {
            if edge_milli[i] > edge_milli[best]
                || (edge_milli[i] == edge_milli[best]
                    && edge_key(points, i) < edge_key(points, best))
            {
                best = i;
            }
        }
        Some(best)
    };

    Measures {
        signed_area2: signed,
        area2: signed.unsigned_abs(),
        orientation,
        edge_milli,
        perimeter_milli,
        longest_edge,
    }
}

fn edge_key(points: &[Point], i: usize) -> (Point, Point) {
    let a = points[i];
    let b = points[(i + 1) % points.len()];
    if a <= b { (a, b) } else { (b, a) }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let prepared = prepare(points)?;
    let vertices: Vec<Point> = prepared.iter().map(|(p, _)| *p).collect();
    Ok(measures_for(&vertices))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Outside,
    Inside,
    Boundary { edge: usize },
}

fn on_segment(a: Point, b: Point, q: Point) -> bool {
    cross(a, b, q) == 0
        && q.x >= a.x.min(b.x)
        && q.x <= a.x.max(b.x)
        && q.y >= a.y.min(b.y)
        && q.y <= a.y.max(b.y)
}

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let prepared = prepare(points)?;
    if queries.len() > MAX_VERTICES {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (index, &query) in queries.iter().enumerate() {
        if !in_range(query) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    let vertices: Vec<Point> = prepared.iter().map(|(p, _)| *p).collect();
    let mut result = Vec::with_capacity(queries.len());

    for &q in queries {
        let mut boundary = None;
        for i in 0..vertices.len() {
            if on_segment(vertices[i], vertices[(i + 1) % vertices.len()], q) {
                boundary = Some(i);
                break;
            }
        }
        if let Some(edge) = boundary {
            result.push(Location::Boundary { edge });
            continue;
        }

        let mut inside = false;
        for i in 0..vertices.len() {
            let a = vertices[i];
            let b = vertices[(i + 1) % vertices.len()];
            if (a.y > q.y) != (b.y > q.y) {
                let c = cross(a, b, q);
                if (b.y > a.y && c > 0) || (b.y < a.y && c < 0) {
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

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (index, &point) in points.iter().enumerate() {
        if !unique.iter().any(|(p, _)| *p == point) {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|(p, _)| *p);

    let mut hull: Vec<(Point, usize)> = if unique.len() <= 1 {
        unique.clone()
    } else {
        let all_collinear = unique[2..]
            .iter()
            .all(|(p, _)| cross(unique[0].0, unique[1].0, *p) == 0);
        if all_collinear {
            match mode {
                Collinear::Include => unique.clone(),
                Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
            }
        } else {
            let mut lower: Vec<(Point, usize)> = Vec::new();
            for item in &unique {
                while lower.len() >= 2 {
                    let turn = cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, item.0);
                    if if mode == Collinear::Include { turn < 0 } else { turn <= 0 } {
                        lower.pop();
                    } else {
                        break;
                    }
                }
                lower.push(*item);
            }

            let mut upper: Vec<(Point, usize)> = Vec::new();
            for item in unique.iter().rev() {
                while upper.len() >= 2 {
                    let turn = cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, item.0);
                    if if mode == Collinear::Include { turn < 0 } else { turn <= 0 } {
                        upper.pop();
                    } else {
                        break;
                    }
                }
                upper.push(*item);
            }

            lower.pop();
            upper.pop();
            lower.extend(upper);
            lower
        }
    };

    if hull.len() >= 3 && signed_area2(&hull.iter().map(|x| x.0).collect::<Vec<_>>()) < 0 {
        hull.reverse();
    }
    if !hull.is_empty() {
        let start = hull
            .iter()
            .enumerate()
            .min_by_key(|(_, (p, _))| *p)
            .map(|(i, _)| i)
            .unwrap();
        hull.rotate_left(start);
    }

    let vertices: Vec<Point> = hull.iter().map(|(p, _)| *p).collect();
    let source_indices: Vec<usize> = hull.iter().map(|(_, i)| *i).collect();
    let area = signed_area2(&vertices).unsigned_abs();
    let perimeter = measures_for(&vertices).perimeter_milli;

    Ok(Hull {
        vertices,
        source_indices,
        area2: area,
        perimeter_milli: perimeter,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validated {
    pub vertices: Vec<Point>,
    pub source_indices: Vec<usize>,
    pub measures: Measures,
}

fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let ab_c = cross(a, b, c);
    let ab_d = cross(a, b, d);
    let cd_a = cross(c, d, a);
    let cd_b = cross(c, d, b);

    if ab_c == 0 && on_segment(a, b, c) {
        return true;
    }
    if ab_d == 0 && on_segment(a, b, d) {
        return true;
    }
    if cd_a == 0 && on_segment(c, d, a) {
        return true;
    }
    if cd_b == 0 && on_segment(c, d, b) {
        return true;
    }
    (ab_c > 0) != (ab_d > 0) && (cd_a > 0) != (cd_b > 0)
}

pub fn validate(points: &[Point]) -> Result<Validated, GeomError> {
    let prepared = prepare(points)?;
    let n = prepared.len();
    let vertices: Vec<Point> = prepared.iter().map(|(p, _)| *p).collect();

    let mut duplicate: Option<(usize, usize)> = None;
    for i in 0..n {
        for j in (i + 1)..n {
            if vertices[i] == vertices[j] {
                let pair = (i, j);
                if duplicate.map_or(true, |old| pair < old) {
                    duplicate = Some(pair);
                }
            }
        }
    }
    if let Some((first, second)) = duplicate {
        return Err(GeomError::Duplicate { first, second });
    }

    let area = signed_area2(&vertices);
    if area == 0 {
        return Err(GeomError::ZeroArea);
    }

    for i in 0..n {
        let prev = vertices[(i + n - 1) % n];
        let current = vertices[i];
        let next = vertices[(i + 1) % n];
        if cross(prev, current, next) == 0 {
            let ax = prev.x as i128 - current.x as i128;
            let ay = prev.y as i128 - current.y as i128;
            let bx = next.x as i128 - current.x as i128;
            let by = next.y as i128 - current.y as i128;
            if ax * bx + ay * by > 0 {
                return Err(GeomError::AdjacentOverlap { vertex: i });
            }
        }
    }

    for i in 0..n {
        for j in (i + 1)..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            if segments_intersect(
                vertices[i],
                vertices[(i + 1) % n],
                vertices[j],
                vertices[(j + 1) % n],
            ) {
                return Err(GeomError::Intersection {
                    edge_a: i,
                    edge_b: j,
                });
            }
        }
    }

    let mut ordered = prepared.clone();
    if area < 0 {
        ordered.reverse();
    }
    let start = ordered
        .iter()
        .enumerate()
        .min_by_key(|(_, (p, _))| *p)
        .map(|(i, _)| i)
        .unwrap();
    ordered.rotate_left(start);

    let canonical_vertices: Vec<Point> = ordered.iter().map(|(p, _)| *p).collect();
    let source_indices: Vec<usize> = ordered.iter().map(|(_, i)| *i).collect();
    let measures = measures_for(&canonical_vertices);

    Ok(Validated {
        vertices: canonical_vertices,
        source_indices,
        measures,
    })
}

fn main() {}