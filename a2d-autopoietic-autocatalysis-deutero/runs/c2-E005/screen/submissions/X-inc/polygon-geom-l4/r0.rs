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
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut effective = points.to_vec();
    if effective.len() >= 2 && effective.first() == effective.last() {
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

fn integer_sqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

fn rounded_length_milli(a: Point, b: Point) -> u64 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    let squared = (dx * dx + dy * dy) as u128;
    let scaled = squared * 1_000_000;
    let root = integer_sqrt(scaled);
    let remainder = scaled - root * root;
    let rounded = if remainder >= root + 1 { root + 1 } else { root };
    rounded as u64
}

fn measures_prepared(points: &[Point]) -> Measures {
    let signed_area2 = shoelace(points);
    let area2 = signed_area2.unsigned_abs();
    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
    };

    let mut edge_milli = Vec::with_capacity(points.len());
    let mut perimeter_milli = 0u64;
    let mut longest_edge: Option<usize> = None;

    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let length = rounded_length_milli(a, b);
        edge_milli.push(length);
        perimeter_milli += length;

        match longest_edge {
            None => longest_edge = Some(i),
            Some(previous) => {
                let old_length = edge_milli[previous];
                let (low, high) = if a <= b { (a, b) } else { (b, a) };
                let (old_a, old_b) = (points[previous], points[(previous + 1) % points.len()]);
                let (old_low, old_high) = if old_a <= old_b {
                    (old_a, old_b)
                } else {
                    (old_b, old_a)
                };
                if length > old_length
                    || (length == old_length
                        && ((low, high) < (old_low, old_high)
                            || ((low, high) == (old_low, old_high) && i < previous)))
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
        edge_milli,
        perimeter_milli,
        longest_edge,
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let effective = prepare(points)?;
    Ok(measures_prepared(&effective))
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

    let mut results = Vec::with_capacity(queries.len());
    for &query in queries {
        let mut boundary = None;
        for i in 0..polygon.len() {
            if on_segment(polygon[i], polygon[(i + 1) % polygon.len()], query) {
                boundary = Some(i);
                break;
            }
        }
        if let Some(edge) = boundary {
            results.push(Location::Boundary { edge });
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
        results.push(if inside {
            Location::Inside
        } else {
            Location::Outside
        });
    }
    Ok(results)
}

fn between_collinear(a: Point, b: Point, p: Point) -> bool {
    cross(a, b, p) == 0
        && p.x >= a.x.min(b.x)
        && p.x <= a.x.max(b.x)
        && p.y >= a.y.min(b.y)
        && p.y <= a.y.max(b.y)
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
    unique.sort_by_key(|&(point, _)| point);

    let mut vertices = Vec::new();
    let mut source_indices = Vec::new();

    if unique.len() <= 2 {
        for (point, source) in unique {
            vertices.push(point);
            source_indices.push(source);
        }
    } else {
        let all_collinear = unique
            .iter()
            .skip(2)
            .all(|&(point, _)| cross(unique[0].0, unique[1].0, point) == 0);

        if all_collinear {
            match mode {
                Collinear::Include => {
                    for (point, source) in unique {
                        vertices.push(point);
                        source_indices.push(source);
                    }
                }
                Collinear::Exclude => {
                    let first = unique.first().copied().unwrap();
                    let last = unique.last().copied().unwrap();
                    vertices.push(first.0);
                    source_indices.push(first.1);
                    vertices.push(last.0);
                    source_indices.push(last.1);
                }
            }
        } else {
            let should_pop = |turn: i128| match mode {
                Collinear::Exclude => turn <= 0,
                Collinear::Include => turn < 0,
            };

            let mut lower: Vec<(Point, usize)> = Vec::new();
            for &item in &unique {
                while lower.len() >= 2
                    && should_pop(cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, item.0))
                {
                    lower.pop();
                }
                lower.push(item);
            }

            let mut upper: Vec<(Point, usize)> = Vec::new();
            for &item in unique.iter().rev() {
                while upper.len() >= 2
                    && should_pop(cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, item.0))
                {
                    upper.pop();
                }
                upper.push(item);
            }

            lower.pop();
            upper.pop();
            lower.extend(upper);
            for (point, source) in lower {
                vertices.push(point);
                source_indices.push(source);
            }
        }
    }

    let signed = if vertices.len() >= 3 {
        shoelace(&vertices)
    } else {
        0
    };
    let area2 = signed.unsigned_abs();
    let perimeter_milli = if vertices.len() >= 2 {
        let mut total = 0u64;
        for i in 0..vertices.len() {
            total += rounded_length_milli(vertices[i], vertices[(i + 1) % vertices.len()]);
        }
        total
    } else {
        0
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
    (ab_c < 0) != (ab_d < 0) && (cd_a < 0) != (cd_b < 0)
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
        let a = effective[i];
        let b = effective[(i + 1) % n];
        for j in (i + 1)..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let c = effective[j];
            let d = effective[(j + 1) % n];
            if segments_intersect(a, b, c, d) {
                return Err(GeomError::Intersection {
                    edge_a: i,
                    edge_b: j,
                });
            }
        }
    }

    let mut start = 0usize;
    for i in 1..n {
        if effective[i] < effective[start] {
            start = i;
        }
    }

    let mut vertices = Vec::with_capacity(n);
    let mut source_indices = Vec::with_capacity(n);
    if signed_area2 > 0 {
        for offset in 0..n {
            let index = (start + offset) % n;
            vertices.push(effective[index]);
            source_indices.push(index);
        }
    } else {
        for offset in 0..n {
            let index = (start + n - (offset % n)) % n;
            vertices.push(effective[index]);
            source_indices.push(index);
        }
    }

    let measures = measures_prepared(&vertices);
    Ok(Validated {
        vertices,
        source_indices,
        measures,
    })
}