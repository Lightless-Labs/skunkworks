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

fn cross(a: Point, b: Point, c: Point) -> i128 {
    let ab_x = b.x as i128 - a.x as i128;
    let ab_y = b.y as i128 - a.y as i128;
    let ac_x = c.x as i128 - a.x as i128;
    let ac_y = c.y as i128 - a.y as i128;
    ab_x * ac_y - ab_y * ac_x
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

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    let squared = (dx * dx + dy * dy) as u128 * 1_000_000u128;

    let mut root = integer_sqrt(squared);
    let remainder = squared - root * root;
    if remainder * 4 >= root * 4 + 1 {
        root += 1;
    }
    root as u64
}

fn integer_sqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

fn edge_lengths(points: &[Point]) -> (Vec<u64>, u64, Option<usize>) {
    let n = points.len();
    let mut lengths = Vec::with_capacity(n);
    let mut perimeter = 0u64;
    for i in 0..n {
        let length = rounded_edge_milli(points[i], points[(i + 1) % n]);
        lengths.push(length);
        perimeter += length;
    }

    let mut longest: Option<usize> = None;
    for i in 0..n {
        let Some(current) = longest else {
            longest = Some(i);
            continue;
        };
        if lengths[i] > lengths[current] {
            longest = Some(i);
        } else if lengths[i] == lengths[current] {
            let pair_i = ordered_endpoints(points[i], points[(i + 1) % n]);
            let pair_current =
                ordered_endpoints(points[current], points[(current + 1) % n]);
            if pair_i < pair_current || (pair_i == pair_current && i < current) {
                longest = Some(i);
            }
        }
    }
    (lengths, perimeter, longest)
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
    let signed_area2 = shoelace(&polygon);
    let area2 = signed_area2.unsigned_abs();
    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
    };
    let (edge_milli, perimeter_milli, longest_edge) = edge_lengths(&polygon);
    Ok(Measures {
        signed_area2,
        area2,
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
        if let Some(entry) = unique.iter_mut().find(|entry| entry.0 == point) {
            entry.1 = entry.1.min(index);
        } else {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|entry| entry.0);

    if unique.len() <= 1 {
        let vertices: Vec<Point> = unique.iter().map(|entry| entry.0).collect();
        let source_indices: Vec<usize> = unique.iter().map(|entry| entry.1).collect();
        let (_, perimeter_milli, _) = edge_lengths(&vertices);
        return Ok(Hull {
            vertices,
            source_indices,
            area2: 0,
            perimeter_milli,
        });
    }

    let all_collinear = unique
        .iter()
        .skip(2)
        .all(|entry| cross(unique[0].0, unique[1].0, entry.0) == 0);

    let hull_points: Vec<Point> = if all_collinear {
        match mode {
            Collinear::Include => unique.iter().map(|entry| entry.0).collect(),
            Collinear::Exclude => vec![unique[0].0, unique[unique.len() - 1].0],
        }
    } else {
        let mut lower: Vec<(Point, usize)> = Vec::new();
        for &entry in &unique {
            while lower.len() >= 2 {
                let n = lower.len();
                let turn = cross(lower[n - 2].0, lower[n - 1].0, entry.0);
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
        for &entry in unique.iter().rev() {
            while upper.len() >= 2 {
                let n = upper.len();
                let turn = cross(upper[n - 2].0, upper[n - 1].0, entry.0);
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
        lower.iter().map(|entry| entry.0).collect()
    };

    let vertices = hull_points;
    let source_indices = vertices
        .iter()
        .map(|point| unique.iter().find(|entry| entry.0 == *point).unwrap().1)
        .collect();
    let signed_area = shoelace(&vertices);
    let area2 = signed_area.unsigned_abs();
    let (_, perimeter_milli, _) = edge_lengths(&vertices);

    Ok(Hull {
        vertices,
        source_indices,
        area2,
        perimeter_milli,
    })
}

fn main() {}