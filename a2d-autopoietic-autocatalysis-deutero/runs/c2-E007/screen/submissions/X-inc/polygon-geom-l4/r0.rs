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

fn cross(a: Point, b: Point, q: Point) -> i128 {
    (b.x as i128 - a.x as i128) * (q.y as i128 - a.y as i128)
        - (b.y as i128 - a.y as i128) * (q.x as i128 - a.x as i128)
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

fn rounded_sqrt_million(d2: u128) -> u64 {
    let target = d2 * 1_000_000;
    let mut lo = 0u128;
    let mut hi = 3_000_000_000_000u128;
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if mid * mid <= target {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let lower = lo;
    let upper = lower + 1;
    if target - lower * lower >= upper * upper - target {
        upper as u64
    } else {
        lower as u64
    }
}

fn edge_length_milli(a: Point, b: Point) -> u64 {
    let dx = a.x as i128 - b.x as i128;
    let dy = a.y as i128 - b.y as i128;
    let d2 = (dx * dx + dy * dy) as u128;
    rounded_sqrt_million(d2)
}

fn endpoint_pair(a: Point, b: Point) -> (Point, Point) {
    if a <= b { (a, b) } else { (b, a) }
}

fn measures_prepared(points: &[Point]) -> Measures {
    let signed_area2 = shoelace(points);
    let n = points.len();
    let edge_milli: Vec<u64> = (0..n)
        .map(|i| edge_length_milli(points[i], points[(i + 1) % n]))
        .collect();
    let perimeter_milli = edge_milli.iter().copied().sum();

    let mut longest_edge = None;
    for i in 0..n {
        match longest_edge {
            None => longest_edge = Some(i),
            Some(best) => {
                if edge_milli[i] > edge_milli[best]
                    || (edge_milli[i] == edge_milli[best]
                        && (endpoint_pair(points[i], points[(i + 1) % n]),
                            i)
                            < (endpoint_pair(
                                points[best],
                                points[(best + 1) % n],
                            ), best))
                {
                    longest_edge = Some(i);
                }
            }
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

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let effective = prepare_polygon(points)?;
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
    let polygon = prepare_polygon(points)?;
    if queries.len() > MAX_VERTICES {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (index, &query) in queries.iter().enumerate() {
        if !in_range(query) {
            return Err(GeomError::QueryOutOfRange { index });
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

fn orient(a: Point, b: Point, c: Point) -> i128 {
    cross(a, b, c)
}

fn between(a: i64, b: i64, x: i64) -> bool {
    x >= a.min(b) && x <= a.max(b)
}

fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let o1 = orient(a, b, c);
    let o2 = orient(a, b, d);
    let o3 = orient(c, d, a);
    let o4 = orient(c, d, b);

    if o1 == 0 && between(a.x, b.x, c.x) && between(a.y, b.y, c.y) {
        return true;
    }
    if o2 == 0 && between(a.x, b.x, d.x) && between(a.y, b.y, d.y) {
        return true;
    }
    if o3 == 0 && between(c.x, d.x, a.x) && between(c.y, d.y, a.y) {
        return true;
    }
    if o4 == 0 && between(c.x, d.x, b.x) && between(c.y, d.y, b.y) {
        return true;
    }
    (o1 > 0) != (o2 > 0) && (o3 > 0) != (o4 > 0)
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

    let start = (0..n).min_by_key(|&i| effective[i]).unwrap();
    let mut indices: Vec<usize> = if area > 0 {
        (0..n).map(|k| (start + k) % n).collect()
    } else {
        (0..n).map(|k| (start + n - k) % n).collect()
    };
    let vertices: Vec<Point> = indices.iter().map(|&i| effective[i]).collect();
    let measures = measures_prepared(&vertices);
    Ok(Validated {
        vertices,
        source_indices: std::mem::take(&mut indices),
        measures,
    })
}

fn cross_points(a: Point, b: Point) -> i128 {
    a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128
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
        if let Some(entry) = unique.iter_mut().find(|entry| entry.0 == point) {
            entry.1 = entry.1.min(index);
        } else {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|entry| entry.0);

    let mut hull: Vec<(Point, usize)> = if unique.len() <= 1 {
        unique.clone()
    } else {
        let all_collinear = unique
            .windows(3)
            .all(|w| cross(w[0].0, w[1].0, w[2].0) == 0);

        if all_collinear {
            if mode == Collinear::Include {
                unique.clone()
            } else {
                vec![unique[0], *unique.last().unwrap()]
            }
        } else {
            let mut lower: Vec<(Point, usize)> = Vec::new();
            for &point in &unique {
                while lower.len() >= 2 {
                    let c = cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, point.0);
                    if if mode == Collinear::Include { c < 0 } else { c <= 0 } {
                        lower.pop();
                    } else {
                        break;
                    }
                }
                lower.push(point);
            }

            let mut upper: Vec<(Point, usize)> = Vec::new();
            for &point in unique.iter().rev() {
                while upper.len() >= 2 {
                    let c = cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, point.0);
                    if if mode == Collinear::Include { c < 0 } else { c <= 0 } {
                        upper.pop();
                    } else {
                        break;
                    }
                }
                upper.push(point);
            }
            lower.pop();
            upper.pop();
            lower.extend(upper);
            lower
        }
    };

    // Andrew's traversal begins at the lexicographic minimum and is counterclockwise.
    let vertices: Vec<Point> = hull.iter().map(|entry| entry.0).collect();
    let source_indices: Vec<usize> = hull.iter().map(|entry| entry.1).collect();
    let signed_area = shoelace(&vertices);
    let area2 = signed_area.unsigned_abs();
    let perimeter_milli = if vertices.len() <= 1 {
        0
    } else {
        (0..vertices.len())
            .map(|i| edge_length_milli(vertices[i], vertices[(i + 1) % vertices.len()]))
            .sum()
    };

    Ok(Hull {
        vertices,
        source_indices,
        area2,
        perimeter_milli,
    })
}