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
    Duplicate { first: usize, second: usize },
    ZeroArea,
    AdjacentOverlap { vertex: usize },
    Intersection { edge_a: usize, edge_b: usize },
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validated {
    pub vertices: Vec<Point>,
    pub source_indices: Vec<usize>,
    pub measures: Measures,
}

const MAX_COORD: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

fn in_range(p: Point) -> bool {
    (-MAX_COORD..=MAX_COORD).contains(&p.x) && (-MAX_COORD..=MAX_COORD).contains(&p.y)
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

    let effective_len = if points.len() >= 2 && points[points.len() - 1] == points[0] {
        points.len() - 1
    } else {
        points.len()
    };

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

fn signed_area2(points: &[Point]) -> i128 {
    let mut sum = 0i128;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }
    sum
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

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    let d2 = (dx * dx + dy * dy) as u128;
    let scaled = d2 * 1_000_000;
    let floor = integer_sqrt(scaled);
    let midpoint_test = 4 * scaled;
    let midpoint_square = (2 * floor + 1) * (2 * floor + 1);
    let rounded = if midpoint_test >= midpoint_square {
        floor + 1
    } else {
        floor
    };
    rounded as u64
}

fn endpoint_pair(a: Point, b: Point) -> (Point, Point) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn measures_for(points: &[Point]) -> Measures {
    let signed = signed_area2(points);
    let orientation = if signed > 0 {
        Orientation::CounterClockwise
    } else if signed < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
    };

    let mut edge_milli = Vec::with_capacity(points.len());
    let mut perimeter_milli = 0u64;
    let mut longest_edge: Option<usize> = None;

    for i in 0..points.len() {
        let length = rounded_edge_milli(points[i], points[(i + 1) % points.len()]);
        edge_milli.push(length);
        perimeter_milli += length;

        let replace = match longest_edge {
            None => true,
            Some(best) => {
                if length > edge_milli[best] {
                    true
                } else if length < edge_milli[best] {
                    false
                } else {
                    let pair = endpoint_pair(points[i], points[(i + 1) % points.len()]);
                    let best_pair = endpoint_pair(points[best], points[(best + 1) % points.len()]);
                    pair < best_pair || (pair == best_pair && i < best)
                }
            }
        };
        if replace {
            longest_edge = Some(i);
        }
    }

    Measures {
        signed_area2: signed,
        area2: signed.unsigned_abs(),
        orientation,
        edge_milli,
        perimeter_milli,
        longest_edge,
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare_polygon(points)?;
    Ok(measures_for(&polygon))
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
        let mut boundary_edge = None;
        let mut inside = false;

        for i in 0..polygon.len() {
            let a = polygon[i];
            let b = polygon[(i + 1) % polygon.len()];

            if on_segment(a, b, query) {
                boundary_edge = Some(i);
                break;
            }

            if (a.y > query.y) != (b.y > query.y) {
                let side = cross(a, b, query);
                if (b.y > a.y && side > 0) || (b.y < a.y && side < 0) {
                    inside = !inside;
                }
            }
        }

        results.push(match boundary_edge {
            Some(edge) => Location::Boundary { edge },
            None if inside => Location::Inside,
            None => Location::Outside,
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

    let mut sorted: Vec<(Point, usize)> = points
        .iter()
        .copied()
        .enumerate()
        .map(|(index, point)| (point, index))
        .collect();
    sorted.sort_by_key(|&(point, index)| (point, index));

    let mut distinct: Vec<(Point, usize)> = Vec::new();
    for item in sorted {
        if distinct.last().map(|last| last.0) != Some(item.0) {
            distinct.push(item);
        }
    }

    let (vertices, source_indices): (Vec<Point>, Vec<usize>) = if distinct.len() <= 1 {
        (
            distinct.iter().map(|item| item.0).collect(),
            distinct.iter().map(|item| item.1).collect(),
        )
    } else {
        let all_collinear = distinct
            .iter()
            .skip(2)
            .all(|item| cross(distinct[0].0, distinct[1].0, item.0) == 0);

        let hull = if all_collinear {
            match mode {
                Collinear::Include => distinct.clone(),
                Collinear::Exclude => vec![distinct[0], *distinct.last().unwrap()],
            }
        } else {
            let mut lower: Vec<(Point, usize)> = Vec::new();
            for &item in &distinct {
                while lower.len() >= 2 {
                    let turn = cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, item.0);
                    let should_pop = match mode {
                        Collinear::Include => turn < 0,
                        Collinear::Exclude => turn <= 0,
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
            for &item in distinct.iter().rev() {
                while upper.len() >= 2 {
                    let turn = cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, item.0);
                    let should_pop = match mode {
                        Collinear::Include => turn < 0,
                        Collinear::Exclude => turn <= 0,
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
        };

        (
            hull.iter().map(|item| item.0).collect(),
            hull.iter().map(|item| item.1).collect(),
        )
    };

    let area2 = if vertices.len() >= 3 {
        signed_area2(&vertices).unsigned_abs()
    } else {
        0
    };
    let perimeter_milli = if vertices.len() >= 2 {
        measures_for(&vertices).perimeter_milli
    } else {
        0
    };

    Ok(Hull {
        vertices,
        source_indices,
        area2,
        perimeter_milli,
    })
}

fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let ab_c = cross(a, b, c);
    let ab_d = cross(a, b, d);
    let cd_a = cross(c, d, a);
    let cd_b = cross(c, d, b);

    if ab_c == 0 && on_segment(a, b, c) {
        return true;
    }
    if ab_d == 0 && on_segment(a, b, d) {
        return true;
    }
    if cd_a == 0 && on_segment(c, d, a) {
        return true;
    }
    if cd_b == 0 && on_segment(c, d, b) {
        return true;
    }

    (ab_c > 0) != (ab_d > 0) && (cd_a > 0) != (cd_b > 0)
}

pub fn validate(points: &[Point]) -> Result<Validated, GeomError> {
    let polygon = prepare_polygon(points)?;
    let n = polygon.len();

    for first in 0..n {
        for second in (first + 1)..n {
            if polygon[first] == polygon[second] {
                return Err(GeomError::Duplicate { first, second });
            }
        }
    }

    let area = signed_area2(&polygon);
    if area == 0 {
        return Err(GeomError::ZeroArea);
    }

    for i in 0..n {
        let prev = polygon[(i + n - 1) % n];
        let current = polygon[i];
        let next = polygon[(i + 1) % n];
        if cross(prev, current, next) == 0 {
            let ax = prev.x as i128 - current.x as i128;
            let ay = prev.y as i128 - current.y as i128;
            let bx = next.x as i128 - current.x as i128;
            let by = next.y as i128 - current.y as i128;
            if ax * bx + ay * by > 0 {
                return Err(GeomError::AdjacentOverlap { vertex: i });
            }
        }
    }

    for i in 0..n {
        for j in (i + 1)..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            if segments_intersect(
                polygon[i],
                polygon[(i + 1) % n],
                polygon[j],
                polygon[(j + 1) % n],
            ) {
                return Err(GeomError::Intersection {
                    edge_a: i,
                    edge_b: j,
                });
            }
        }
    }

    let mut indices: Vec<usize> = (0..n).collect();
    if area < 0 {
        indices.reverse();
    }
    let min_position = indices
        .iter()
        .enumerate()
        .min_by_key(|&(_, &index)| polygon[index])
        .map(|(position, _)| position)
        .unwrap();

    indices.rotate_left(min_position);
    let vertices: Vec<Point> = indices.iter().map(|&index| polygon[index]).collect();
    let measures = measures_for(&vertices);

    Ok(Validated {
        vertices,
        source_indices: indices,
        measures,
    })
}