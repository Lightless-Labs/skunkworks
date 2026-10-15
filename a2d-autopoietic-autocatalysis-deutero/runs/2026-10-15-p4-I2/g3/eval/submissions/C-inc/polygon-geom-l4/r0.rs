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
    let abx = b.x as i128 - a.x as i128;
    let aby = b.y as i128 - a.y as i128;
    let acx = c.x as i128 - a.x as i128;
    let acy = c.y as i128 - a.y as i128;
    abx * acy - aby * acx
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
    let mut x = 1u128 << ((128 - n.leading_zeros() as usize + 1) / 2);
    loop {
        let y = (x + n / x) / 2;
        if y >= x {
            return x;
        }
        x = y;
    }
}

fn rounded_length(a: Point, b: Point) -> u64 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    let square = (dx * dx + dy * dy) as u128;
    let scaled = square * 1_000_000u128;
    let root = isqrt(scaled);
    let remainder = scaled - root * root;
    let rounded = root + u128::from(remainder >= root * 2 + 1);
    rounded as u64
}

fn measures_unchecked(points: &[Point]) -> Measures {
    let signed_area2 = shoelace(points);
    let mut edge_milli = Vec::with_capacity(points.len());
    let mut perimeter_milli = 0u64;
    let mut longest_edge = None;

    for i in 0..points.len() {
        let length = rounded_length(points[i], points[(i + 1) % points.len()]);
        edge_milli.push(length);
        perimeter_milli += length;

        let should_replace = match longest_edge {
            None => true,
            Some(best) if length > edge_milli[best] => true,
            Some(best) if length == edge_milli[best] => {
                let candidate = ordered_pair(points[i], points[(i + 1) % points.len()]);
                let incumbent = ordered_pair(points[best], points[(best + 1) % points.len()]);
                candidate < incumbent || (candidate == incumbent && i < best)
            }
            _ => false,
        };
        if should_replace {
            longest_edge = Some(i);
        }
    }

    Measures {
        signed_area2,
        area2: signed_area2.unsigned_abs(),
        orientation: if signed_area2 > 0 {
            Orientation::CounterClockwise
        } else if signed_area2 < 0 {
            Orientation::Clockwise
        } else {
            Orientation::Degenerate
        },
        edge_milli,
        perimeter_milli,
        longest_edge,
    }
}

fn ordered_pair(a: Point, b: Point) -> (Point, Point) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
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
    for (index, &query) in queries.iter().enumerate() {
        if !in_range(query) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    Ok(queries
        .iter()
        .map(|&query| {
            for i in 0..polygon.len() {
                if on_segment(polygon[i], polygon[(i + 1) % polygon.len()], query) {
                    return Location::Boundary { edge: i };
                }
            }

            let mut inside = false;
            for i in 0..polygon.len() {
                let a = polygon[i];
                let b = polygon[(i + 1) % polygon.len()];
                if (a.y > query.y) != (b.y > query.y) {
                    let orientation = cross(a, b, query);
                    if (b.y > a.y && orientation > 0) || (b.y < a.y && orientation < 0) {
                        inside = !inside;
                    }
                }
            }
            if inside {
                Location::Inside
            } else {
                Location::Outside
            }
        })
        .collect())
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
        if !unique.iter().any(|(seen, _)| *seen == point) {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|&(point, _)| point);
    Ok(unique)
}

fn all_collinear(points: &[(Point, usize)]) -> bool {
    points.len() < 3 || points[2..].iter().all(|&(p, _)| cross(points[0].0, points[1].0, p) == 0)
}

fn hull_measure(vertices: &[Point]) -> (u128, u64) {
    if vertices.is_empty() {
        return (0, 0);
    }
    let area = if vertices.len() >= 3 {
        shoelace(vertices).unsigned_abs()
    } else {
        0
    };
    let perimeter = if vertices.len() == 1 {
        0
    } else {
        (0..vertices.len())
            .map(|i| rounded_length(vertices[i], vertices[(i + 1) % vertices.len()]))
            .sum()
    };
    (area, perimeter)
}

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    let sorted = cloud(points)?;
    if sorted.is_empty() {
        return Ok(Hull {
            vertices: Vec::new(),
            source_indices: Vec::new(),
            area2: 0,
            perimeter_milli: 0,
        });
    }
    if sorted.len() == 1 {
        let (area2, perimeter_milli) = hull_measure(&[sorted[0].0]);
        return Ok(Hull {
            vertices: vec![sorted[0].0],
            source_indices: vec![sorted[0].1],
            area2,
            perimeter_milli,
        });
    }

    if all_collinear(&sorted) {
        let chosen: Vec<_> = match mode {
            Collinear::Include => sorted,
            Collinear::Exclude => vec![sorted[0], *sorted.last().unwrap()],
        };
        let vertices: Vec<_> = chosen.iter().map(|x| x.0).collect();
        let source_indices: Vec<_> = chosen.iter().map(|x| x.1).collect();
        let (area2, perimeter_milli) = hull_measure(&vertices);
        return Ok(Hull {
            vertices,
            source_indices,
            area2,
            perimeter_milli,
        });
    }

    let mut lower: Vec<(Point, usize)> = Vec::new();
    for &point in &sorted {
        while lower.len() >= 2 {
            let turn = cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, point.0);
            if turn > 0 || (mode == Collinear::Include && turn == 0) {
                break;
            }
            lower.pop();
        }
        lower.push(point);
    }

    let mut upper: Vec<(Point, usize)> = Vec::new();
    for &point in sorted.iter().rev() {
        while upper.len() >= 2 {
            let turn = cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, point.0);
            if turn > 0 || (mode == Collinear::Include && turn == 0) {
                break;
            }
            upper.pop();
        }
        upper.push(point);
    }

    lower.pop();
    upper.pop();
    lower.extend(upper);

    let vertices: Vec<_> = lower.iter().map(|x| x.0).collect();
    let source_indices: Vec<_> = lower.iter().map(|x| x.1).collect();
    let (area2, perimeter_milli) = hull_measure(&vertices);
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
    (ab_c > 0) != (ab_d > 0) && (cd_a > 0) != (cd_b > 0)
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

    let area = shoelace(&effective);
    if area == 0 {
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

    let minimum = (0..n).min_by_key(|&i| effective[i]).unwrap();
    let direction: isize = if area > 0 { 1 } else { -1 };
    let mut order = Vec::with_capacity(n);
    for step in 0..n {
        order.push((minimum as isize + direction * step as isize).rem_euclid(n as isize) as usize);
    }

    let vertices: Vec<_> = order.iter().map(|&i| effective[i]).collect();
    let source_indices = order;
    let measures = measures_unchecked(&vertices);
    Ok(Validated {
        vertices,
        source_indices,
        measures,
    })
}

fn main() {}