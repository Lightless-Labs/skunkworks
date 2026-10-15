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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validated {
    pub vertices: Vec<Point>,
    pub source_indices: Vec<usize>,
    pub measures: Measures,
}

const LIMIT: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

fn in_range(p: Point) -> bool {
    (-LIMIT..=LIMIT).contains(&p.x) && (-LIMIT..=LIMIT).contains(&p.y)
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

    let len = if points.len() >= 2 && points[0] == points[points.len() - 1] {
        points.len() - 1
    } else {
        points.len()
    };
    if len < 3 {
        return Err(GeomError::TooFewVertices { len });
    }
    Ok(&points[..len])
}

fn cross(a: Point, b: Point, c: Point) -> i128 {
    (b.x as i128 - a.x as i128) * (c.y as i128 - a.y as i128)
        - (b.y as i128 - a.y as i128) * (c.x as i128 - a.x as i128)
}

fn shoelace(points: &[Point]) -> i128 {
    let mut sum = 0i128;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }
    sum
}

fn rounded_length_milli(a: Point, b: Point) -> u64 {
    let dx = (a.x as i128 - b.x as i128).unsigned_abs();
    let dy = (a.y as i128 - b.y as i128).unsigned_abs();

    // For validated coordinates, this is at most 8e24, within u128.
    let n = (dx * dx + dy * dy) * 1_000_000u128;

    // The answer is at most ceil(sqrt(8e24)) < 3e12. Avoid testing
    // mid * mid, which could overflow for arbitrary search bounds.
    let mut lo = 0u128;
    let mut hi = 3_000_000_000_000u128;
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if mid <= n / mid {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }

    // sqrt(n) rounds up exactly when n - lo^2 >= lo + 1/2.
    // Compare 4*n with (2*lo+1)^2 without overflowing.
    let threshold = 2 * lo + 1;
    if 4 * n >= threshold * threshold {
        (lo + 1) as u64
    } else {
        lo as u64
    }
}

