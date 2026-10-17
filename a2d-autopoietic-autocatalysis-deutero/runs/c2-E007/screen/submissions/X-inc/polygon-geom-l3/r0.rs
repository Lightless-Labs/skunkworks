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

const LIMIT: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

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

fn cross(a: Point, b: Point, q: Point) -> i128 {
    let abx = b.x as i128 - a.x as i128;
    let aby = b.y as i128 - a.y as i128;
    let aqx = q.x as i128 - a.x as i128;
    let aqy = q.y as i128 - a.y as i128;
    abx * aqy - aby * aqx
}

fn squared_distance(a: Point, b: Point) -> u128 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    (dx * dx + dy * dy) as u128
}

fn rounded_sqrt(n: u128) -> u128 {
    let mut lo = 0u128;
    let mut hi = 1u128;
    while hi * hi < n {
        hi <<= 1;
    }
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if mid * mid <= n {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let lower_error = n - lo * lo;
    let upper_error = hi * hi - n;
    if upper_error <= lower_error {
        hi
    } else {
        lo
    }
}

fn edge_mill(a: Point, b: Point) -> u64 {
    let scaled = squared_distance(a, b) * 1_000_000u128;
    rounded_sqrt(scaled) as u64
}

fn polygon_signed_area2(points: &[Point]) -> i128 {
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
        sum += edge_mill(points[i], points[(i + 1) % points.len()]);
    }
    sum
}

fn longest_edge_index(points: &[Point], lengths: &[u64]) -> Option<usize> {
    if points.is_empty() {
        return None;
    }
    let mut best = 0usize;
    for i in 1..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let best_a = points[best];
        let best_b = points[(best + 1) % points.len()];
        let pair = if a <= b { (a, b) } else { (b, a) };
        let best_pair = if best_a <= best_b {
            (best_a, best_b)
        } else {
            (best_b, best_a)
        };

        if lengths[i] > lengths[best]
            || (lengths[i] == lengths[best]
                && (pair < best_pair || (pair == best_pair && i < best)))
        {
            best = i;
        }
    }
    Some(best)
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let points = prepare_polygon(points)?;
    let signed_area2 = polygon_signed_area2(&points);
    let edge_milli: Vec<u64> = (0..points.len())
        .map(|i| edge_mill(points[i], points[(i + 1) % points.len()]))
        .collect();
    let perimeter_milli = edge_milli.iter().copied().sum();
    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
    };
    Ok(Measures {
        signed_area2,
        area2: signed_area2.unsigned_abs(),
        orientation,
        longest_edge: longest_edge_index(&points, &edge_milli),
        edge_milli,
        perimeter_milli,
    })
}

fn classify_one(points: &[Point], q: Point) -> Location {
    let mut owner = None;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        if cross(a, b, q) == 0
            && q.x >= a.x.min(b.x)
            && q.x <= a.x.max(b.x)
            && q.y >= a.y.min(b.y)
            && q.y <= a.y.max(b.y)
        {
            owner = Some(i);
            break;
        }
    }
    if let Some(edge) = owner {
        return Location::Boundary { edge };
    }

    let mut inside = false;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
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
    let polygon = prepare_polygon(points)?;
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
        .copied()
        .map(|query| classify_one(&polygon, query))
        .collect())
}

fn point_cross(a: Point, b: Point, c: Point) -> i128 {
    cross(a, b, c)
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

    let mut sorted: Vec<(Point, usize)> = Vec::with_capacity(points.len());
    for (index, &point) in points.iter().enumerate() {
        sorted.push((point, index));
    }
    sorted.sort_by_key(|&(point, index)| (point, index));
    let mut unique: Vec<(Point, usize)> = Vec::with_capacity(sorted.len());
    for item in sorted {
        if unique.last().map_or(true, |last| last.0 != item.0) {
            unique.push(item);
        }
    }

    let vertices_with_sources: Vec<(Point, usize)> = match unique.len() {
        0 | 1 => unique,
        2 => unique,
        _ => {
            let all_collinear = unique
                .iter()
                .skip(2)
                .all(|item| point_cross(unique[0].0, unique[1].0, item.0) == 0);

            if all_collinear {
                if mode == Collinear::Include {
                    unique
                } else {
                    vec![unique[0], *unique.last().unwrap()]
                }
            } else {
                let mut lower: Vec<(Point, usize)> = Vec::new();
                for &item in &unique {
                    while lower.len() >= 2 {
                        let turn = point_cross(
                            lower[lower.len() - 2].0,
                            lower[lower.len() - 1].0,
                            item.0,
                        );
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
                    lower.push(item);
                }

                let mut upper: Vec<(Point, usize)> = Vec::new();
                for &item in unique.iter().rev() {
                    while upper.len() >= 2 {
                        let turn = point_cross(
                            upper[upper.len() - 2].0,
                            upper[upper.len() - 1].0,
                            item.0,
                        );
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
                    upper.push(item);
                }

                lower.pop();
                upper.pop();
                lower.extend(upper);
                lower
            }
        }
    };

    let vertices: Vec<Point> = vertices_with_sources.iter().map(|item| item.0).collect();
    let source_indices: Vec<usize> = vertices_with_sources.iter().map(|item| item.1).collect();
    let signed_area2 = if vertices.len() >= 3 {
        polygon_signed_area2(&vertices)
    } else {
        0
    };

    Ok(Hull {
        area2: signed_area2.unsigned_abs(),
        perimeter_milli: perimeter_milli(&vertices),
        vertices,
        source_indices,
    })
}