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

const MAX_COORD: i64 = 1_000_000_000;
const MAX_INPUT: usize = 4096;

fn valid_point(p: Point) -> bool {
    (-MAX_COORD..=MAX_COORD).contains(&p.x) && (-MAX_COORD..=MAX_COORD).contains(&p.y)
}

fn prepare_polygon(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > MAX_INPUT {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }

    for (index, &point) in points.iter().enumerate() {
        if !valid_point(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut effective = points.to_vec();
    if effective.len() >= 2 && effective.first() == effective.last() {
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

fn rounded_sqrt(n: u128) -> u64 {
    let mut low = 0u128;
    let mut high = 1u128 << 64;

    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if mid <= n / mid {
            low = mid;
        } else {
            high = mid;
        }
    }

    let threshold = low * low + low;
    if n > threshold {
        (low + 1) as u64
    } else {
        low as u64
    }
}

fn edge_mill(a: Point, b: Point) -> u64 {
    rounded_sqrt(squared_distance(a, b) * 1_000_000)
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

fn perimeter_milli(points: &[Point]) -> u64 {
    if points.len() < 2 {
        return 0;
    }

    (0..points.len())
        .map(|i| edge_mill(points[i], points[(i + 1) % points.len()]))
        .sum()
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
    let signed = signed_area2(&polygon);
    let mut edge_milli = Vec::with_capacity(polygon.len());
    let mut perimeter = 0u64;
    let mut longest_edge: Option<usize> = None;

    for i in 0..polygon.len() {
        let length = edge_mill(polygon[i], polygon[(i + 1) % polygon.len()]);
        edge_milli.push(length);
        perimeter += length;

        if let Some(current) = longest_edge {
            let pair = ordered_endpoints(polygon[i], polygon[(i + 1) % polygon.len()]);
            let current_pair = ordered_endpoints(
                polygon[current],
                polygon[(current + 1) % polygon.len()],
            );
            if length > edge_milli[current]
                || (length == edge_milli[current]
                    && (pair < current_pair || (pair == current_pair && i < current)))
            {
                longest_edge = Some(i);
            }
        } else {
            longest_edge = Some(i);
        }
    }

    let orientation = if signed > 0 {
        Orientation::CounterClockwise
    } else if signed < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
    };

    Ok(Measures {
        signed_area2: signed,
        area2: signed.unsigned_abs(),
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

    if queries.len() > MAX_INPUT {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (index, &query) in queries.iter().enumerate() {
        if !valid_point(query) {
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
    if points.len() > MAX_INPUT {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !valid_point(point) {
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

    let hull: Vec<(Point, usize)> = if unique.len() <= 1 {
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
                    let turn = cross(
                        lower[lower.len() - 2].0,
                        lower[lower.len() - 1].0,
                        item.0,
                    );
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
                    let turn = cross(
                        upper[upper.len() - 2].0,
                        upper[upper.len() - 1].0,
                        item.0,
                    );
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

    let vertices: Vec<Point> = hull.iter().map(|&(point, _)| point).collect();
    let source_indices: Vec<usize> = hull.iter().map(|&(_, index)| index).collect();
    let area2 = if vertices.len() >= 3 {
        signed_area2(&vertices).unsigned_abs()
    } else {
        0
    };
    let perimeter_milli = perimeter_milli(&vertices);

    Ok(Hull {
        vertices,
        source_indices,
        area2,
        perimeter_milli,
    })
}