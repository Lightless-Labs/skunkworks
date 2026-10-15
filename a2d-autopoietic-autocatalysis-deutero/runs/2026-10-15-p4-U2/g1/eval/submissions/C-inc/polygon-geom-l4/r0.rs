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
    for (index, &p) in points.iter().enumerate() {
        if !in_range(p) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let effective = if points.len() >= 2 && points[0] == points[points.len() - 1] {
        &points[..points.len() - 1]
    } else {
        points
    };

    if effective.len() < 3 {
        return Err(GeomError::TooFewVertices {
            len: effective.len(),
        });
    }
    Ok(effective)
}

fn cross(a: Point, b: Point, c: Point) -> i128 {
    (b.x as i128 - a.x as i128) * (c.y as i128 - a.y as i128)
        - (b.y as i128 - a.y as i128) * (c.x as i128 - a.x as i64 as i128)
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
    let n = (dx * dx + dy * dy) as u128 * 1_000_000;
    let root = isqrt(n);
    let rounded = if n - root * root >= root + 1 {
        root + 1
    } else {
        root
    };
    rounded as u64
}

fn measures_prepared(points: &[Point]) -> Measures {
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
    let mut longest_edge = None;
    let mut longest_key: Option<(u64, Point, Point, usize)> = None;

    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let length = edge_length_milli(a, b);
        perimeter_milli += length;
        edge_milli.push(length);

        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        let key = (length, low, high, i);
        let replace = match longest_key {
            None => true,
            Some((best_len, best_low, best_high, best_index)) => {
                length > best_len
                    || (length == best_len && (low, high, i) < (best_low, best_high, best_index))
            }
        };
        if replace {
            longest_key = Some(key);
            longest_edge = Some(i);
        }
    }

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
    Ok(measures_prepared(prepare(points)?))
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
    for (index, &q) in queries.iter().enumerate() {
        if !in_range(q) {
            return Err(GeomError::QueryOutOfRange { index });
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
                let side = cross(a, b, q);
                if (b.y > a.y && side > 0) || (b.y < a.y && side < 0) {
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

fn hull_area2(points: &[Point]) -> u128 {
    if points.len() < 3 {
        return 0;
    }
    shoelace(points).unsigned_abs()
}

fn hull_perimeter(points: &[Point]) -> u64 {
    if points.len() < 2 {
        return 0;
    }
    let mut sum = 0u64;
    for i in 0..points.len() {
        sum += edge_length_milli(points[i], points[(i + 1) % points.len()]);
    }
    sum
}

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    if points.len() > 4096 {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &p) in points.iter().enumerate() {
        if !in_range(p) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (index, &p) in points.iter().enumerate() {
        if !unique.iter().any(|(q, _)| *q == p) {
            unique.push((p, index));
        }
    }
    unique.sort_by_key(|(p, _)| *p);

    if unique.len() <= 1 {
        let vertices: Vec<Point> = unique.iter().map(|(p, _)| *p).collect();
        let source_indices: Vec<usize> = unique.iter().map(|(_, i)| *i).collect();
        return Ok(Hull {
            vertices,
            source_indices,
            area2: 0,
            perimeter_milli: 0,
        });
    }

    let all_collinear = unique[2..]
        .iter()
        .all(|(p, _)| cross(unique[0].0, unique[1].0, *p) == 0);

    if all_collinear {
        let selected: Vec<(Point, usize)> = match mode {
            Collinear::Include => unique,
            Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
        };
        let vertices: Vec<Point> = selected.iter().map(|(p, _)| *p).collect();
        let source_indices: Vec<usize> = selected.iter().map(|(_, i)| *i).collect();
        return Ok(Hull {
            area2: 0,
            perimeter_milli: hull_perimeter(&vertices),
            vertices,
            source_indices,
        });
    }

    let mut lower: Vec<(Point, usize)> = Vec::new();
    for item in &unique {
        while lower.len() >= 2 {
            let turn = cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, item.0);
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
        lower.push(*item);
    }

    let mut upper: Vec<(Point, usize)> = Vec::new();
    for item in unique.iter().rev() {
        while upper.len() >= 2 {
            let turn = cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, item.0);
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
        upper.push(*item);
    }

    lower.pop();
    upper.pop();
    lower.extend(upper);

    let vertices: Vec<Point> = lower.iter().map(|(p, _)| *p).collect();
    let source_indices: Vec<usize> = lower.iter().map(|(_, i)| *i).collect();
    Ok(Hull {
        area2: hull_area2(&vertices),
        perimeter_milli: hull_perimeter(&vertices),
        vertices,
        source_indices,
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
    (ab_c > 0) != (ab_d > 0) && (cd_a > 0) != (cd_b > 0)
}

pub fn validate(points: &[Point]) -> Result<Validated, GeomError> {
    let polygon = prepare(points)?;

    for i in 0..polygon.len() {
        for j in i + 1..polygon.len() {
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

    let n = polygon.len();
    for i in 0..n {
        let prev = polygon[(i + n - 1) % n];
        let current = polygon[i];
        let next = polygon[(i + 1) % n];
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

    let start = (0..n).min_by_key(|&i| polygon[i]).unwrap();
    let forward = area > 0;
    let mut vertices = Vec::with_capacity(n);
    let mut source_indices = Vec::with_capacity(n);
    for k in 0..n {
        let index = if forward {
            (start + k) % n
        } else {
            (start + n - k) % n
        };
        vertices.push(polygon[index]);
        source_indices.push(index);
    }

    let measures = measures_prepared(&vertices);
    Ok(Validated {
        vertices,
        source_indices,
        measures,
    })
}

fn main() {}