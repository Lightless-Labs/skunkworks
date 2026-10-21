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
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let effective_len = if points.len() >= 2 && points.last() == points.first() {
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

fn signed_area2(points: &[Point]) -> i128 {
    let mut sum = 0_i128;
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

fn rounded_length_milli(a: Point, b: Point) -> u64 {
    let dx = a.x as i128 - b.x as i128;
    let dy = a.y as i128 - b.y as i128;
    let squared = (dx * dx + dy * dy) as u128 * 1_000_000;
    let floor = isqrt(squared);
    let remainder = squared - floor * floor;
    let rounded = if remainder >= floor + 1 { floor + 1 } else { floor };
    rounded as u64
}

fn endpoint_pair(a: Point, b: Point) -> (Point, Point) {
    if a <= b { (a, b) } else { (b, a) }
}

fn measures_for(points: &[Point]) -> Measures {
    let signed = signed_area2(points);
    let edge_milli: Vec<u64> = (0..points.len())
        .map(|i| rounded_length_milli(points[i], points[(i + 1) % points.len()]))
        .collect();
    let perimeter_milli = edge_milli.iter().copied().sum();
    let mut longest_edge = None;
    for i in 0..points.len() {
        match longest_edge {
            None => longest_edge = Some(i),
            Some(best) => {
                if edge_milli[i] > edge_milli[best]
                    || (edge_milli[i] == edge_milli[best]
                        && (endpoint_pair(points[i], points[(i + 1) % points.len()]),
                            i)
                            < (
                                endpoint_pair(
                                    points[best],
                                    points[(best + 1) % points.len()],
                                ),
                                best,
                            ))
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
        perimeter_milli,
        longest_edge,
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
    for (index, &query) in queries.iter().enumerate() {
        if !in_range(query) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    Ok(queries
        .iter()
        .copied()
        .map(|q| {
            for i in 0..polygon.len() {
                if on_segment(polygon[i], polygon[(i + 1) % polygon.len()], q) {
                    return Location::Boundary { edge: i };
                }
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
            if inside { Location::Inside } else { Location::Outside }
        })
        .collect())
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
        if !unique.iter().any(|&(p, _)| p == point) {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|&(point, _)| point);

    if unique.len() <= 1 {
        let vertices: Vec<Point> = unique.iter().map(|&(p, _)| p).collect();
        let source_indices: Vec<usize> = unique.iter().map(|&(_, i)| i).collect();
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

    let hull_pairs: Vec<(Point, usize)> = if all_collinear {
        match mode {
            Collinear::Include => unique,
            Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
        }
    } else {
        let mut lower: Vec<(Point, usize)> = Vec::new();
        for &item in &unique {
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
            lower.push(item);
        }
        let mut upper: Vec<(Point, usize)> = Vec::new();
        for &item in unique.iter().rev() {
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
            upper.push(item);
        }
        lower.pop();
        upper.pop();
        lower.extend(upper);
        lower
    };

    let vertices: Vec<Point> = hull_pairs.iter().map(|&(p, _)| p).collect();
    let source_indices: Vec<usize> = hull_pairs.iter().map(|&(_, i)| i).collect();
    let area = if vertices.len() >= 3 {
        signed_area2(&vertices).unsigned_abs()
    } else {
        0
    };
    let perimeter = if vertices.len() < 2 {
        0
    } else {
        (0..vertices.len())
            .map(|i| rounded_length_milli(vertices[i], vertices[(i + 1) % vertices.len()]))
            .sum()
    };

    Ok(Hull {
        vertices,
        source_indices,
        area2: area,
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
    let effective = prepare(points)?;

    let mut duplicate: Option<(usize, usize)> = None;
    for i in 0..effective.len() {
        for j in (i + 1)..effective.len() {
            if effective[i] == effective[j] {
                let pair = (i, j);
                if duplicate.map_or(true, |best| pair < best) {
                    duplicate = Some(pair);
                }
            }
        }
    }
    if let Some((first, second)) = duplicate {
        return Err(GeomError::Duplicate { first, second });
    }

    let area = signed_area2(&effective);
    if area == 0 {
        return Err(GeomError::ZeroArea);
    }

    let n = effective.len();
    for i in 0..n {
        let prev = effective[(i + n - 1) % n];
        let current = effective[i];
        let next = effective[(i + 1) % n];
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

    let minimum_index = (0..n).min_by_key(|&i| effective[i]).unwrap();
    let forward = area > 0;
    let mut vertices = Vec::with_capacity(n);
    let mut source_indices = Vec::with_capacity(n);
    for offset in 0..n {
        let index = if forward {
            (minimum_index + offset) % n
        } else {
            (minimum_index + n - offset) % n
        };
        vertices.push(effective[index]);
        source_indices.push(index);
    }

    let measures = measures_for(&vertices);
    Ok(Validated {
        vertices,
        source_indices,
        measures,
    })
}