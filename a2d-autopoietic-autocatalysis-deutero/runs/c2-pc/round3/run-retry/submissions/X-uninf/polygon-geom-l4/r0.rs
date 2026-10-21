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

const MAX_VERTICES: usize = 4096;
const COORD_LIMIT: i64 = 1_000_000_000;

fn in_range(p: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&p.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&p.y)
}

fn prepare_polygon(points: &[Point]) -> Result<Vec<Point>, GeomError> {
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
    let abx = b.x as i128 - a.x as i128;
    let aby = b.y as i128 - a.y as i128;
    let acx = c.x as i128 - a.x as i128;
    let acy = c.y as i128 - a.y as i128;
    abx * acy - aby * acx
}

fn dot_at_vertex(prev: Point, current: Point, next: Point) -> i128 {
    let ax = prev.x as i128 - current.x as i128;
    let ay = prev.y as i128 - current.y as i128;
    let bx = next.x as i128 - current.x as i128;
    let by = next.y as i128 - current.y as i128;
    ax * bx + ay * by
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

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    let n = (dx * dx + dy * dy) as u128 * 1_000_000;

    let mut lo = 0u128;
    let mut hi = 3_000_000_000_000u128;
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if mid <= n / mid.max(1) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }

    let floor = lo;
    let threshold = floor * floor + floor;
    if n > threshold {
        (floor + 1) as u64
    } else {
        floor as u64
    }
}

fn endpoint_pair(a: Point, b: Point) -> (Point, Point) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn measures_for(points: &[Point]) -> Measures {
    let signed = signed_area2(points);
    let mut edge_milli = Vec::with_capacity(points.len());
    let mut perimeter = 0u64;
    let mut longest_edge = None;

    for i in 0..points.len() {
        let length = rounded_edge_milli(points[i], points[(i + 1) % points.len()]);
        edge_milli.push(length);
        perimeter += length;

        match longest_edge {
            None => longest_edge = Some(i),
            Some(best) => {
                let current_pair = endpoint_pair(points[i], points[(i + 1) % points.len()]);
                let best_pair = endpoint_pair(points[best], points[(best + 1) % points.len()]);
                if length > edge_milli[best]
                    || (length == edge_milli[best]
                        && (current_pair < best_pair
                            || (current_pair == best_pair && i < best)))
                {
                    longest_edge = Some(i);
                }
            }
        }
    }

    Measures {
        signed_area2: signed,
        area2: signed.unsigned_abs(),
        orientation: if signed > 0 {
            Orientation::CounterClockwise
        } else if signed < 0 {
            Orientation::Clockwise
        } else {
            Orientation::Degenerate
        },
        edge_milli,
        perimeter_milli: perimeter,
        longest_edge,
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare_polygon(points)?;
    Ok(measures_for(&polygon))
}

fn on_segment(a: Point, b: Point, q: Point) -> bool {
    cross(a, b, q) == 0
        && q.x >= a.x.min(b.x)
        && q.x <= a.x.max(b.x)
        && q.y >= a.y.min(b.y)
        && q.y <= a.y.max(b.y)
}

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let polygon = prepare_polygon(points)?;
    if queries.len() > MAX_VERTICES {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (index, &query) in queries.iter().enumerate() {
        if !in_range(query) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    let mut results = Vec::with_capacity(queries.len());
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

        results.push(match boundary {
            Some(edge) => Location::Boundary { edge },
            None if inside => Location::Inside,
            None => Location::Outside,
        });
    }
    Ok(results)
}

fn prepare_cloud(points: &[Point]) -> Result<Vec<(Point, usize)>, GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut unique = Vec::<(Point, usize)>::new();
    for (index, &point) in points.iter().enumerate() {
        if !unique.iter().any(|(existing, _)| *existing == point) {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|entry| entry.0);
    Ok(unique)
}

fn all_collinear(points: &[(Point, usize)]) -> bool {
    if points.len() < 3 {
        return true;
    }
    points[2..]
        .iter()
        .all(|(p, _)| cross(points[0].0, points[1].0, *p) == 0)
}

fn hull_area2(vertices: &[Point]) -> u128 {
    if vertices.len() < 3 {
        0
    } else {
        signed_area2(vertices).unsigned_abs()
    }
}

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    let unique = prepare_cloud(points)?;
    let (ordered, sources) = if unique.len() <= 1 {
        (
            unique.iter().map(|entry| entry.0).collect::<Vec<_>>(),
            unique.iter().map(|entry| entry.1).collect::<Vec<_>>(),
        )
    } else if all_collinear(&unique) {
        let chosen: Vec<_> = match mode {
            Collinear::Include => unique.clone(),
            Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
        };
        (
            chosen.iter().map(|entry| entry.0).collect(),
            chosen.iter().map(|entry| entry.1).collect(),
        )
    } else {
        let mut lower = Vec::<(Point, usize)>::new();
        for &point in &unique {
            while lower.len() >= 2 {
                let n = lower.len();
                let c = cross(lower[n - 2].0, lower[n - 1].0, point.0);
                if c > 0 || (mode == Collinear::Include && c == 0) {
                    break;
                }
                lower.pop();
            }
            lower.push(point);
        }

        let mut upper = Vec::<(Point, usize)>::new();
        for &point in unique.iter().rev() {
            while upper.len() >= 2 {
                let n = upper.len();
                let c = cross(upper[n - 2].0, upper[n - 1].0, point.0);
                if c > 0 || (mode == Collinear::Include && c == 0) {
                    break;
                }
                upper.pop();
            }
            upper.push(point);
        }

        lower.pop();
        upper.pop();
        lower.extend(upper);
        (
            lower.iter().map(|entry| entry.0).collect(),
            lower.iter().map(|entry| entry.1).collect(),
        )
    };

    let mut perimeter = 0u64;
    if ordered.len() >= 2 {
        for i in 0..ordered.len() {
            perimeter += rounded_edge_milli(ordered[i], ordered[(i + 1) % ordered.len()]);
        }
    }

    Ok(Hull {
        area2: hull_area2(&ordered),
        vertices: ordered,
        source_indices: sources,
        perimeter_milli: perimeter,
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
    let polygon = prepare_polygon(points)?;
    let n = polygon.len();

    for i in 0..n {
        for j in i + 1..n {
            if polygon[i] == polygon[j] {
                return Err(GeomError::Duplicate {
                    first: i,
                    second: j,
                });
            }
        }
    }

    let area = signed_area2(&polygon);
    if area == 0 {
        return Err(GeomError::ZeroArea);
    }

    for i in 0..n {
        let prev = polygon[(i + n - 1) % n];
        let current = polygon[i];
        let next = polygon[(i + 1) % n];
        if cross(prev, current, next) == 0 && dot_at_vertex(prev, current, next) > 0 {
            return Err(GeomError::AdjacentOverlap { vertex: i });
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

    let mut vertices = Vec::with_capacity(n);
    let mut source_indices = Vec::with_capacity(n);
    for offset in 0..n {
        let index = if area > 0 {
            (minimum + offset) % n
        } else {
            (minimum + n - offset) % n
        };
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