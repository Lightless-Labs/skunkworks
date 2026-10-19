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

fn cross(a: Point, b: Point, q: Point) -> i128 {
    let abx = b.x as i128 - a.x as i128;
    let aby = b.y as i128 - a.y as i128;
    let aqx = q.x as i128 - a.x as i128;
    let aqy = q.y as i128 - a.y as i128;
    abx * aqy - aby * aqx
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
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    let scaled_squared = (dx * dx + dy * dy) as u128 * 1_000_000;
    let root = isqrt(scaled_squared);
    let rounded = if scaled_squared - root * root >= root + 1 {
        root + 1
    } else {
        root
    };
    rounded as u64
}

fn measures_for(points: &[Point]) -> Measures {
    let n = points.len();
    let signed_area2 = shoelace(points);
    let area2 = signed_area2.unsigned_abs();
    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
    };

    let mut edge_milli = Vec::with_capacity(n);
    let mut perimeter_milli = 0u64;
    let mut longest_edge = None;

    for i in 0..n {
        let length = edge_length_milli(points[i], points[(i + 1) % n]);
        edge_milli.push(length);
        perimeter_milli += length;

        let replace = match longest_edge {
            None => true,
            Some(best) if length > edge_milli[best] => true,
            Some(best) if length < edge_milli[best] => false,
            Some(best) => {
                let endpoints = ordered_endpoints(points[i], points[(i + 1) % n]);
                let best_endpoints =
                    ordered_endpoints(points[best], points[(best + 1) % n]);
                endpoints < best_endpoints || (endpoints == best_endpoints && i < best)
            }
        };
        if replace {
            longest_edge = Some(i);
        }
    }

    Measures {
        signed_area2,
        area2,
        orientation,
        edge_milli,
        perimeter_milli,
        longest_edge,
    }
}

fn ordered_endpoints(a: Point, b: Point) -> (Point, Point) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
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

    if queries.len() > MAX_VERTICES {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (index, &query) in queries.iter().enumerate() {
        if !in_range(query) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    let n = polygon.len();
    let mut results = Vec::with_capacity(queries.len());

    for &query in queries {
        let mut boundary_edge = None;
        for i in 0..n {
            if on_segment(polygon[i], polygon[(i + 1) % n], query) {
                boundary_edge = Some(i);
                break;
            }
        }

        if let Some(edge) = boundary_edge {
            results.push(Location::Boundary { edge });
            continue;
        }

        let mut inside = false;
        for i in 0..n {
            let a = polygon[i];
            let b = polygon[(i + 1) % n];
            if (a.y > query.y) != (b.y > query.y) {
                let c = cross(a, b, query);
                if (a.y < b.y && c > 0) || (a.y > b.y && c < 0) {
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

fn prepare_cloud(points: &[Point]) -> Result<Vec<(Point, usize)>, GeomError> {
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
        if !unique.iter().any(|(existing, _)| *existing == point) {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|(point, _)| *point);
    Ok(unique)
}

fn all_collinear(points: &[(Point, usize)]) -> bool {
    if points.len() < 3 {
        return true;
    }
    let a = points[0].0;
    let b = points[1].0;
    points.iter().skip(2).all(|(p, _)| cross(a, b, *p) == 0)
}

fn monotone_hull(points: &[(Point, usize)], include_collinear: bool) -> Vec<(Point, usize)> {
    if points.len() <= 1 {
        return points.to_vec();
    }

    let should_pop = |a: Point, b: Point, c: Point| {
        let turn = cross(a, b, c);
        if include_collinear {
            turn < 0
        } else {
            turn <= 0
        }
    };

    let mut lower = Vec::new();
    for &item in points {
        while lower.len() >= 2
            && should_pop(lower[lower.len() - 2].0, lower[lower.len() - 1].0, item.0)
        {
            lower.pop();
        }
        lower.push(item);
    }

    let mut upper = Vec::new();
    for &item in points.iter().rev() {
        while upper.len() >= 2
            && should_pop(upper[upper.len() - 2].0, upper[upper.len() - 1].0, item.0)
        {
            upper.pop();
        }
        upper.push(item);
    }

    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    let unique = prepare_cloud(points)?;
    let include = mode == Collinear::Include;

    let hull = if unique.is_empty() {
        Vec::new()
    } else if all_collinear(&unique) {
        match mode {
            Collinear::Include => unique.clone(),
            Collinear::Exclude if unique.len() == 1 => vec![unique[0]],
            Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
        }
    } else {
        monotone_hull(&unique, include)
    };

    let vertices: Vec<Point> = hull.iter().map(|(point, _)| *point).collect();
    let source_indices: Vec<usize> = hull.iter().map(|(_, index)| *index).collect();

    let area2 = if vertices.len() >= 3 {
        shoelace(&vertices).unsigned_abs()
    } else {
        0
    };
    let perimeter_milli = match vertices.len() {
        0 | 1 => 0,
        n => (0..n)
            .map(|i| edge_length_milli(vertices[i], vertices[(i + 1) % n]))
            .sum(),
    };

    Ok(Hull {
        vertices,
        source_indices,
        area2,
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
    let effective = prepare_polygon(points)?;
    let n = effective.len();

    let mut duplicate = None;
    for i in 0..n {
        for j in (i + 1)..n {
            if effective[i] == effective[j] {
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

    let min_index = (0..n).min_by_key(|&i| effective[i]).unwrap();
    let mut indices = Vec::with_capacity(n);
    if area > 0 {
        for offset in 0..n {
            indices.push((min_index + offset) % n);
        }
    } else {
        for offset in 0..n {
            indices.push((min_index + n - offset) % n);
        }
    }

    let vertices: Vec<Point> = indices.iter().map(|&i| effective[i]).collect();
    let source_indices = indices;
    let measures = measures_for(&vertices);

    Ok(Validated {
        vertices,
        source_indices,
        measures,
    })
}