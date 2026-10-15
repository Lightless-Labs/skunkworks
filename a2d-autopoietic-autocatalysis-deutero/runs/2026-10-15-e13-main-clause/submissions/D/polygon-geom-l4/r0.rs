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

fn in_range(p: Point) -> bool {
    (-LIMIT..=LIMIT).contains(&p.x) && (-LIMIT..=LIMIT).contains(&p.y)
}

fn prepare(points: &[Point]) -> Result<&[Point], GeomError> {
    if points.len() > 4096 {
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

fn isqrt(n: u128) -> u128 {
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

fn edge_length_milli(a: Point, b: Point) -> u64 {
    let dx = a.x as i128 - b.x as i128;
    let dy = a.y as i128 - b.y as i128;
    let squared = (dx * dx + dy * dy) as u128;
    let scaled = squared * 1_000_000;
    let root = isqrt(scaled);
    let rounded = if scaled - root * root >= root + 1 {
        root + 1
    } else {
        root
    };
    rounded as u64
}

fn measures_for(points: &[Point]) -> Measures {
    let signed_area2 = shoelace(points);
    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
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

fn edge_tie_key(points: &[Point], i: usize) -> (Point, Point) {
    let a = points[i];
    let b = points[(i + 1) % points.len()];
    if a <= b { (a, b) } else { (b, a) }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    Ok(measures_for(prepare(points)?))
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
    if queries.len() > 4096 {
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
    let polygon = prepare(points)?;
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

    let area = shoelace(polygon);
    if area == 0 {
        return Err(GeomError::ZeroArea);
    }

    for i in 0..n {
        let prev = polygon[(i + n - 1) % n];
        let current = polygon[i];
        let next = polygon[(i + 1) % n];
        if cross(prev, current, next) == 0 {
            let dot = (prev.x as i128 - current.x as i128)
                * (next.x as i128 - current.x as i128)
                + (prev.y as i128 - current.y as i128)
                    * (next.y as i128 - current.y as i128);
            if dot > 0 {
                return Err(GeomError::AdjacentOverlap { vertex: i });
            }
        }
    }

    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];
        for j in i + 1..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let c = polygon[j];
            let d = polygon[(j + 1) % n];
            if segments_intersect(a, b, c, d) {
                return Err(GeomError::Intersection {
                    edge_a: i,
                    edge_b: j,
                });
            }
        }
    }

    let minimum_index = (0..n).min_by_key(|&i| polygon[i]).unwrap();
    let forward = area > 0;
    let mut vertices = Vec::with_capacity(n);
    let mut source_indices = Vec::with_capacity(n);
    for offset in 0..n {
        let index = if forward {
            (minimum_index + offset) % n
        } else {
            (minimum_index + n - offset % n) % n
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

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    if points.len() > 4096 {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (index, &point) in points.iter().enumerate() {
        if let Some(entry) = unique.iter_mut().find(|entry| entry.0 == point) {
            entry.1 = entry.1.min(index);
        } else {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|entry| entry.0);

    if unique.len() <= 1 {
        let vertices: Vec<Point> = unique.iter().map(|entry| entry.0).collect();
        let source_indices: Vec<usize> = unique.iter().map(|entry| entry.1).collect();
        let perimeter_milli = if vertices.len() == 1 { 0 } else { 0 };
        return Ok(Hull {
            vertices,
            source_indices,
            area2: 0,
            perimeter_milli,
        });
    }

    let all_collinear = unique
        .iter()
        .skip(2)
        .all(|entry| cross(unique[0].0, unique[1].0, entry.0) == 0);

    let hull_entries: Vec<(Point, usize)> = if all_collinear {
        match mode {
            Collinear::Include => unique.clone(),
            Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
        }
    } else {
        let mut lower: Vec<(Point, usize)> = Vec::new();
        for &entry in &unique {
            while lower.len() >= 2 {
                let turn = cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, entry.0);
                let remove = match mode {
                    Collinear::Exclude => turn <= 0,
                    Collinear::Include => turn < 0,
                };
                if remove {
                    lower.pop();
                } else {
                    break;
                }
            }
            lower.push(entry);
        }

        let mut upper: Vec<(Point, usize)> = Vec::new();
        for &entry in unique.iter().rev() {
            while upper.len() >= 2 {
                let turn = cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, entry.0);
                let remove = match mode {
                    Collinear::Exclude => turn <= 0,
                    Collinear::Include => turn < 0,
                };
                if remove {
                    upper.pop();
                } else {
                    break;
                }
            }
            upper.push(entry);
        }
        lower.pop();
        upper.pop();
        lower.extend(upper);
        lower
    };

    let vertices: Vec<Point> = hull_entries.iter().map(|entry| entry.0).collect();
    let source_indices: Vec<usize> = hull_entries.iter().map(|entry| entry.1).collect();
    let signed_area2 = if vertices.len() >= 3 {
        shoelace(&vertices)
    } else {
        0
    };
    let perimeter_milli = if vertices.len() >= 2 {
        (0..vertices.len())
            .map(|i| edge_length_milli(vertices[i], vertices[(i + 1) % vertices.len()]))
            .sum()
    } else {
        0
    };

    Ok(Hull {
        vertices,
        source_indices,
        area2: signed_area2.unsigned_abs(),
        perimeter_milli,
    })
}