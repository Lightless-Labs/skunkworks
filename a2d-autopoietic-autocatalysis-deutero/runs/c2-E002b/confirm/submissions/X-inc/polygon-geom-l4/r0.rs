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

fn prepare_polygon(points: &[Point]) -> Result<&[Point], GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
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
    Ok(&points[..effective_len])
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
    let mut hi = n.min(u64::MAX as u128) + 1;
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if mid <= n / mid {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let lower_sq = lo * lo;
    let upper = lo + 1;
    let upper_sq = upper * upper;
    if n - lower_sq >= upper_sq - n {
        upper
    } else {
        lo
    }
}

fn edge_length_milli(a: Point, b: Point) -> u64 {
    let dx = (a.x as i128 - b.x as i128).unsigned_abs();
    let dy = (a.y as i128 - b.y as i128).unsigned_abs();
    let squared_scaled = (dx * dx + dy * dy) * 1_000_000u128;
    rounded_sqrt(squared_scaled) as u64
}

fn edge_key(a: Point, b: Point) -> (Point, Point) {
    if a <= b { (a, b) } else { (b, a) }
}

fn measures_for(points: &[Point]) -> Measures {
    let signed_area2 = shoelace(points);
    let n = points.len();
    let mut edge_milli = Vec::with_capacity(n);
    let mut perimeter_milli = 0u64;
    let mut longest_edge: Option<usize> = None;

    for i in 0..n {
        let len = edge_length_milli(points[i], points[(i + 1) % n]);
        edge_milli.push(len);
        perimeter_milli += len;

        match longest_edge {
            None => longest_edge = Some(i),
            Some(best) => {
                if len > edge_milli[best]
                    || (len == edge_milli[best]
                        && (edge_key(points[i], points[(i + 1) % n])
                            < edge_key(points[best], points[(best + 1) % n])
                            || (edge_key(points[i], points[(i + 1) % n])
                                == edge_key(points[best], points[(best + 1) % n])
                                && i < best)))
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
    Ok(measures_for(effective))
}

fn point_on_segment(p: Point, a: Point, b: Point) -> bool {
    cross(a, b, p) == 0
        && p.x >= a.x.min(b.x)
        && p.x <= a.x.max(b.x)
        && p.y >= a.y.min(b.y)
        && p.y <= a.y.max(b.y)
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
            if point_on_segment(q, polygon[i], polygon[(i + 1) % n]) {
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

fn between_collinear(a: Point, b: Point, p: Point) -> bool {
    cross(a, b, p) == 0
        && p.x >= a.x.min(b.x)
        && p.x <= a.x.max(b.x)
        && p.y >= a.y.min(b.y)
        && p.y <= a.y.max(b.y)
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
    unique.sort_by_key(|&(p, _)| p);

    let mut output: Vec<(Point, usize)> = Vec::new();
    if unique.len() <= 1 {
        output = unique;
    } else {
        let all_collinear = unique
            .iter()
            .skip(2)
            .all(|&(p, _)| cross(unique[0].0, unique[1].0, p) == 0);

        if all_collinear {
            output = match mode {
                Collinear::Include => unique,
                Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
            };
        } else {
            let should_pop = |a: Point, b: Point, c: Point| match mode {
                Collinear::Exclude => cross(a, b, c) <= 0,
                Collinear::Include => cross(a, b, c) < 0,
            };

            let mut lower: Vec<(Point, usize)> = Vec::new();
            for &item in &unique {
                while lower.len() >= 2
                    && should_pop(lower[lower.len() - 2].0, lower[lower.len() - 1].0, item.0)
                {
                    lower.pop();
                }
                lower.push(item);
            }

            let mut upper: Vec<(Point, usize)> = Vec::new();
            for &item in unique.iter().rev() {
                while upper.len() >= 2
                    && should_pop(upper[upper.len() - 2].0, upper[upper.len() - 1].0, item.0)
                {
                    upper.pop();
                }
                upper.push(item);
            }

            lower.pop();
            upper.pop();
            output.extend(lower);
            output.extend(upper);
        }
    }

    let vertices: Vec<Point> = output.iter().map(|&(p, _)| p).collect();
    let source_indices: Vec<usize> = output.iter().map(|&(_, i)| i).collect();
    let signed_area2 = if vertices.len() >= 3 {
        shoelace(&vertices)
    } else {
        0
    };

    let perimeter_milli = if vertices.is_empty() {
        0
    } else {
        let mut sum = 0u64;
        for i in 0..vertices.len() {
            sum += edge_length_milli(vertices[i], vertices[(i + 1) % vertices.len()]);
        }
        sum
    };

    let _ = between_collinear;
    Ok(Hull {
        vertices,
        source_indices,
        area2: signed_area2.unsigned_abs(),
        perimeter_milli,
    })
}

fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let c1 = cross(a, b, c);
    let c2 = cross(a, b, d);
    let c3 = cross(c, d, a);
    let c4 = cross(c, d, b);

    if c1 == 0 && point_on_segment(c, a, b) {
        return true;
    }
    if c2 == 0 && point_on_segment(d, a, b) {
        return true;
    }
    if c3 == 0 && point_on_segment(a, c, d) {
        return true;
    }
    if c4 == 0 && point_on_segment(b, c, d) {
        return true;
    }

    (c1 > 0) != (c2 > 0) && (c3 > 0) != (c4 > 0)
}

pub fn validate(points: &[Point]) -> Result<Validated, GeomError> {
    let polygon = prepare_polygon(points)?;
    let n = polygon.len();

    for i in 0..n {
        for j in (i + 1)..n {
            if polygon[i] == polygon[j] {
                return Err(GeomError::Duplicate {
                    first: i,
                    second: j,
                });
            }
        }
    }

    let signed = shoelace(polygon);
    if signed == 0 {
        return Err(GeomError::ZeroArea);
    }

    for i in 0..n {
        let prev = polygon[(i + n - 1) % n];
        let curr = polygon[i];
        let next = polygon[(i + 1) % n];
        if cross(prev, curr, next) == 0 {
            let dot = (prev.x as i128 - curr.x as i128)
                * (next.x as i128 - curr.x as i128)
                + (prev.y as i128 - curr.y as i128)
                    * (next.y as i128 - curr.y as i128);
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

    let min_index = (0..n).min_by_key(|&i| polygon[i]).unwrap();
    let mut order = Vec::with_capacity(n);
    if signed > 0 {
        for offset in 0..n {
            order.push((min_index + offset) % n);
        }
    } else {
        for offset in 0..n {
            order.push((min_index + n - offset) % n);
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