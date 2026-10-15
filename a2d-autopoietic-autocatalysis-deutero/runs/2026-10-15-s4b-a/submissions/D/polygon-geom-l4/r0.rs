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
const RAW_LIMIT: usize = 4096;

fn in_range(p: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&p.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&p.y)
}

fn prepare_polygon(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > RAW_LIMIT {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &p) in points.iter().enumerate() {
        if !in_range(p) {
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

fn cross_vec(ax: i128, ay: i128, bx: i128, by: i128) -> i128 {
    ax * by - ay * bx
}

fn cross(a: Point, b: Point, c: Point) -> i128 {
    cross_vec(
        b.x as i128 - a.x as i128,
        b.y as i128 - a.y as i128,
        c.x as i128 - a.x as i128,
        c.y as i128 - a.y as i128,
    )
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

fn rounded_length_milli(a: Point, b: Point) -> u64 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    let d2 = (dx * dx + dy * dy) as u128;
    let target = d2 * 1_000_000;
    let mut lo = 0u128;
    let mut hi = 3_000_000_000_000u128;
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if mid * mid <= target {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let lower_square = lo * lo;
    let upper = lo + 1;
    let upper_square = upper * upper;
    if target - lower_square >= upper_square - target {
        upper as u64
    } else {
        lo as u64
    }
}

fn measures_for(points: &[Point]) -> Measures {
    let signed = signed_area2(points);
    let n = points.len();
    let mut edge_milli = Vec::with_capacity(n);
    let mut perimeter_milli = 0u64;
    let mut longest_edge = None;
    let mut longest_pair: Option<(Point, Point)> = None;

    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        let length = rounded_length_milli(a, b);
        edge_milli.push(length);
        perimeter_milli += length;

        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        let pair = (low, high);
        let replace = match longest_edge {
            None => true,
            Some(best) => {
                length > edge_milli[best]
                    || (length == edge_milli[best]
                        && (pair < longest_pair.unwrap()
                            || (pair == longest_pair.unwrap() && i < best)))
            }
        };
        if replace {
            longest_edge = Some(i);
            longest_pair = Some(pair);
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
    if queries.len() > RAW_LIMIT {
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
                if boundary.is_none() {
                    boundary = Some(i);
                }
                continue;
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

fn between_collinear(a: Point, b: Point, p: Point) -> bool {
    cross(a, b, p) == 0
        && p.x >= a.x.min(b.x)
        && p.x <= a.x.max(b.x)
        && p.y >= a.y.min(b.y)
        && p.y <= a.y.max(b.y)
}

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    if points.len() > RAW_LIMIT {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &p) in points.iter().enumerate() {
        if !in_range(p) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (index, &p) in points.iter().enumerate() {
        if !unique.iter().any(|&(q, _)| q == p) {
            unique.push((p, index));
        }
    }
    unique.sort_by_key(|&(p, _)| p);

    let mut vertices: Vec<(Point, usize)> = if unique.len() <= 1 {
        unique
    } else {
        let all_collinear = unique[2..]
            .iter()
            .all(|&(p, _)| cross(unique[0].0, unique[1].0, p) == 0);
        if all_collinear {
            match mode {
                Collinear::Include => unique,
                Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
            }
        } else {
            let mut lower: Vec<(Point, usize)> = Vec::new();
            for &item in &unique {
                while lower.len() >= 2 {
                    let n = lower.len();
                    let c = cross(lower[n - 2].0, lower[n - 1].0, item.0);
                    if c > 0 || (mode == Collinear::Include && c == 0) {
                        break;
                    }
                    lower.pop();
                }
                lower.push(item);
            }

            let mut upper: Vec<(Point, usize)> = Vec::new();
            for &item in unique.iter().rev() {
                while upper.len() >= 2 {
                    let n = upper.len();
                    let c = cross(upper[n - 2].0, upper[n - 1].0, item.0);
                    if c > 0 || (mode == Collinear::Include && c == 0) {
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
        }
    };

    let vertex_points: Vec<Point> = vertices.iter().map(|x| x.0).collect();
    let area2 = if vertex_points.len() >= 3 {
        signed_area2(&vertex_points).unsigned_abs()
    } else {
        0
    };
    let perimeter_milli = if vertex_points.is_empty() {
        0
    } else if vertex_points.len() == 1 {
        0
    } else {
        let mut sum = 0u64;
        for i in 0..vertex_points.len() {
            sum += rounded_length_milli(
                vertex_points[i],
                vertex_points[(i + 1) % vertex_points.len()],
            );
        }
        sum
    };

    let source_indices = vertices.iter().map(|x| x.1).collect();
    vertices.shrink_to_fit();
    Ok(Hull {
        vertices: vertex_points,
        source_indices,
        area2,
        perimeter_milli,
    })
}

fn orientation_sign(a: Point, b: Point, c: Point) -> i8 {
    match cross(a, b, c) {
        v if v > 0 => 1,
        v if v < 0 => -1,
        _ => 0,
    }
}

fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let o1 = orientation_sign(a, b, c);
    let o2 = orientation_sign(a, b, d);
    let o3 = orientation_sign(c, d, a);
    let o4 = orientation_sign(c, d, b);

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
    o1 != o2 && o3 != o4
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

    let area = signed_area2(&effective);
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
            let a = effective[i];
            let b = effective[(i + 1) % n];
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

    let mut min_index = 0;
    for i in 1..n {
        if effective[i] < effective[min_index] {
            min_index = i;
        }
    }
    let forward = area > 0;
    let mut vertices = Vec::with_capacity(n);
    let mut source_indices = Vec::with_capacity(n);
    for k in 0..n {
        let index = if forward {
            (min_index + k) % n
        } else {
            (min_index + n - k) % n
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