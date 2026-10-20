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

    let mut effective = points.to_vec();
    if effective.len() >= 2 && effective.last() == effective.first() {
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
    let mut sum = 0i128;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }
    sum
}

fn rounded_sqrt(n: u128) -> u128 {
    if n == 0 {
        return 0;
    }

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

    let lower_square = lo * lo;
    let upper_square = hi * hi;
    if n - lower_square >= upper_square - n {
        hi
    } else {
        lo
    }
}

fn edge_mill(a: Point, b: Point) -> u64 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
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

    let edge_milli: Vec<u64> = (0..points.len())
        .map(|i| edge_mill(points[i], points[(i + 1) % points.len()]))
        .collect();
    let perimeter_milli = edge_milli.iter().copied().sum();

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
        area2: signed_area2.unsigned_abs(),
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
    let effective = prepare_polygon(points)?;
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
    let polygon = prepare_polygon(points)?;

    if queries.len() > MAX_VERTICES {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (index, &query) in queries.iter().enumerate() {
        if !in_range(query) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    let mut result = Vec::with_capacity(queries.len());
    for &query in queries {
        let mut boundary = None;
        for i in 0..polygon.len() {
            if on_segment(polygon[i], polygon[(i + 1) % polygon.len()], query) {
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
            if (a.y > query.y) != (b.y > query.y) {
                let side = cross(a, b, query);
                if (b.y > a.y && side > 0) || (b.y < a.y && side < 0) {
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

fn segment_intersects(a: Point, b: Point, c: Point, d: Point) -> bool {
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

fn dot_at_vertex(prev: Point, current: Point, next: Point) -> i128 {
    (prev.x as i128 - current.x as i128) * (next.x as i128 - current.x as i128)
        + (prev.y as i128 - current.y as i128) * (next.y as i128 - current.y as i128)
}

pub fn validate(points: &[Point]) -> Result<Validated, GeomError> {
    let effective = prepare_polygon(points)?;
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
        if cross(prev, current, next) == 0 && dot_at_vertex(prev, current, next) > 0 {
            return Err(GeomError::AdjacentOverlap { vertex: i });
        }
    }

    for i in 0..n {
        let a = effective[i];
        let b = effective[(i + 1) % n];
        for j in (i + 1)..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let c = effective[j];
            let d = effective[(j + 1) % n];
            if segment_intersects(a, b, c, d) {
                return Err(GeomError::Intersection {
                    edge_a: i,
                    edge_b: j,
                });
            }
        }
    }

    let minimum_index = (0..n)
        .min_by_key(|&i| effective[i])
        .expect("effective polygon has at least three points");

    let mut order = Vec::with_capacity(n);
    if signed_area2 > 0 {
        for k in 0..n {
            order.push((minimum_index + k) % n);
        }
    } else {
        for k in 0..n {
            order.push((minimum_index + n - k) % n);
        }
    }

    let vertices: Vec<Point> = order.iter().map(|&i| effective[i]).collect();
    let source_indices = order;
    let measures = measures_for(&vertices);

    Ok(Validated {
        vertices,
        source_indices,
        measures,
    })
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
    unique.sort_by_key(|(point, _)| *point);

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
        .all(|(p, _)| cross(unique[0].0, unique[1].0, *p) == 0);

    let selected: Vec<(Point, usize)> = if all_collinear {
        match mode {
            Collinear::Include => unique,
            Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
        }
    } else {
        let mut lower: Vec<(Point, usize)> = Vec::new();
        for &item in &unique {
            while lower.len() >= 2 {
                let turn = cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, item.0);
                if turn > 0 || (mode == Collinear::Include && turn == 0) {
                    break;
                }
                lower.pop();
            }
            lower.push(item);
        }

        let mut upper: Vec<(Point, usize)> = Vec::new();
        for &item in unique.iter().rev() {
            while upper.len() >= 2 {
                let turn = cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, item.0);
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
    };

    let vertices: Vec<Point> = selected.iter().map(|(p, _)| *p).collect();
    let source_indices: Vec<usize> = selected.iter().map(|(_, i)| *i).collect();
    let signed_area2 = if vertices.len() >= 3 {
        shoelace(&vertices)
    } else {
        0
    };
    let perimeter_milli = if vertices.is_empty() {
        0
    } else {
        (0..vertices.len())
            .map(|i| edge_mill(vertices[i], vertices[(i + 1) % vertices.len()]))
            .sum()
    };

    Ok(Hull {
        vertices,
        source_indices,
        area2: signed_area2.unsigned_abs(),
        perimeter_milli,
    })
}