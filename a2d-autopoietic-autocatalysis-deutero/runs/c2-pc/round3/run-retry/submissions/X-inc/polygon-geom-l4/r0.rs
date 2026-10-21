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

const MAX_COORD: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

fn in_range(p: Point) -> bool {
    (-MAX_COORD..=MAX_COORD).contains(&p.x) && (-MAX_COORD..=MAX_COORD).contains(&p.y)
}

fn prepare(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &p) in points.iter().enumerate() {
        if !in_range(p) {
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

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    let squared = (dx * dx + dy * dy) as u128 * 1_000_000;
    let root = isqrt(squared);
    let rounded = if squared - root * root >= root + 1 {
        root + 1
    } else {
        root
    };
    rounded as u64
}

fn measures_unchecked(points: &[Point]) -> Measures {
    let signed_area2 = shoelace(points);
    let mut edge_milli = Vec::with_capacity(points.len());
    let mut perimeter_milli = 0u64;
    let mut longest_edge = None;
    let mut best_pair: Option<(Point, Point)> = None;

    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let length = rounded_edge_milli(a, b);
        edge_milli.push(length);
        perimeter_milli += length;

        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        let pair = (low, high);
        match longest_edge {
            None => {
                longest_edge = Some(i);
                best_pair = Some(pair);
            }
            Some(best) => {
                if length > edge_milli[best]
                    || (length == edge_milli[best]
                        && (pair < best_pair.unwrap()
                            || (pair == best_pair.unwrap() && i < best)))
                {
                    longest_edge = Some(i);
                    best_pair = Some(pair);
                }
            }
        }
    }

    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
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

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let effective = prepare(points)?;
    Ok(measures_unchecked(&effective))
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
    for (index, &q) in queries.iter().enumerate() {
        if !in_range(q) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    let mut results = Vec::with_capacity(queries.len());
    for &q in queries {
        let mut boundary = None;
        for i in 0..polygon.len() {
            if on_segment(polygon[i], polygon[(i + 1) % polygon.len()], q) {
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
            if (a.y > q.y) != (b.y > q.y) {
                let c = cross(a, b, q);
                if (b.y > a.y && c > 0) || (b.y < a.y && c < 0) {
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
    (ab_c < 0) != (ab_d < 0) && (cd_a < 0) != (cd_b < 0)
}

pub fn validate(points: &[Point]) -> Result<Validated, GeomError> {
    let effective = prepare(points)?;
    let n = effective.len();

    for i in 0..n {
        for j in i + 1..n {
            if effective[i] == effective[j] {
                return Err(GeomError::Duplicate {
                    first: i,
                    second: j,
                });
            }
        }
    }

    let area = shoelace(&effective);
    if area == 0 {
        return Err(GeomError::ZeroArea);
    }

    for i in 0..n {
        let prev = effective[(i + n - 1) % n];
        let current = effective[i];
        let next = effective[(i + 1) % n];
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
        let a = effective[i];
        let b = effective[(i + 1) % n];
        for j in i + 1..n {
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

    let start = (0..n).min_by_key(|&i| effective[i]).unwrap();
    let mut vertices = Vec::with_capacity(n);
    let mut source_indices = Vec::with_capacity(n);
    for offset in 0..n {
        let index = if area > 0 {
            (start + offset) % n
        } else {
            (start + n - offset % n) % n
        };
        vertices.push(effective[index]);
        source_indices.push(index);
    }

    let measures = measures_unchecked(&vertices);
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
    for (index, &p) in points.iter().enumerate() {
        if !in_range(p) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (index, &point) in points.iter().enumerate() {
        if let Some(existing) = unique.iter_mut().find(|item| item.0 == point) {
            existing.1 = existing.1.min(index);
        } else {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|item| item.0);

    if unique.len() <= 1 {
        let vertices: Vec<Point> = unique.iter().map(|x| x.0).collect();
        let source_indices: Vec<usize> = unique.iter().map(|x| x.1).collect();
        return Ok(Hull {
            vertices,
            source_indices,
            area2: 0,
            perimeter_milli: 0,
        });
    }

    let all_collinear = unique
        .iter()
        .skip(2)
        .all(|item| cross(unique[0].0, unique[1].0, item.0) == 0);

    let hull_pairs: Vec<(Point, usize)> = if all_collinear {
        match mode {
            Collinear::Include => unique.clone(),
            Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
        }
    } else {
        let mut lower: Vec<(Point, usize)> = Vec::new();
        for &item in &unique {
            while lower.len() >= 2 {
                let c = cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, item.0);
                if if mode == Collinear::Include { c < 0 } else { c <= 0 } {
                    lower.pop();
                } else {
                    break;
                }
            }
            lower.push(item);
        }

        let mut upper: Vec<(Point, usize)> = Vec::new();
        for &item in unique.iter().rev() {
            while upper.len() >= 2 {
                let c = cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, item.0);
                if if mode == Collinear::Include { c < 0 } else { c <= 0 } {
                    upper.pop();
                } else {
                    break;
                }
            }
            upper.push(item);
        }

        lower.pop();
        upper.pop();
        lower.extend(upper);
        lower
    };

    let vertices: Vec<Point> = hull_pairs.iter().map(|item| item.0).collect();
    let source_indices: Vec<usize> = hull_pairs.iter().map(|item| item.1).collect();
    let area2 = if vertices.len() >= 3 {
        shoelace(&vertices).unsigned_abs()
    } else {
        0
    };

    let perimeter_milli = if vertices.len() >= 2 {
        let mut sum = 0u64;
        for i in 0..vertices.len() {
            sum += rounded_edge_milli(vertices[i], vertices[(i + 1) % vertices.len()]);
        }
        sum
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