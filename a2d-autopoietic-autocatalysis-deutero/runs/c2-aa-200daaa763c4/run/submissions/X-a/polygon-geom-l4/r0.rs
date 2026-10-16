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
    for (i, &p) in points.iter().enumerate() {
        if !in_range(p) {
            return Err(GeomError::OutOfRange { index: i });
        }
    }

    let mut len = points.len();
    if len >= 2 && points[0] == points[len - 1] {
        len -= 1;
    }
    if len < 3 {
        return Err(GeomError::TooFewVertices { len });
    }

    Ok(points[..len].to_vec())
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

fn rounded_sqrt(n: u128) -> u128 {
    let mut lo = 0u128;
    let mut hi = 1u128;
    while hi * hi < n {
        hi *= 2;
    }
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if mid * mid < n {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    let lower_sq = lo * lo;
    let upper_sq = hi * hi;
    if n - lower_sq >= upper_sq - n {
        hi
    } else {
        lo
    }
}

fn edge_mill(a: Point, b: Point) -> u64 {
    let dx = (a.x as i128 - b.x as i128).unsigned_abs();
    let dy = (a.y as i128 - b.y as i128).unsigned_abs();
    rounded_sqrt((dx * dx + dy * dy) * 1_000_000) as u64
}

fn measures(points: &[Point]) -> Measures {
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

    let longest_edge = (0..points.len()).max_by(|&i, &j| {
        edge_milli[i]
            .cmp(&edge_milli[j])
            .then_with(|| {
                let pair = |k: usize| {
                    let a = points[k];
                    let b = points[(k + 1) % points.len()];
                    if a <= b { (a, b) } else { (b, a) }
                };
                pair(j).cmp(&pair(i))
            })
            .then_with(|| j.cmp(&i))
    });

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
    let points = prepare(points)?;
    Ok(measures(&points))
}

fn on_segment(a: Point, b: Point, p: Point) -> bool {
    cross(a, b, p) == 0
        && p.x >= a.x.min(b.x)
        && p.x <= a.x.max(b.x)
        && p.y >= a.y.min(b.y)
        && p.y <= a.y.max(b.y)
}

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let polygon = prepare(points)?;
    if queries.len() > MAX_VERTICES {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (i, &q) in queries.iter().enumerate() {
        if !in_range(q) {
            return Err(GeomError::QueryOutOfRange { index: i });
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
                let c = cross(a, b, q);
                if (b.y > a.y && c > 0) || (b.y < a.y && c < 0) {
                    inside = !inside;
                }
            }
        }

        result.push(match boundary {
            Some(edge) => Location::Boundary { edge },
            None if inside => Location::Inside,
            None => Location::Outside,
        });
    }
    Ok(result)
}

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    if points.len() > MAX_VERTICES {
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

    let mut hull: Vec<(Point, usize)> = if unique.len() <= 1 {
        unique.clone()
    } else {
        let all_collinear = unique[2..]
            .iter()
            .all(|&(p, _)| cross(unique[0].0, unique[1].0, p) == 0);

        if all_collinear {
            match mode {
                Collinear::Include => unique.clone(),
                Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
            }
        } else {
            let mut lower: Vec<(Point, usize)> = Vec::new();
            for &item in &unique {
                while lower.len() >= 2 {
                    let n = lower.len();
                    let c = cross(lower[n - 2].0, lower[n - 1].0, item.0);
                    if if mode == Collinear::Include {
                        c < 0
                    } else {
                        c <= 0
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
                    let n = upper.len();
                    let c = cross(upper[n - 2].0, upper[n - 1].0, item.0);
                    if if mode == Collinear::Include {
                        c < 0
                    } else {
                        c <= 0
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
        }
    };

    if hull.len() >= 3 && shoelace(&hull.iter().map(|x| x.0).collect::<Vec<_>>()) < 0 {
        hull.reverse();
    }
    if hull.len() >= 2 {
        let min_pos = hull
            .iter()
            .enumerate()
            .min_by_key(|(_, x)| x.0)
            .map(|(i, _)| i)
            .unwrap();
        hull.rotate_left(min_pos);
    }

    let vertices: Vec<Point> = hull.iter().map(|x| x.0).collect();
    let source_indices: Vec<usize> = hull.iter().map(|x| x.1).collect();
    let area2 = shoelace(&vertices).unsigned_abs();
    let perimeter_milli = if vertices.len() >= 2 {
        (0..vertices.len())
            .map(|i| edge_mill(vertices[i], vertices[(i + 1) % vertices.len()]))
            .sum()
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
        for j in i + 1..n {
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

    let min_index = (0..n).min_by_key(|&i| effective[i]).unwrap();
    let forward = area > 0;
    let mut order = Vec::with_capacity(n);
    for k in 0..n {
        let index = if forward {
            (min_index + k) % n
        } else {
            (min_index + n - k) % n
        };
        order.push(index);
    }

    let vertices: Vec<Point> = order.iter().map(|&i| effective[i]).collect();
    let measures = measures(&vertices);
    Ok(Validated {
        vertices,
        source_indices: order,
        measures,
    })
}