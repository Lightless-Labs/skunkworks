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

const MAX_VERTICES: usize = 4096;
const COORD_LIMIT: i64 = 1_000_000_000;

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

fn cross(a: Point, b: Point, q: Point) -> i128 {
    (b.x as i128 - a.x as i128) * (q.y as i128 - a.y as i128)
        - (b.y as i128 - a.y as i128) * (q.x as i128 - a.x as i128)
}

fn squared_distance(a: Point, b: Point) -> u128 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    (dx * dx + dy * dy) as u128
}

fn rounded_sqrt_million(d2: u128) -> u64 {
    let value = d2 * 1_000_000;
    let mut lo = 0u128;
    let mut hi = 3_000_000_000u128;
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if mid * mid <= value {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let lower_square = lo * lo;
    let next = lo + 1;
    let upper_square = next * next;
    if value - lower_square >= upper_square - value {
        (lo + 1) as u64
    } else {
        lo as u64
    }
}

fn edge_mill(a: Point, b: Point) -> u64 {
    rounded_sqrt_million(squared_distance(a, b))
}

fn shoelace(points: &[Point]) -> i128 {
    if points.is_empty() {
        return 0;
    }
    let mut sum = 0i128;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }
    sum
}

fn perimeter(points: &[Point]) -> (Vec<u64>, u64) {
    if points.is_empty() {
        return (Vec::new(), 0);
    }
    let mut edges = Vec::with_capacity(points.len());
    let mut total = 0u64;
    for i in 0..points.len() {
        let length = edge_mill(points[i], points[(i + 1) % points.len()]);
        edges.push(length);
        total += length;
    }
    (edges, total)
}

fn measures_for(points: &[Point]) -> Measures {
    let signed_area2 = shoelace(points);
    let (edge_milli, perimeter_milli) = perimeter(points);

    let longest_edge = if edge_milli.is_empty() {
        None
    } else {
        let mut best = 0usize;
        for i in 1..edge_milli.len() {
            let pair_i = {
                let a = points[i];
                let b = points[(i + 1) % points.len()];
                if a <= b { (a, b) } else { (b, a) }
            };
            let pair_best = {
                let a = points[best];
                let b = points[(best + 1) % points.len()];
                if a <= b { (a, b) } else { (b, a) }
            };
            if edge_milli[i] > edge_milli[best]
                || (edge_milli[i] == edge_milli[best]
                    && (pair_i < pair_best || (pair_i == pair_best && i < best)))
            {
                best = i;
            }
        }
        Some(best)
    };

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
    let prepared = prepare_polygon(points)?;
    Ok(measures_for(&prepared))
}

fn on_segment(a: Point, b: Point, q: Point) -> bool {
    cross(a, b, q) == 0
        && q.x >= a.x.min(b.x)
        && q.x <= a.x.max(b.x)
        && q.y >= a.y.min(b.y)
        && q.y <= a.y.max(b.y)
}

fn classify_one(points: &[Point], q: Point) -> Location {
    let n = points.len();
    for i in 0..n {
        if on_segment(points[i], points[(i + 1) % n], q) {
            return Location::Boundary { edge: i };
        }
    }

    let mut inside = false;
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        if (a.y > q.y) != (b.y > q.y) {
            let c = cross(a, b, q);
            if (b.y > a.y && c > 0) || (b.y < a.y && c < 0) {
                inside = !inside;
            }
        }
    }
    if inside {
        Location::Inside
    } else {
        Location::Outside
    }
}

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let prepared = prepare_polygon(points)?;
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
        .map(|&query| classify_one(&prepared, query))
        .collect())
}

fn point_cross(a: Point, b: Point, c: Point) -> i128 {
    (b.x as i128 - a.x as i128) * (c.y as i128 - a.y as i128)
        - (b.y as i128 - a.y as i128) * (c.x as i128 - a.x as i128)
}

fn between_on_segment(a: Point, b: Point, p: Point) -> bool {
    point_cross(a, b, p) == 0
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
        if let Some(existing) = unique.iter_mut().find(|entry| entry.0 == point) {
            existing.1 = existing.1.min(index);
        } else {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|entry| entry.0);

    if unique.len() <= 1 {
        let vertices: Vec<Point> = unique.iter().map(|entry| entry.0).collect();
        let source_indices: Vec<usize> = unique.iter().map(|entry| entry.1).collect();
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
        .all(|entry| point_cross(unique[0].0, unique[1].0, entry.0) == 0);

    let hull_points: Vec<Point> = if all_collinear {
        match mode {
            Collinear::Include => unique.iter().map(|entry| entry.0).collect(),
            Collinear::Exclude => vec![unique[0].0, unique[unique.len() - 1].0],
        }
    } else {
        let mut lower: Vec<Point> = Vec::new();
        for entry in &unique {
            while lower.len() >= 2 {
                let turn = point_cross(lower[lower.len() - 2], lower[lower.len() - 1], entry.0);
                if if mode == Collinear::Include { turn < 0 } else { turn <= 0 } {
                    lower.pop();
                } else {
                    break;
                }
            }
            lower.push(entry.0);
        }

        let mut upper: Vec<Point> = Vec::new();
        for entry in unique.iter().rev() {
            while upper.len() >= 2 {
                let turn = point_cross(upper[upper.len() - 2], upper[upper.len() - 1], entry.0);
                if if mode == Collinear::Include { turn < 0 } else { turn <= 0 } {
                    upper.pop();
                } else {
                    break;
                }
            }
            upper.push(entry.0);
        }

        lower.pop();
        upper.pop();
        lower.extend(upper);
        lower
    };

    let source_indices: Vec<usize> = hull_points
        .iter()
        .map(|point| unique.iter().find(|entry| entry.0 == *point).unwrap().1)
        .collect();
    let signed = shoelace(&hull_points);
    let (_, perimeter_milli) = perimeter(&hull_points);

    Ok(Hull {
        vertices: hull_points,
        source_indices,
        area2: signed.unsigned_abs(),
        perimeter_milli,
    })
}