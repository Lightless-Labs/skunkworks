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
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut effective_len = points.len();
    if effective_len >= 2 && points[effective_len - 1] == points[0] {
        effective_len -= 1;
    }
    if effective_len < 3 {
        return Err(GeomError::TooFewVertices { len: effective_len });
    }
    Ok(points[..effective_len].to_vec())
}

fn cross(a: Point, b: Point, c: Point) -> i128 {
    let abx = b.x as i128 - a.x as i128;
    let aby = b.y as i128 - a.y as i128;
    let acx = c.x as i128 - a.x as i128;
    let acy = c.y as i128 - a.y as i128;
    abx * acy - aby * acx
}

fn on_segment(a: Point, b: Point, q: Point) -> bool {
    cross(a, b, q) == 0
        && q.x >= a.x.min(b.x)
        && q.x <= a.x.max(b.x)
        && q.y >= a.y.min(b.y)
        && q.y <= a.y.max(b.y)
}

fn squared_distance(a: Point, b: Point) -> u128 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    (dx * dx + dy * dy) as u128
}

fn rounded_sqrt(n: u128) -> u128 {
    let mut lo = 0u128;
    let mut hi = 1u128;
    while hi.saturating_mul(hi) < n {
        hi *= 2;
    }
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if mid * mid <= n {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let lower_square = lo * lo;
    let upper_square = hi * hi;
    if n - lower_square >= upper_square - n {
        hi
    } else {
        lo
    }
}

fn edge_length_milli(a: Point, b: Point) -> u64 {
    let scaled = squared_distance(a, b) * 1_000_000;
    rounded_sqrt(scaled) as u64
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

fn perimeter_and_edges(points: &[Point]) -> (Vec<u64>, u64) {
    let mut edges = Vec::with_capacity(points.len());
    let mut perimeter = 0u64;
    if points.is_empty() {
        return (edges, perimeter);
    }
    for i in 0..points.len() {
        let length = edge_length_milli(points[i], points[(i + 1) % points.len()]);
        edges.push(length);
        perimeter += length;
    }
    (edges, perimeter)
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare_polygon(points)?;
    let signed = signed_area2(&polygon);
    let (edge_milli, perimeter_milli) = perimeter_and_edges(&polygon);

    let orientation = if signed > 0 {
        Orientation::CounterClockwise
    } else if signed < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
    };

    let mut longest_edge = None;
    for i in 0..edge_milli.len() {
        let Some(current) = longest_edge else {
            longest_edge = Some(i);
            continue;
        };
        if edge_milli[i] > edge_milli[current] {
            longest_edge = Some(i);
        } else if edge_milli[i] == edge_milli[current] {
            let endpoints_i = ordered_pair(polygon[i], polygon[(i + 1) % polygon.len()]);
            let endpoints_current =
                ordered_pair(polygon[current], polygon[(current + 1) % polygon.len()]);
            if endpoints_i < endpoints_current
                || (endpoints_i == endpoints_current && i < current)
            {
                longest_edge = Some(i);
            }
        }
    }

    Ok(Measures {
        signed_area2: signed,
        area2: signed.unsigned_abs(),
        orientation,
        edge_milli,
        perimeter_milli,
        longest_edge,
    })
}

fn ordered_pair(a: Point, b: Point) -> (Point, Point) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let polygon = prepare_polygon(points)?;

    if queries.len() > RAW_LIMIT {
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

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    if points.len() > RAW_LIMIT {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut indexed = points
        .iter()
        .copied()
        .enumerate()
        .map(|(index, point)| (point, index))
        .collect::<Vec<_>>();
    indexed.sort_by_key(|&(point, index)| (point, index));

    let mut unique = Vec::with_capacity(indexed.len());
    for entry in indexed {
        if unique.last().map(|last: &(Point, usize)| last.0) != Some(entry.0) {
            unique.push(entry);
        }
    }

    if unique.len() <= 1 {
        let vertices: Vec<Point> = unique.iter().map(|&(point, _)| point).collect();
        let source_indices: Vec<usize> = unique.iter().map(|&(_, index)| index).collect();
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
        .all(|&(point, _)| cross(unique[0].0, unique[1].0, point) == 0);

    let hull_entries = if all_collinear {
        match mode {
            Collinear::Include => unique,
            Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
        }
    } else {
        let mut lower: Vec<(Point, usize)> = Vec::new();
        for &entry in &unique {
            while lower.len() >= 2 {
                let turn = cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, entry.0);
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
            lower.push(entry);
        }

        let mut upper: Vec<(Point, usize)> = Vec::new();
        for &entry in unique.iter().rev() {
            while upper.len() >= 2 {
                let turn = cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, entry.0);
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
            upper.push(entry);
        }

        lower.pop();
        upper.pop();
        lower.extend(upper);
        lower
    };

    let vertices: Vec<Point> = hull_entries.iter().map(|&(point, _)| point).collect();
    let source_indices: Vec<usize> = hull_entries.iter().map(|&(_, index)| index).collect();
    let signed = signed_area2(&vertices);
    let (_, perimeter_milli) = perimeter_and_edges(&vertices);

    Ok(Hull {
        vertices,
        source_indices,
        area2: signed.unsigned_abs(),
        perimeter_milli,
    })
}