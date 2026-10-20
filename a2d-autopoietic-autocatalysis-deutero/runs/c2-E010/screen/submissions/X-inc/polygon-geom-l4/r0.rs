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

fn prepare(points: &[Point]) -> Result<Vec<Point>, GeomError> {
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

fn cross(a: Point, b: Point, c: Point) -> i128 {
    (b.x as i128 - a.x as i128) * (c.y as i128 - a.y as i128)
        - (b.y as i128 - a.y as i128) * (c.x as i128 - a.x as i128)
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

fn abs_i128(value: i128) -> u128 {
    if value < 0 {
        (-value) as u128
    } else {
        value as u128
    }
}

fn rounded_sqrt(n: u128) -> u128 {
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
    let lower_sq = lo * lo;
    let upper_sq = hi * hi;
    if n - lower_sq >= upper_sq - n {
        hi
    } else {
        lo
    }
}

fn edge_length_milli(a: Point, b: Point) -> u64 {
    let dx = (a.x as i128 - b.x as i128) as i128;
    let dy = (a.y as i128 - b.y as i128) as i128;
    let d2 = (dx * dx + dy * dy) as u128;
    rounded_sqrt(d2 * 1_000_000) as u64
}

fn measures_for(points: &[Point]) -> Measures {
    let signed_area2 = shoelace(points);
    let orientation = if signed_area2 > 0 {
        Orientation::CounterClockwise
    } else if signed_area2 < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
    };

    let mut edge_milli = Vec::with_capacity(points.len());
    let mut perimeter_milli = 0u64;
    for i in 0..points.len() {
        let len = edge_length_milli(points[i], points[(i + 1) % points.len()]);
        edge_milli.push(len);
        perimeter_milli += len;
    }

    let longest_edge = if edge_milli.is_empty() {
        None
    } else {
        let mut best = 0usize;
        for i in 1..edge_milli.len() {
            if edge_milli[i] > edge_milli[best]
                || (edge_milli[i] == edge_milli[best]
                    && edge_tie_key(points, i) < edge_tie_key(points, best))
            {
                best = i;
            }
        }
        Some(best)
    };

    Measures {
        signed_area2,
        area2: abs_i128(signed_area2),
        orientation,
        edge_milli,
        perimeter_milli,
        longest_edge,
    }
}

fn edge_tie_key(points: &[Point], i: usize) -> (Point, Point, usize) {
    let a = points[i];
    let b = points[(i + 1) % points.len()];
    if a <= b {
        (a, b, i)
    } else {
        (b, a, i)
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let effective = prepare(points)?;
    Ok(measures_for(&effective))
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
    for (i, &q) in queries.iter().enumerate() {
        if !in_range(q) {
            return Err(GeomError::QueryOutOfRange { index: i });
        }
    }

    let mut result = Vec::with_capacity(queries.len());
    for &q in queries {
        let mut boundary = None;
        let mut inside = false;
        for i in 0..polygon.len() {
            let a = polygon[i];
            let b = polygon[(i + 1) % polygon.len()];
            if on_segment(a, b, q) {
                boundary = Some(i);
                break;
            }
            if (a.y > q.y) != (b.y > q.y) {
                let c = cross(a, b, q);
                if (b.y > a.y && c > 0) || (b.y < a.y && c < 0) {
                    inside = !inside;
                }
            }
        }
        result.push(if let Some(edge) = boundary {
            Location::Boundary { edge }
        } else if inside {
            Location::Inside
        } else {
            Location::Outside
        });
    }
    Ok(result)
}

fn cloud_points(points: &[Point]) -> Result<Vec<(Point, usize)>, GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (i, &p) in points.iter().enumerate() {
        if !in_range(p) {
            return Err(GeomError::OutOfRange { index: i });
        }
    }

    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (i, &p) in points.iter().enumerate() {
        if !unique.iter().any(|(q, _)| *q == p) {
            unique.push((p, i));
        }
    }
    unique.sort_by_key(|entry| entry.0);
    Ok(unique)
}

fn turn(a: Point, b: Point, c: Point) -> i128 {
    cross(a, b, c)
}

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    let unique = cloud_points(points)?;
    if unique.is_empty() {
        return Ok(Hull {
            vertices: Vec::new(),
            source_indices: Vec::new(),
            area2: 0,
            perimeter_milli: 0,
        });
    }
    if unique.len() == 1 {
        return Ok(Hull {
            vertices: vec![unique[0].0],
            source_indices: vec![unique[0].1],
            area2: 0,
            perimeter_milli: 0,
        });
    }

    let all_collinear = unique
        .iter()
        .skip(2)
        .all(|entry| turn(unique[0].0, unique[1].0, entry.0) == 0);

    let hull: Vec<(Point, usize)> = if all_collinear {
        match mode {
            Collinear::Include => unique.clone(),
            Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
        }
    } else {
        let mut lower: Vec<(Point, usize)> = Vec::new();
        for &entry in &unique {
            while lower.len() >= 2 {
                let t = turn(lower[lower.len() - 2].0, lower[lower.len() - 1].0, entry.0);
                if t > 0 || (mode == Collinear::Include && t == 0) {
                    break;
                }
                lower.pop();
            }
            lower.push(entry);
        }

        let mut upper: Vec<(Point, usize)> = Vec::new();
        for &entry in unique.iter().rev() {
            while upper.len() >= 2 {
                let t = turn(upper[upper.len() - 2].0, upper[upper.len() - 1].0, entry.0);
                if t > 0 || (mode == Collinear::Include && t == 0) {
                    break;
                }
                upper.pop();
            }
            upper.push(entry);
        }
        lower.pop();
        upper.pop();
        lower.extend(upper);
        lower
    };

    let vertices: Vec<Point> = hull.iter().map(|entry| entry.0).collect();
    let source_indices: Vec<usize> = hull.iter().map(|entry| entry.1).collect();
    let area2 = abs_i128(shoelace(&vertices));
    let perimeter_milli = measures_for(&vertices).perimeter_milli;
    Ok(Hull {
        vertices,
        source_indices,
        area2,
        perimeter_milli,
    })
}

