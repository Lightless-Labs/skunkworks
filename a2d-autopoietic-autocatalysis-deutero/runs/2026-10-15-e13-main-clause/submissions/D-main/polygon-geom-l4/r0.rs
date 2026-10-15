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
const MAX_QUERIES: usize = 4096;

fn in_range(p: Point) -> bool {
    (-LIMIT..=LIMIT).contains(&p.x) && (-LIMIT..=LIMIT).contains(&p.y)
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
    (b.x as i128 - a.x as i128) * (c.y as i128 - a.y as i128)
        - (b.y as i128 - a.y as i128) * (c.x as i128 - a.x as i128)
}

fn dot_from(vertex: Point, a: Point, b: Point) -> i128 {
    (a.x as i128 - vertex.x as i128) * (b.x as i128 - vertex.x as i128)
        + (a.y as i128 - vertex.y as i128) * (b.y as i128 - vertex.y as i128)
}

fn signed_area2(points: &[Point]) -> i128 {
    let mut area = 0i128;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        area += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }
    area
}

fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = x / 2 + x % 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let dx = a.x as i128 - b.x as i128;
    let dy = a.y as i128 - b.y as i128;
    let scaled_sq = (dx * dx + dy * dy) as u128 * 1_000_000;
    let root = isqrt(scaled_sq);
    let midpoint_sq = (2 * root + 1) * (2 * root + 1);
    (root + u128::from(4 * scaled_sq >= midpoint_sq)) as u64
}

fn ordered_pair(a: Point, b: Point) -> (Point, Point) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn measures_for(points: &[Point]) -> Measures {
    let area = signed_area2(points);
    let orientation = if area > 0 {
        Orientation::CounterClockwise
    } else if area < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
    };

    let mut edge_milli = Vec::with_capacity(points.len());
    let mut perimeter_milli = 0u64;
    for i in 0..points.len() {
        let length = rounded_edge_milli(points[i], points[(i + 1) % points.len()]);
        edge_milli.push(length);
        perimeter_milli += length;
    }

    let mut longest_edge = None;
    for i in 0..points.len() {
        let replace = match longest_edge {
            None => true,
            Some(best) if edge_milli[i] > edge_milli[best] => true,
            Some(best) if edge_milli[i] < edge_milli[best] => false,
            Some(best) => {
                let pair = ordered_pair(points[i], points[(i + 1) % points.len()]);
                let best_pair = ordered_pair(points[best], points[(best + 1) % points.len()]);
                pair < best_pair || (pair == best_pair && i < best)
            }
        };
        if replace {
            longest_edge = Some(i);
        }
    }

    Measures {
        signed_area2: area,
        area2: area.unsigned_abs(),
        orientation,
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

    if queries.len() > MAX_QUERIES {
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
    (o1 < 0) != (o2 < 0) && (o3 < 0) != (o4 < 0)
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
        if cross(prev, current, next) == 0 && dot_from(current, prev, next) > 0 {
            return Err(GeomError::AdjacentOverlap { vertex: i });
        }
    }

    for i in 0..n {
        for j in (i + 1)..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            if segment_intersects(
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
    let direction: isize = if area > 0 { 1 } else { -1 };
    let mut vertices = Vec::with_capacity(n);
    let mut source_indices = Vec::with_capacity(n);
    for step in 0..n {
        let index =
            (min_index as isize + direction * step as isize).rem_euclid(n as isize) as usize;
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
        if !unique.iter().any(|&(existing, _)| existing == point) {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|&(point, _)| point);

    let mut hull_indices: Vec<usize> = Vec::new();
    if unique.len() <= 1 {
        hull_indices.extend(0..unique.len());
    } else {
        let all_collinear = unique
            .windows(3)
            .all(|w| cross(w[0].0, w[1].0, w[2].0) == 0);

        if all_collinear {
            if mode == Collinear::Include {
                hull_indices.extend(0..unique.len());
            } else {
                hull_indices.push(0);
                hull_indices.push(unique.len() - 1);
            }
        } else {
            let mut lower: Vec<usize> = Vec::new();
            for i in 0..unique.len() {
                while lower.len() >= 2 {
                    let a = unique[lower[lower.len() - 2]].0;
                    let b = unique[lower[lower.len() - 1]].0;
                    let c = unique[i].0;
                    let turn = cross(a, b, c);
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
                lower.push(i);
            }

            let mut upper: Vec<usize> = Vec::new();
            for i in (0..unique.len()).rev() {
                while upper.len() >= 2 {
                    let a = unique[upper[upper.len() - 2]].0;
                    let b = unique[upper[upper.len() - 1]].0;
                    let c = unique[i].0;
                    let turn = cross(a, b, c);
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
                upper.push(i);
            }
            lower.pop();
            upper.pop();
            hull_indices.extend(lower);
            hull_indices.extend(upper);
        }
    }

    let vertices: Vec<Point> = hull_indices.iter().map(|&i| unique[i].0).collect();
    let source_indices: Vec<usize> = hull_indices.iter().map(|&i| unique[i].1).collect();
    let area2 = if vertices.len() >= 3 {
        signed_area2(&vertices).unsigned_abs()
    } else {
        0
    };

    let mut perimeter_milli = 0u64;
    if vertices.len() >= 2 {
        for i in 0..vertices.len() {
            perimeter_milli += rounded_edge_milli(vertices[i], vertices[(i + 1) % vertices.len()]);
        }
    }

    Ok(Hull {
        vertices,
        source_indices,
        area2,
        perimeter_milli,
    })
}

fn main() {}