fn ordered_endpoints(a: Point, b: Point) -> (Point, Point) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn measures_unchecked(points: &[Point]) -> Measures {
    let signed_area2 = shoelace(points);
    let area2 = signed_area2.unsigned_abs();
    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
    };

    let mut edge_milli = Vec::with_capacity(points.len());
    let mut perimeter_milli = 0u64;
    for i in 0..points.len() {
        let length = rounded_length_milli(points[i], points[(i + 1) % points.len()]);
        edge_milli.push(length);
        perimeter_milli += length;
    }

    let mut longest_edge = None;
    for i in 0..points.len() {
        let Some(best) = longest_edge else {
            longest_edge = Some(i);
            continue;
        };
        if edge_milli[i] > edge_milli[best] {
            longest_edge = Some(i);
        } else if edge_milli[i] == edge_milli[best] {
            let endpoints = ordered_endpoints(points[i], points[(i + 1) % points.len()]);
            let best_endpoints =
                ordered_endpoints(points[best], points[(best + 1) % points.len()]);
            if endpoints < best_endpoints || (endpoints == best_endpoints && i < best) {
                longest_edge = Some(i);
            }
        }
    }

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
    Ok(measures_unchecked(prepare(points)?))
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

    let mut result = Vec::with_capacity(queries.len());
    for &q in queries {
        let mut boundary = None;
        for i in 0..polygon.len() {
            if on_segment(polygon[i], polygon[(i + 1) % polygon.len()], q) {
                boundary = Some(i);
                break;
            }
        }
        if let Some(edge) = boundary {
            result.push(Location::Boundary { edge });
            continue;
        }

        let mut inside = false;
        for i in 0..polygon.len() {
            let a = polygon[i];
            let b = polygon[(i + 1) % polygon.len()];
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

fn cloud(points: &[Point]) -> Result<Vec<(Point, usize)>, GeomError> {
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
    unique.sort_by_key(|entry| entry.0);
    Ok(unique)
}

fn all_collinear(points: &[(Point, usize)]) -> bool {
    points.len() < 3
        || points[2..]
            .iter()
            .all(|(p, _)| cross(points[0].0, points[1].0, *p) == 0)
}

fn hull_perimeter(points: &[Point]) -> u64 {
    if points.len() < 2 {
        return 0;
    }
    let mut sum = 0u64;
    for i in 0..points.len() {
        sum += rounded_length_milli(points[i], points[(i + 1) % points.len()]);
    }
    sum
}

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    let unique = cloud(points)?;
    let output: Vec<(Point, usize)> = if unique.len() <= 1 {
        unique
    } else if all_collinear(&unique) {
        match mode {
            Collinear::Include => unique,
            Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
        }
    } else {
        let mut lower: Vec<(Point, usize)> = Vec::new();
        for item in &unique {
            while lower.len() >= 2 {
                let turn = cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, item.0);
                if turn > 0 || (turn == 0 && mode == Collinear::Include) {
                    break;
                }
                lower.pop();
            }
            lower.push(*item);
        }

        let mut upper: Vec<(Point, usize)> = Vec::new();
        for item in unique.iter().rev() {
            while upper.len() >= 2 {
                let turn = cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, item.0);
                if turn > 0 || (turn == 0 && mode == Collinear::Include) {
                    break;
                }
                upper.pop();
            }
            upper.push(*item);
        }

        lower.pop();
        upper.pop();
        lower.extend(upper);
        lower
    };

    let vertices: Vec<Point> = output.iter().map(|(p, _)| *p).collect();
    let source_indices: Vec<usize> = output.iter().map(|(_, index)| *index).collect();
    let area2 = shoelace(&vertices).unsigned_abs();
    let perimeter_milli = hull_perimeter(&vertices);
    Ok(Hull {
        vertices,
        source_indices,
        area2,
        perimeter_milli,
    })
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
    (ab_c < 0) != (ab_d < 0) && (cd_a < 0) != (cd_b < 0)
}

pub fn validate(points: &[Point]) -> Result<Validated, GeomError> {
    let polygon = prepare(points)?;
    let n = polygon.len();

    for i in 0..n {
        for j in (i + 1)..n {
            if polygon[i] == polygon[j] {
                return Err(GeomError::Duplicate {
                    first: i,
                    second: j,
                });
            }
        }
    }

    let signed = shoelace(polygon);
    if signed == 0 {
        return Err(GeomError::ZeroArea);
    }

    for i in 0..n {
        let prev = polygon[(i + n - 1) % n];
        let current = polygon[i];
        let next = polygon[(i + 1) % n];
        if cross(prev, current, next) == 0 {
            let dx1 = prev.x as i128 - current.x as i128;
            let dy1 = prev.y as i128 - current.y as i128;
            let dx2 = next.x as i128 - current.x as i128;
            let dy2 = next.y as i128 - current.y as i128;
            if dx1 * dx2 + dy1 * dy2 > 0 {
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
                polygon[i],
                polygon[(i + 1) % n],
                polygon[j],
                polygon[(j + 1) % n],
            ) {
                return Err(GeomError::Intersection {
                    edge_a: i,
                    edge_b: j,
                });
            }
        }
    }

    let start = (0..n).min_by_key(|&i| polygon[i]).unwrap();
    let mut vertices = Vec::with_capacity(n);
    let mut source_indices = Vec::with_capacity(n);
    if signed > 0 {
        for k in 0..n {
            let index = (start + k) % n;
            vertices.push(polygon[index]);
            source_indices.push(index);
        }
    } else {
        for k in 0..n {
            let index = (start + n - k) % n;
            vertices.push(polygon[index]);
            source_indices.push(index);
        }
    }

    let measures = measures_unchecked(&vertices);
    Ok(Validated {
        vertices,
        source_indices,
        measures,
    })
}