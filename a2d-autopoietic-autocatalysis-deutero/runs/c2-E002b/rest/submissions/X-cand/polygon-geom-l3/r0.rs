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

fn prepare_polygon(points: &[Point]) -> Result<&[Point], GeomError> {
    if points.len() > RAW_LIMIT {
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

fn rounded_sqrt(n: u128) -> u128 {
    if n == 0 {
        return 0;
    }

    let mut low = 0u128;
    let mut high = 1u128;
    while high <= n / high {
        high *= 2;
    }

    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if mid <= n / mid {
            low = mid;
        } else {
            high = mid;
        }
    }

    // Compare n with (low + 1/2)^2 without fractions:
    // 4n >= (2low + 1)^2 rounds upward, including exact ties.
    let threshold = 2 * low + 1;
    if 4 * n >= threshold * threshold {
        low + 1
    } else {
        low
    }
}

fn edge_mill(a: Point, b: Point) -> u64 {
    rounded_sqrt(squared_distance(a, b) * 1_000_000) as u64
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

fn polygon_perimeter(points: &[Point]) -> (Vec<u64>, u64, Option<usize>) {
    let n = points.len();
    let mut lengths = Vec::with_capacity(n);
    let mut perimeter = 0u64;
    let mut longest: Option<usize> = None;

    for i in 0..n {
        let length = edge_mill(points[i], points[(i + 1) % n]);
        lengths.push(length);
        perimeter += length;

        let should_replace = match longest {
            None => true,
            Some(best) => {
                if length > lengths[best] {
                    true
                } else if length < lengths[best] {
                    false
                } else {
                    let mut edge = (points[i], points[(i + 1) % n]);
                    if edge.1 < edge.0 {
                        edge = (edge.1, edge.0);
                    }
                    let mut best_edge = (points[best], points[(best + 1) % n]);
                    if best_edge.1 < best_edge.0 {
                        best_edge = (best_edge.1, best_edge.0);
                    }
                    edge < best_edge || (edge == best_edge && i < best)
                }
            }
        };
        if should_replace {
            longest = Some(i);
        }
    }

    (lengths, perimeter, longest)
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare_polygon(points)?;
    let signed_area2 = polygon_signed_area2(polygon);
    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
    };
    let (edge_milli, perimeter_milli, longest_edge) = polygon_perimeter(polygon);

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
    let n = polygon.len();
    let mut boundary = None;
    let mut odd = false;

    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];

        if on_segment(a, b, q) {
            if boundary.is_none() {
                boundary = Some(i);
            }
            continue;
        }

        if (a.y > q.y) != (b.y > q.y) {
            let side = cross(a, b, q);
            if (b.y > a.y && side > 0) || (b.y < a.y && side < 0) {
                odd = !odd;
            }
        }
    }

    if let Some(edge) = boundary {
        Location::Boundary { edge }
    } else if odd {
        Location::Inside
    } else {
        Location::Outside
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

    Ok(queries
        .iter()
        .copied()
        .map(|query| classify_one(polygon, query))
        .collect())
}

fn all_collinear(points: &[Point]) -> bool {
    points.len() < 3 || points[2..].iter().all(|&p| cross(points[0], points[1], p) == 0)
}

fn hull_area2(points: &[Point]) -> u128 {
    if points.len() < 3 {
        return 0;
    }
    polygon_signed_area2(points).unsigned_abs()
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
        if !unique.iter().any(|&(existing, _)| existing == point) {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|&(point, _)| point);

    let distinct: Vec<Point> = unique.iter().map(|&(point, _)| point).collect();
    let (vertices, indices) = if distinct.len() <= 1 {
        (distinct, unique.iter().map(|&(_, index)| index).collect())
    } else if all_collinear(&distinct) {
        if mode == Collinear::Include {
            (
                distinct,
                unique.iter().map(|&(_, index)| index).collect(),
            )
        } else {
            let chosen = if distinct.len() == 2 {
                vec![0, 1]
            } else {
                vec![0, distinct.len() - 1]
            };
            (
                chosen.iter().map(|&i| distinct[i]).collect(),
                chosen.iter().map(|&i| unique[i].1).collect(),
            )
        }
    } else {
        let mut lower: Vec<usize> = Vec::new();
        for i in 0..distinct.len() {
            while lower.len() >= 2 {
                let n = lower.len();
                let turn = cross(distinct[lower[n - 2]], distinct[lower[n - 1]], distinct[i]);
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
            lower.push(i);
        }

        let mut upper: Vec<usize> = Vec::new();
        for i in (0..distinct.len()).rev() {
            while upper.len() >= 2 {
                let n = upper.len();
                let turn = cross(distinct[upper[n - 2]], distinct[upper[n - 1]], distinct[i]);
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
            upper.push(i);
        }

        lower.pop();
        upper.pop();
        lower.extend(upper);

        // The monotone chain is already counterclockwise and begins at the
        // lexicographically smallest point.
        let vertices: Vec<Point> = lower.iter().map(|&i| distinct[i]).collect();
        let indices: Vec<usize> = lower.iter().map(|&i| unique[i].1).collect();
        (vertices, indices)
    };

    let area2 = hull_area2(&vertices);
    let (_, perimeter_milli, _) = polygon_perimeter(&vertices);

    Ok(Hull {
        vertices,
        source_indices: indices,
        area2,
        perimeter_milli,
    })
}