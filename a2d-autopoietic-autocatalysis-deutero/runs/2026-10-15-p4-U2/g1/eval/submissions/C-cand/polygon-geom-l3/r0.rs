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
const MAX_INPUT: usize = 4096;

fn in_range(p: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&p.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&p.y)
}

fn prepare_polygon(points: &[Point]) -> Result<&[Point], GeomError> {
    if points.len() > MAX_INPUT {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let effective_len = if points.len() >= 2 && points.last() == points.first() {
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

    // Compare n with the square of the midpoint between lo and lo + 1,
    // avoiding floating point and preserving exact half-up rounding.
    let midpoint_cmp = 4 * n;
    let threshold = (2 * lo + 1) * (2 * lo + 1);
    if midpoint_cmp >= threshold {
        lo + 1
    } else {
        lo
    }
}

fn edge_mill(a: Point, b: Point) -> u64 {
    rounded_sqrt(squared_distance(a, b) * 1_000_000) as u64
}

fn cycle_area2(points: &[Point]) -> i128 {
    let mut sum = 0i128;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }
    sum
}

fn edge_pair(a: Point, b: Point) -> (Point, Point) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn edge_lengths(points: &[Point]) -> Vec<u64> {
    (0..points.len())
        .map(|i| edge_mill(points[i], points[(i + 1) % points.len()]))
        .collect()
}

fn perimeter(lengths: &[u64]) -> u64 {
    lengths.iter().copied().sum()
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let points = prepare_polygon(points)?;
    let signed_area2 = cycle_area2(points);
    let edge_milli = edge_lengths(points);
    let perimeter_milli = perimeter(&edge_milli);

    let longest_edge = (0..points.len()).max_by(|&i, &j| {
        edge_milli[i]
            .cmp(&edge_milli[j])
            .then_with(|| edge_pair(points[j], points[(j + 1) % points.len()])
                .cmp(&edge_pair(points[i], points[(i + 1) % points.len()])))
            .then_with(|| j.cmp(&i))
    });

    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
    };

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

fn classify_one(points: &[Point], q: Point) -> Location {
    for i in 0..points.len() {
        if on_segment(points[i], points[(i + 1) % points.len()], q) {
            return Location::Boundary { edge: i };
        }
    }

    let mut inside = false;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        if (a.y > q.y) != (b.y > q.y) {
            let side = cross(a, b, q);
            if (b.y > a.y && side > 0) || (b.y < a.y && side < 0) {
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
    let points = prepare_polygon(points)?;
    if queries.len() > MAX_INPUT {
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
        .map(|query| classify_one(points, query))
        .collect())
}

fn cloud_turn(a: Point, b: Point, c: Point) -> i128 {
    cross(a, b, c)
}

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    if points.len() > MAX_INPUT {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (index, &point) in points.iter().enumerate() {
        if let Some(entry) = unique.iter_mut().find(|entry| entry.0 == point) {
            entry.1 = entry.1.min(index);
        } else {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|entry| entry.0);

    let (vertices, source_indices) = match unique.len() {
        0 => (Vec::new(), Vec::new()),
        1 => (vec![unique[0].0], vec![unique[0].1]),
        _ => {
            let all_collinear = unique[2..]
                .iter()
                .all(|entry| cloud_turn(unique[0].0, unique[1].0, entry.0) == 0);

            if all_collinear {
                let chosen: Vec<(Point, usize)> = match mode {
                    Collinear::Include => unique,
                    Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
                };
                (
                    chosen.iter().map(|entry| entry.0).collect(),
                    chosen.iter().map(|entry| entry.1).collect(),
                )
            } else {
                let mut lower: Vec<(Point, usize)> = Vec::new();
                for &entry in &unique {
                    while lower.len() >= 2 {
                        let n = lower.len();
                        let turn = cloud_turn(lower[n - 2].0, lower[n - 1].0, entry.0);
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
                        let n = upper.len();
                        let turn = cloud_turn(upper[n - 2].0, upper[n - 1].0, entry.0);
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
                (
                    lower.iter().map(|entry| entry.0).collect(),
                    lower.iter().map(|entry| entry.1).collect(),
                )
            }
        }
    };

    let signed_area2 = cycle_area2(&vertices);
    let edge_milli = if vertices.is_empty() {
        Vec::new()
    } else if vertices.len() == 1 {
        vec![0]
    } else {
        edge_lengths(&vertices)
    };

    Ok(Hull {
        vertices,
        source_indices,
        area2: signed_area2.unsigned_abs(),
        perimeter_milli: perimeter(&edge_milli),
    })
}

fn main() {}