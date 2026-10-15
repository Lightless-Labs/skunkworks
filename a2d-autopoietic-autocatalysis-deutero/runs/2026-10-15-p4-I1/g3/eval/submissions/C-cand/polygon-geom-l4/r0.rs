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

fn prepare(points: &[Point]) -> Result<(Vec<Point>, Vec<usize>), GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (i, &p) in points.iter().enumerate() {
        if !in_range(p) {
            return Err(GeomError::OutOfRange { index: i });
        }
    }

    let mut len = points.len();
    if len >= 2 && points[len - 1] == points[0] {
        len -= 1;
    }
    if len < 3 {
        return Err(GeomError::TooFewVertices { len });
    }
    Ok((points[..len].to_vec(), (0..len).collect()))
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

fn edge_mill(a: Point, b: Point) -> u64 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    let d2 = (dx * dx + dy * dy) as u128;
    let target = d2 * 1_000_000u128;

    let mut lo = 0u128;
    let mut hi = 3_000_000_001u128;
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if mid * mid <= target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let remainder = target - lo * lo;
    if remainder >= lo + 1 {
        (lo + 1) as u64
    } else {
        lo as u64
    }
}

fn measures_prepared(points: &[Point]) -> Measures {
    let signed = shoelace(points);
    let n = points.len();
    let mut edge_milli = Vec::with_capacity(n);
    let mut perimeter_milli = 0u64;
    let mut longest_edge: Option<usize> = None;

    for i in 0..n {
        let length = edge_mill(points[i], points[(i + 1) % n]);
        edge_milli.push(length);
        perimeter_milli += length;

        let replace = match longest_edge {
            None => true,
            Some(best) if length > edge_milli[best] => true,
            Some(best) if length < edge_milli[best] => false,
            Some(best) => {
                let pair = if points[i] <= points[(i + 1) % n] {
                    (points[i], points[(i + 1) % n])
                } else {
                    (points[(i + 1) % n], points[i])
                };
                let j = best;
                let best_pair = if points[j] <= points[(j + 1) % n] {
                    (points[j], points[(j + 1) % n])
                } else {
                    (points[(j + 1) % n], points[j])
                };
                pair < best_pair || (pair == best_pair && i < j)
            }
        };
        if replace {
            longest_edge = Some(i);
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
    let (effective, _) = prepare(points)?;
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
    let (poly, _) = prepare(points)?;
    if queries.len() > MAX_VERTICES {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (i, &q) in queries.iter().enumerate() {
        if !in_range(q) {
            return Err(GeomError::QueryOutOfRange { index: i });
        }
    }

    let n = poly.len();
    let mut result = Vec::with_capacity(queries.len());
    for &q in queries {
        let mut boundary = None;
        for i in 0..n {
            if on_segment(poly[i], poly[(i + 1) % n], q) {
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
            let a = poly[i];
            let b = poly[(i + 1) % n];
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

fn segment_intersects(a: Point, b: Point, c: Point, d: Point) -> bool {
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
    (o1 > 0) != (o2 > 0) && (o3 > 0) != (o4 > 0)
}

pub fn validate(points: &[Point]) -> Result<Validated, GeomError> {
    let (effective, _) = prepare(points)?;
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

    let signed = shoelace(&effective);
    if signed == 0 {
        return Err(GeomError::ZeroArea);
    }

    for i in 0..n {
        let prev = effective[(i + n - 1) % n];
        let curr = effective[i];
        let next = effective[(i + 1) % n];
        if cross(prev, curr, next) == 0 {
            let ax = prev.x as i128 - curr.x as i128;
            let ay = prev.y as i128 - curr.y as i128;
            let bx = next.x as i128 - curr.x as i128;
            let by = next.y as i128 - curr.y as i128;
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
            if segment_intersects(a, b, c, d) {
                return Err(GeomError::Intersection {
                    edge_a: i,
                    edge_b: j,
                });
            }
        }
    }

    let min_index = (0..n).min_by_key(|&i| effective[i]).unwrap();
    let mut indices = Vec::with_capacity(n);
    if signed > 0 {
        for k in 0..n {
            indices.push((min_index + k) % n);
        }
    } else {
        for k in 0..n {
            indices.push((min_index + n - k) % n);
        }
    }

    let vertices: Vec<Point> = indices.iter().map(|&i| effective[i]).collect();
    let source_indices = indices;
    let measures = measures_prepared(&vertices);
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
    for (i, &p) in points.iter().enumerate() {
        if !in_range(p) {
            return Err(GeomError::OutOfRange { index: i });
        }
    }

    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (i, &p) in points.iter().enumerate() {
        if let Some((_, first)) = unique.iter_mut().find(|(q, _)| *q == p) {
            if i < *first {
                *first = i;
            }
        } else {
            unique.push((p, i));
        }
    }
    unique.sort_by_key(|&(p, _)| p);

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

    let selected: Vec<(Point, usize)> = if all_collinear {
        match mode {
            Collinear::Include => unique,
            Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
        }
    } else {
        let mut lower: Vec<(Point, usize)> = Vec::new();
        for &item in &unique {
            while lower.len() >= 2 {
                let c = cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, item.0);
                let remove = match mode {
                    Collinear::Exclude => c <= 0,
                    Collinear::Include => c < 0,
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
                let c = cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, item.0);
                let remove = match mode {
                    Collinear::Exclude => c <= 0,
                    Collinear::Include => c < 0,
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

    let vertices: Vec<Point> = selected.iter().map(|&(p, _)| p).collect();
    let source_indices: Vec<usize> = selected.iter().map(|&(_, i)| i).collect();

    let (area2, perimeter_milli) = if vertices.len() >= 3 {
        let signed = shoelace(&vertices);
        let perimeter: u64 = (0..vertices.len())
            .map(|i| edge_mill(vertices[i], vertices[(i + 1) % vertices.len()]))
            .sum();
        (signed.unsigned_abs(), perimeter)
    } else if vertices.len() == 2 {
        (0, 2 * edge_mill(vertices[0], vertices[1]))
    } else {
        (0, 0)
    };

    Ok(Hull {
        vertices,
        source_indices,
        area2,
        perimeter_milli,
    })
}

fn main() {}