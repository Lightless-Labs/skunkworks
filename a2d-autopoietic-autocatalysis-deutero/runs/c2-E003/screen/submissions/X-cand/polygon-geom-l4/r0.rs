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

fn prepare(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > 4096 {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (i, &p) in points.iter().enumerate() {
        if !in_range(p) {
            return Err(GeomError::OutOfRange { index: i });
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
    let n = points.len();
    let mut sum = 0i128;
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        sum += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }
    sum
}

fn edge_milli(a: Point, b: Point) -> u64 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    let squared = (dx * dx + dy * dy) as u128 * 1_000_000;
    let mut lo = 0u128;
    let mut hi = 3_000_000_000u128;
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if mid * mid <= squared {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let remainder = squared - lo * lo;
    if remainder >= lo * 2 + 1 {
        (lo + 1) as u64
    } else {
        lo as u64
    }
}

fn measures_for(points: &[Point]) -> Measures {
    let signed_area2 = shoelace(points);
    let n = points.len();
    let edge_milli: Vec<u64> = (0..n)
        .map(|i| edge_milli(points[i], points[(i + 1) % n]))
        .collect();
    let perimeter_milli = edge_milli.iter().copied().sum();

    let longest_edge = if n == 0 {
        None
    } else {
        let mut best = 0usize;
        for i in 1..n {
            if edge_milli[i] > edge_milli[best] {
                best = i;
            } else if edge_milli[i] == edge_milli[best] {
                let pair = |idx: usize| {
                    let a = points[idx];
                    let b = points[(idx + 1) % n];
                    if a <= b { (a, b) } else { (b, a) }
                };
                if pair(i) < pair(best) || (pair(i) == pair(best) && i < best) {
                    best = i;
                }
            }
        }
        Some(best)
    };

    Measures {
        signed_area2,
        area2: signed_area2.unsigned_abs(),
        orientation: match signed_area2.cmp(&0) {
            std::cmp::Ordering::Less => Orientation::Clockwise,
            std::cmp::Ordering::Equal => Orientation::Degenerate,
            std::cmp::Ordering::Greater => Orientation::CounterClockwise,
        },
        edge_milli,
        perimeter_milli,
        longest_edge,
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    Ok(measures_for(&prepare(points)?))
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
    for (i, &q) in queries.iter().enumerate() {
        if !in_range(q) {
            return Err(GeomError::QueryOutOfRange { index: i });
        }
    }

    let n = polygon.len();
    let mut result = Vec::with_capacity(queries.len());
    for &q in queries {
        let mut boundary = None;
        for i in 0..n {
            if on_segment(polygon[i], polygon[(i + 1) % n], q) {
                boundary = Some(i);
                break;
            }
        }
        if let Some(edge) = boundary {
            result.push(Location::Boundary { edge });
            continue;
        }

        let mut inside = false;
        for i in 0..n {
            let a = polygon[i];
            let b = polygon[(i + 1) % n];
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

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    if points.len() > 4096 {
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
    unique.sort_by_key(|&(p, _)| p);

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
        .all(|&(p, _)| cross(unique[0].0, unique[1].0, p) == 0);

    let selected: Vec<(Point, usize)> = if all_collinear {
        if mode == Collinear::Include {
            unique.clone()
        } else {
            vec![unique[0], *unique.last().unwrap()]
        }
    } else {
        let mut lower: Vec<(Point, usize)> = Vec::new();
        for &item in &unique {
            while lower.len() >= 2 {
                let turn = cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, item.0);
                if if mode == Collinear::Include {
                    turn < 0
                } else {
                    turn <= 0
                } {
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
                let turn = cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, item.0);
                if if mode == Collinear::Include {
                    turn < 0
                } else {
                    turn <= 0
                } {
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

    let vertices: Vec<Point> = selected.iter().map(|x| x.0).collect();
    let source_indices: Vec<usize> = selected.iter().map(|x| x.1).collect();
    let signed = if vertices.len() >= 3 {
        shoelace(&vertices)
    } else {
        0
    };
    let perimeter_milli = if vertices.len() >= 2 {
        (0..vertices.len())
            .map(|i| edge_milli(vertices[i], vertices[(i + 1) % vertices.len()]))
            .sum()
    } else {
        0
    };

    Ok(Hull {
        vertices,
        source_indices,
        area2: signed.unsigned_abs(),
        perimeter_milli,
    })
}

fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let o1 = cross(a, b, c);
    let o2 = cross(a, b, d);
    let o3 = cross(c, d, a);
    let o4 = cross(c, d, b);

    if o1 == 0 && on_segment(a, b, c) {
        return true;
    }
    if o2 == 0 && on_segment(a, b, d) {
        return true;
    }
    if o3 == 0 && on_segment(c, d, a) {
        return true;
    }
    if o4 == 0 && on_segment(c, d, b) {
        return true;
    }
    (o1 < 0) != (o2 < 0) && (o3 < 0) != (o4 < 0)
}

pub fn validate(points: &[Point]) -> Result<Validated, GeomError> {
    let polygon = prepare(points)?;
    let n = polygon.len();

    let mut duplicate = None;
    for i in 0..n {
        for j in i + 1..n {
            if polygon[i] == polygon[j] {
                duplicate = Some((i, j));
                break;
            }
        }
        if duplicate.is_some() {
            break;
        }
    }
    if let Some((first, second)) = duplicate {
        return Err(GeomError::Duplicate { first, second });
    }

    let signed_area = shoelace(&polygon);
    if signed_area == 0 {
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

    let mut min_index = 0usize;
    for i in 1..n {
        if polygon[i] < polygon[min_index] {
            min_index = i;
        }
    }

    let mut order: Vec<usize> = Vec::with_capacity(n);
    if signed_area > 0 {
        for k in 0..n {
            order.push((min_index + k) % n);
        }
    } else {
        for k in 0..n {
            order.push((min_index + n - k) % n);
        }
    }

    let vertices: Vec<Point> = order.iter().map(|&i| polygon[i]).collect();
    let source_indices = order;
    let measures = measures_for(&vertices);
    Ok(Validated {
        vertices,
        source_indices,
        measures,
    })
}