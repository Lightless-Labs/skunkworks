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

fn in_range(point: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&point.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&point.y)
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

fn squared_distance(a: Point, b: Point) -> u128 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    (dx * dx + dy * dy) as u128
}

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let n = squared_distance(a, b) * 1_000_000;
    let mut lo = 0u128;
    let mut hi = 3_000_000_000u128;

    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if mid * mid <= n {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }

    if n - lo * lo >= lo * 2 + 1 {
        (lo + 1) as u64
    } else {
        lo as u64
    }
}

fn area2_signed(points: &[Point]) -> i128 {
    let mut sum = 0i128;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }
    sum
}

fn perimeter_milli(points: &[Point]) -> u64 {
    if points.is_empty() {
        return 0;
    }
    let mut sum = 0u64;
    for i in 0..points.len() {
        sum += rounded_edge_milli(points[i], points[(i + 1) % points.len()]);
    }
    sum
}

fn ordered_endpoints(a: Point, b: Point) -> (Point, Point) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare_polygon(points)?;
    let signed_area2 = area2_signed(&polygon);
    let orientation = if signed_area2 > 0 {
        Orientation::CounterClockwise
    } else if signed_area2 < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
    };

    let mut edge_milli = Vec::with_capacity(polygon.len());
    let mut perimeter = 0u64;
    for i in 0..polygon.len() {
        let length = rounded_edge_milli(polygon[i], polygon[(i + 1) % polygon.len()]);
        edge_milli.push(length);
        perimeter += length;
    }

    let longest_edge = (0..polygon.len()).max_by(|&i, &j| {
        edge_milli[i]
            .cmp(&edge_milli[j])
            .then_with(|| {
                let i_pair = ordered_endpoints(polygon[i], polygon[(i + 1) % polygon.len()]);
                let j_pair = ordered_endpoints(polygon[j], polygon[(j + 1) % polygon.len()]);
                j_pair.cmp(&i_pair)
            })
            .then_with(|| j.cmp(&i))
    });

    Ok(Measures {
        signed_area2,
        area2: signed_area2.unsigned_abs(),
        orientation,
        edge_milli,
        perimeter_milli: perimeter,
        longest_edge,
    })
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

    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (index, &point) in points.iter().enumerate() {
        if !unique.iter().any(|(existing, _)| *existing == point) {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|&(point, _)| point);

    let mut hull: Vec<(Point, usize)> = if unique.len() <= 1 {
        unique.clone()
    } else {
        let all_collinear = unique
            .iter()
            .skip(2)
            .all(|&(point, _)| cross(unique[0].0, unique[1].0, point) == 0);

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
                    let turn = cross(lower[n - 2].0, lower[n - 1].0, item.0);
                    let should_pop = match mode {
                        Collinear::Exclude => turn <= 0,
                        Collinear::Include => turn < 0,
                    };
                    if should_pop {
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
                    let turn = cross(upper[n - 2].0, upper[n - 1].0, item.0);
                    let should_pop = match mode {
                        Collinear::Exclude => turn <= 0,
                        Collinear::Include => turn < 0,
                    };
                    if should_pop {
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

    let vertices_for_area: Vec<Point> = hull.iter().map(|&(point, _)| point).collect();
    if vertices_for_area.len() >= 3 && area2_signed(&vertices_for_area) < 0 {
        hull.reverse();
    }

    let vertices: Vec<Point> = hull.iter().map(|&(point, _)| point).collect();
    let source_indices: Vec<usize> = hull.iter().map(|&(_, index)| index).collect();
    let signed_area = if vertices.len() >= 3 {
        area2_signed(&vertices)
    } else {
        0
    };

    Ok(Hull {
        area2: signed_area.unsigned_abs(),
        perimeter_milli: perimeter_milli(&vertices),
        vertices,
        source_indices,
    })
}

fn main() {}