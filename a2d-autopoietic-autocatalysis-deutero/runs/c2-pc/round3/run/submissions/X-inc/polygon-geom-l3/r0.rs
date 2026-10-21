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
const MAX_ITEMS: usize = 4096;

fn in_range(p: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&p.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&p.y)
}

fn prepare_polygon(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > MAX_ITEMS {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut effective = points.to_vec();
    if effective.len() >= 2 && effective[0] == *effective.last().unwrap() {
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
    (b.x as i128 - a.x as i128) * (q.y as i128 - a.y as i128)
        - (b.y as i128 - a.y as i128) * (q.x as i128 - a.x as i128)
}

fn cross_origin(a: Point, b: Point) -> i128 {
    a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128
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

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let scaled_sq = squared_distance(a, b) * 1_000_000;
    let root = integer_sqrt(scaled_sq);
    let lhs = scaled_sq * 4;
    let midpoint_sq = (root * 2 + 1) * (root * 2 + 1);
    let rounded = if lhs >= midpoint_sq { root + 1 } else { root };
    rounded as u64
}

fn integer_sqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut lo = 1u128;
    let mut hi = 1u128 << 64;
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if mid <= n / mid {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

fn area2_signed(points: &[Point]) -> i128 {
    let mut sum = 0i128;
    for i in 0..points.len() {
        sum += cross_origin(points[i], points[(i + 1) % points.len()]);
    }
    sum
}

fn perimeter_milli(points: &[Point]) -> u64 {
    if points.len() < 2 {
        return 0;
    }
    (0..points.len())
        .map(|i| rounded_edge_milli(points[i], points[(i + 1) % points.len()]))
        .sum()
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare_polygon(points)?;
    let signed_area2 = area2_signed(&polygon);
    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
    };

    let edge_milli: Vec<u64> = (0..polygon.len())
        .map(|i| rounded_edge_milli(polygon[i], polygon[(i + 1) % polygon.len()]))
        .collect();
    let perimeter_milli = edge_milli.iter().copied().sum();

    let longest_edge = (0..polygon.len()).max_by(|&i, &j| {
        edge_milli[i]
            .cmp(&edge_milli[j])
            .then_with(|| {
                let pair_i = ordered_endpoints(polygon[i], polygon[(i + 1) % polygon.len()]);
                let pair_j = ordered_endpoints(polygon[j], polygon[(j + 1) % polygon.len()]);
                pair_j.cmp(&pair_i)
            })
            .then_with(|| j.cmp(&i))
    });

    Ok(Measures {
        signed_area2,
        area2: signed_area2.unsigned_abs(),
        orientation,
        edge_milli,
        perimeter_milli,
        longest_edge,
    })
}

fn ordered_endpoints(a: Point, b: Point) -> (Point, Point) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let polygon = prepare_polygon(points)?;
    if queries.len() > MAX_ITEMS {
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

fn classify_one(polygon: &[Point], query: Point) -> Location {
    for i in 0..polygon.len() {
        if on_segment(polygon[i], polygon[(i + 1) % polygon.len()], query) {
            return Location::Boundary { edge: i };
        }
    }

    let mut inside = false;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        if (a.y > query.y) != (b.y > query.y) {
            let c = cross(a, b, query);
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

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    if points.len() > MAX_ITEMS {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (index, &point) in points.iter().enumerate() {
        if !unique.iter().any(|(p, _)| *p == point) {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|entry| entry.0);

    let (vertices_with_sources, collinear) = if unique.len() <= 2 {
        (unique.clone(), unique.len() == 2)
    } else {
        let all_collinear = unique
            .iter()
            .skip(1)
            .all(|entry| cross(unique[0].0, unique[unique.len() - 1].0, entry.0) == 0);

        if all_collinear {
            let selected = match mode {
                Collinear::Include => unique.clone(),
                Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
            };
            (selected, true)
        } else {
            (build_hull(&unique, mode), false)
        }
    };

    let vertices: Vec<Point> = vertices_with_sources.iter().map(|entry| entry.0).collect();
    let source_indices: Vec<usize> = vertices_with_sources.iter().map(|entry| entry.1).collect();
    let signed_area = if vertices.len() >= 3 {
        area2_signed(&vertices)
    } else {
        0
    };

    let _ = collinear;
    Ok(Hull {
        area2: signed_area.unsigned_abs(),
        perimeter_milli: perimeter_milli(&vertices),
        vertices,
        source_indices,
    })
}

fn build_hull(sorted: &[(Point, usize)], mode: Collinear) -> Vec<(Point, usize)> {
    let mut lower: Vec<(Point, usize)> = Vec::new();
    for &entry in sorted {
        while lower.len() >= 2 {
            let turn = cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, entry.0);
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
        lower.push(entry);
    }

    let mut upper: Vec<(Point, usize)> = Vec::new();
    for &entry in sorted.iter().rev() {
        while upper.len() >= 2 {
            let turn = cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, entry.0);
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
        upper.push(entry);
    }

    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}