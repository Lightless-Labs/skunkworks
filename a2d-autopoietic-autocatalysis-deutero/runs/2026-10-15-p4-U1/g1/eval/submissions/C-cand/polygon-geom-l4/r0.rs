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

const COORD_LIMIT: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

fn in_range(point: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&point.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&point.y)
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

fn rounded_sqrt(n: u128) -> u128 {
    let mut lo = 0u128;
    let mut hi = 1u128;
    while hi * hi < n {
        hi *= 2;
    }
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if mid * mid <= n {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let lower_sq = lo * lo;
    let upper_sq = hi * hi;
    if n - lower_sq >= upper_sq - n {
        hi
    } else {
        lo
    }
}

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let dx = (b.x as i128 - a.x as i128).unsigned_abs();
    let dy = (b.y as i128 - a.y as i128).unsigned_abs();
    let scaled_sq = (dx * dx + dy * dy) * 1_000_000u128;
    rounded_sqrt(scaled_sq) as u64
}

fn edge_pair(a: Point, b: Point) -> (Point, Point) {
    if a <= b { (a, b) } else { (b, a) }
}

fn measures_for(points: &[Point]) -> Measures {
    let signed_area2 = shoelace(points);
    let area2 = signed_area2.unsigned_abs();
    let orientation = if signed_area2 > 0 {
        Orientation::CounterClockwise
    } else if signed_area2 < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
    };

    let mut lengths = Vec::with_capacity(points.len());
    let mut perimeter_milli = 0u64;
    let mut longest_edge = None;

    for i in 0..points.len() {
        let length = rounded_edge_milli(points[i], points[(i + 1) % points.len()]);
        lengths.push(length);
        perimeter_milli += length;

        match longest_edge {
            None => longest_edge = Some(i),
            Some(best) => {
                let pair = edge_pair(points[i], points[(i + 1) % points.len()]);
                let best_pair = edge_pair(points[best], points[(best + 1) % points.len()]);
                if length > lengths[best]
                    || (length == lengths[best]
                        && (pair < best_pair || (pair == best_pair && i < best)))
                {
                    longest_edge = Some(i);
                }
            }
        }
    }

    Measures {
        signed_area2,
        area2,
        orientation,
        edge_milli: lengths,
        perimeter_milli,
        longest_edge,
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let prepared = prepare(points)?;
    Ok(measures_for(&prepared))
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

        let mut odd = false;
        for i in 0..polygon.len() {
            let a = polygon[i];
            let b = polygon[(i + 1) % polygon.len()];
            if (a.y > q.y) != (b.y > q.y) {
                let side = cross(a, b, q);
                if (b.y > a.y && side > 0) || (b.y < a.y && side < 0) {
                    odd = !odd;
                }
            }
        }
        result.push(if odd {
            Location::Inside
        } else {
            Location::Outside
        });
    }
    Ok(result)
}

fn cloud_checked(points: &[Point]) -> Result<Vec<(Point, usize)>, GeomError> {
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
    unique.sort_by_key(|(point, _)| *point);
    Ok(unique)
}

fn all_collinear(points: &[(Point, usize)]) -> bool {
    points.len() <= 2 || points.iter().all(|(p, _)| cross(points[0].0, points[1].0, *p) == 0)
}

fn hull_cycle(points: &[(Point, usize)], mode: Collinear) -> Vec<(Point, usize)> {
    if points.len() <= 1 {
        return points.to_vec();
    }
    if all_collinear(points) {
        return match mode {
            Collinear::Include => points.to_vec(),
            Collinear::Exclude => vec![points[0], points[points.len() - 1]],
        };
    }

    let mut lower: Vec<(Point, usize)> = Vec::new();
    for &item in points {
        while lower.len() >= 2 {
            let n = lower.len();
            let turn = cross(lower[n - 2].0, lower[n - 1].0, item.0);
            if turn > 0 || (mode == Collinear::Include && turn == 0) {
                break;
            }
            lower.pop();
        }
        lower.push(item);
    }

    let mut upper: Vec<(Point, usize)> = Vec::new();
    for &item in points.iter().rev() {
        while upper.len() >= 2 {
            let n = upper.len();
            let turn = cross(upper[n - 2].0, upper[n - 1].0, item.0);
            if turn > 0 || (mode == Collinear::Include && turn == 0) {
                break;
            }
            upper.pop();
        }
        upper.push(item);
    }

    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    let unique = cloud_checked(points)?;
    let cycle = hull_cycle(&unique, mode);
    let vertices: Vec<Point> = cycle.iter().map(|(point, _)| *point).collect();
    let source_indices: Vec<usize> = cycle.iter().map(|(_, index)| *index).collect();

    let area2 = if vertices.len() >= 3 {
        shoelace(&vertices).unsigned_abs()
    } else {
        0
    };
    let perimeter_milli = if vertices.is_empty() {
        0
    } else {
        measures_for(&vertices).perimeter_milli
    };

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
    ((ab_c > 0 && ab_d < 0) || (ab_c < 0 && ab_d > 0))
        && ((cd_a > 0 && cd_b < 0) || (cd_a < 0 && cd_b > 0))
}

pub fn validate(points: &[Point]) -> Result<Validated, GeomError> {
    let polygon = prepare(points)?;
    let n = polygon.len();

    for first in 0..n {
        for second in first + 1..n {
            if polygon[first] == polygon[second] {
                return Err(GeomError::Duplicate { first, second });
            }
        }
    }

    let signed_area2 = shoelace(&polygon);
    if signed_area2 == 0 {
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
        for j in i + 1..n {
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

    let mut minimum = 0usize;
    for i in 1..n {
        if polygon[i] < polygon[minimum] {
            minimum = i;
        }
    }
    let direction: isize = if signed_area2 > 0 { 1 } else { -1 };
    let mut vertices = Vec::with_capacity(n);
    let mut source_indices = Vec::with_capacity(n);
    for step in 0..n {
        let index =
            (minimum as isize + direction * step as isize).rem_euclid(n as isize) as usize;
        vertices.push(polygon[index]);
        source_indices.push(index);
    }

    let measures = measures_for(&vertices);
    Ok(Validated {
        vertices,
        source_indices,
        measures,
    })
}

fn main() {}