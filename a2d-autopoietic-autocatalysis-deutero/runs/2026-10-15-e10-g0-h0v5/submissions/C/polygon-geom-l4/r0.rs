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

fn in_range(point: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&point.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&point.y)
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

fn signed_area2(points: &[Point]) -> i128 {
    let mut sum = 0i128;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }
    sum
}

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    let squared = (dx * dx + dy * dy) as u128;
    let scaled = squared * 1_000_000u128;

    // Find floor(sqrt(scaled)) with integer binary search.
    let mut low = 0u128;
    let mut high = 3_000_000_000_000_000u128;
    while low < high {
        let mid = low + (high - low + 1) / 2;
        if mid * mid <= scaled {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    // Compare the fractional part to one half without floating point.
    let floor = low;
    if 4 * scaled >= (2 * floor + 1) * (2 * floor + 1) {
        (floor + 1) as u64
    } else {
        floor as u64
    }
}

fn endpoint_pair(a: Point, b: Point) -> (Point, Point) {
    if a <= b { (a, b) } else { (b, a) }
}

fn measures_for(points: &[Point]) -> Measures {
    let signed = signed_area2(points);
    let mut edge_milli = Vec::with_capacity(points.len());
    let mut perimeter_milli = 0u64;
    let mut longest_edge: Option<usize> = None;

    for i in 0..points.len() {
        let next = (i + 1) % points.len();
        let length = rounded_edge_milli(points[i], points[next]);
        edge_milli.push(length);
        perimeter_milli += length;

        let replace = match longest_edge {
            None => true,
            Some(best) => {
                let best_next = (best + 1) % points.len();
                length > edge_milli[best]
                    || (length == edge_milli[best]
                        && (endpoint_pair(points[i], points[next])
                            < endpoint_pair(points[best], points[best_next])
                            || (endpoint_pair(points[i], points[next])
                                == endpoint_pair(points[best], points[best_next])
                                && i < best)))
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

fn dot_at(prev: Point, current: Point, next: Point) -> i128 {
    (prev.x as i128 - current.x as i128) * (next.x as i128 - current.x as i128)
        + (prev.y as i128 - current.y as i128) * (next.y as i128 - current.y as i128)
}

pub fn validate(points: &[Point]) -> Result<Validated, GeomError> {
    let effective = prepare(points)?;
    let n = effective.len();

    // Iterate by first index, then second, for the required lexicographic pair.
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

    let signed = signed_area2(&effective);
    if signed == 0 {
        return Err(GeomError::ZeroArea);
    }

    for i in 0..n {
        let prev = effective[(i + n - 1) % n];
        let current = effective[i];
        let next = effective[(i + 1) % n];
        if cross(prev, current, next) == 0 && dot_at(prev, current, next) > 0 {
            return Err(GeomError::AdjacentOverlap { vertex: i });
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

    let start = (0..n).min_by_key(|&i| effective[i]).unwrap();
    let mut vertices = Vec::with_capacity(n);
    let mut source_indices = Vec::with_capacity(n);
    for offset in 0..n {
        let index = if signed > 0 {
            (start + offset) % n
        } else {
            (start + n - offset % n) % n
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

    let mut hull_indices: Vec<usize> = Vec::new();
    if unique.len() <= 1 {
        hull_indices.extend(0..unique.len());
    } else {
        let all_collinear = unique
            .iter()
            .skip(2)
            .all(|&(point, _)| cross(unique[0].0, unique[1].0, point) == 0);

        if all_collinear {
            hull_indices = if mode == Collinear::Include {
                (0..unique.len()).collect()
            } else {
                vec![0, unique.len() - 1]
            };
        } else {
            let should_pop = |turn: i128| match mode {
                Collinear::Exclude => turn <= 0,
                Collinear::Include => turn < 0,
            };

            let mut lower: Vec<usize> = Vec::new();
            for i in 0..unique.len() {
                while lower.len() >= 2 {
                    let a = unique[lower[lower.len() - 2]].0;
                    let b = unique[lower[lower.len() - 1]].0;
                    if should_pop(cross(a, b, unique[i].0)) {
                        lower.pop();
                    } else {
                        break;
                    }
                }
                lower.push(i);
            }

            let mut upper: Vec<usize> = Vec::new();
            for i in (0..unique.len()).rev() {
                while upper.len() >= 2 {
                    let a = unique[upper[upper.len() - 2]].0;
                    let b = unique[upper[upper.len() - 1]].0;
                    if should_pop(cross(a, b, unique[i].0)) {
                        upper.pop();
                    } else {
                        break;
                    }
                }
                upper.push(i);
            }

            lower.pop();
            upper.pop();
            lower.extend(upper);
            hull_indices = lower;
        }
    }

    let vertices: Vec<Point> = hull_indices.iter().map(|&i| unique[i].0).collect();
    let source_indices: Vec<usize> = hull_indices.iter().map(|&i| unique[i].1).collect();
    let area2 = if vertices.len() >= 3 {
        signed_area2(&vertices).unsigned_abs()
    } else {
        0
    };
    let perimeter_milli = if vertices.is_empty() {
        0
    } else {
        measures_for(&vertices).perimeter_milli
    };

    Ok(Hull {
        vertices,
        source_indices,
        area2,
        perimeter_milli,
    })
}

fn main() {}