fn dot_at(prev: Point, current: Point, next: Point) -> i128 {
    (prev.x as i128 - current.x as i128) * (next.x as i128 - current.x as i128)
        + (prev.y as i128 - current.y as i128) * (next.y as i128 - current.y as i128)
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
    let effective = prepare(points)?;
    let n = effective.len();

    for i in 0..n {
        for j in (i + 1)..n {
            if effective[i] == effective[j] {
                return Err(GeomError::Duplicate {
                    first: i,
                    second: j,
                });
            }
        }
    }

    let signed_area2 = shoelace(&effective);
    if signed_area2 == 0 {
        return Err(GeomError::ZeroArea);
    }

    for i in 0..n {
        let prev = effective[(i + n - 1) % n];
        let current = effective[i];
        let next = effective[(i + 1) % n];
        if cross(prev, current, next) == 0 && dot_at(prev, current, next) > 0 {
            return Err(GeomError::AdjacentOverlap { vertex: i });
        }
    }

    for i in 0..n {
        for j in (i + 1)..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            if segments_intersect(
                effective[i],
                effective[(i + 1) % n],
                effective[j],
                effective[(j + 1) % n],
            ) {
                return Err(GeomError::Intersection {
                    edge_a: i,
                    edge_b: j,
                });
            }
        }
    }

    let min_index = (0..n).min_by_key(|&i| effective[i]).unwrap();
    let mut canonical_indices = Vec::with_capacity(n);
    if signed_area2 > 0 {
        for offset in 0..n {
            canonical_indices.push((min_index + offset) % n);
        }
    } else {
        for offset in 0..n {
            canonical_indices.push((min_index + n - offset) % n);
        }
    }

    let vertices: Vec<Point> = canonical_indices.iter().map(|&i| effective[i]).collect();
    let source_indices = canonical_indices;
    let measures = measures_for(&vertices);
    Ok(Validated {
        vertices,
        source_indices,
        measures,
    })
}