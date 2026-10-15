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
const INPUT_LIMIT: usize = 4096;

fn in_range(p: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&p.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&p.y)
}

fn prepare_polygon(points: &[Point]) -> Result<&[Point], GeomError> {
    if points.len() > INPUT_LIMIT {
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

fn squared_distance(a: Point, b: Point) -> u128 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    (dx * dx + dy * dy) as u128
}

fn rounded_sqrt(n: u128) -> u128 {
    let mut lo = 0u128;
    let mut hi = 1u128 << 64;
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if mid <= n / mid {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    let lower_square = lo * lo;
    let remainder = n - lower_square;
    if remainder >= lo + 1 {
        lo + 1
    } else {
        lo
    }
}

fn edge_mill(a: Point, b: Point) -> u64 {
    rounded_sqrt(squared_distance(a, b) * 1_000_000) as u64
}

fn shoelace(points: &[Point]) -> i128 {
    let mut sum = 0i128;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }
    sum
}

fn endpoint_key(a: Point, b: Point) -> (Point, Point) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare_polygon(points)?;
    let signed_area2 = shoelace(polygon);
    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
    };

    let mut edge_milli = Vec::with_capacity(polygon.len());
    let mut perimeter_milli = 0u64;
    for i in 0..polygon.len() {
        let length = edge_mill(polygon[i], polygon[(i + 1) % polygon.len()]);
        edge_milli.push(length);
        perimeter_milli += length;
    }

    let mut longest_edge = None;
    for i in 0..polygon.len() {
        let replace = match longest_edge {
            None => true,
            Some(best) => {
                edge_milli[i] > edge_milli[best]
                    || (edge_milli[i] == edge_milli[best]
                        && (endpoint_key(polygon[i], polygon[(i + 1) % polygon.len()]),
                            i)
                            < (endpoint_key(
                                polygon[best],
                                polygon[(best + 1) % polygon.len()],
                            ), best))
            }
        };
        if replace {
            longest_edge = Some(i);
        }
    }

    Ok(Measures {
        signed_area2,
        area2: signed_area2.unsigned_abs(),
        orientation,
        edge_milli,
        perimeter_milli,
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

fn classify_one(polygon: &[Point], q: Point) -> Location {
    for i in 0..polygon.len() {
        if on_segment(polygon[i], polygon[(i + 1) % polygon.len()], q) {
            return Location::Boundary { edge: i };
        }
    }

    let mut odd = false;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        if (a.y > q.y) != (b.y > q.y) {
            let side = cross(a, b, q);
            if (b.y > a.y && side > 0) || (b.y < a.y && side < 0) {
                odd = !odd;
            }
        }
    }

    if odd {
        Location::Inside
    } else {
        Location::Outside
    }
}

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let polygon = prepare_polygon(points)?;
    if queries.len() > INPUT_LIMIT {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (index, &query) in queries.iter().enumerate() {
        if !in_range(query) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }
    Ok(queries
        .iter()
        .copied()
        .map(|query| classify_one(polygon, query))
        .collect())
}

fn hull_cross(a: Point, b: Point, c: Point) -> i128 {
    cross(a, b, c)
}

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    if points.len() > INPUT_LIMIT {
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

    let all_collinear = unique.len() <= 2
        || unique[2..]
            .iter()
            .all(|entry| hull_cross(unique[0].0, unique[1].0, entry.0) == 0);

    let hull_points: Vec<Point> = if all_collinear {
        match mode {
            Collinear::Include => unique.iter().map(|entry| entry.0).collect(),
            Collinear::Exclude => vec![unique[0].0, unique[unique.len() - 1].0],
        }
    } else {
        let keep_collinear = mode == Collinear::Include;
        let mut lower: Vec<Point> = Vec::new();
        for entry in &unique {
            while lower.len() >= 2 {
                let turn = hull_cross(lower[lower.len() - 2], lower[lower.len() - 1], entry.0);
                if if keep_collinear { turn < 0 } else { turn <= 0 } {
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
                let turn = hull_cross(upper[upper.len() - 2], upper[upper.len() - 1], entry.0);
                if if keep_collinear { turn < 0 } else { turn <= 0 } {
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
        .map(|point| {
            unique
                .binary_search_by_key(point, |entry| entry.0)
                .map(|index| unique[index].1)
                .unwrap()
        })
        .collect();

    let area2 = if hull_points.len() >= 3 {
        shoelace(&hull_points).unsigned_abs()
    } else {
        0
    };

    let mut perimeter_milli = 0u64;
    if hull_points.len() >= 2 {
        for i in 0..hull_points.len() {
            perimeter_milli += edge_mill(
                hull_points[i],
                hull_points[(i + 1) % hull_points.len()],
            );
        }
    }

    Ok(Hull {
        vertices: hull_points,
        source_indices,
        area2,
        perimeter_milli,
    })
}

fn main() {}