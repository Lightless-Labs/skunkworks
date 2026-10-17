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

const LIMIT: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

fn in_range(p: Point) -> bool {
    (-LIMIT..=LIMIT).contains(&p.x) && (-LIMIT..=LIMIT).contains(&p.y)
}

fn prepare(points: &[Point]) -> Result<Vec<Point>, GeomError> {
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

fn isqrt(n: u128) -> u128 {
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
    let root = isqrt(scaled);
    let rem = scaled - root * root;
    let rounded = root + u128::from(rem * 4 >= root * 4 + 1);
    rounded as u64
}

fn measures_for(points: &[Point]) -> Measures {
    let signed = signed_area2(points);
    let mut edge_milli = Vec::with_capacity(points.len());
    let mut perimeter = 0u64;
    let mut longest: Option<usize> = None;

    for i in 0..points.len() {
        let length = rounded_edge_milli(points[i], points[(i + 1) % points.len()]);
        edge_milli.push(length);
        perimeter += length;
        match longest {
            None => longest = Some(i),
            Some(best) if length > edge_milli[best] => longest = Some(i),
            Some(best) if length == edge_milli[best] => {
                let pair = endpoint_pair(points[i], points[(i + 1) % points.len()]);
                let best_pair = endpoint_pair(points[best], points[(best + 1) % points.len()]);
                if pair < best_pair || (pair == best_pair && i < best) {
                    longest = Some(i);
                }
            }
            _ => {}
        }
    }

    Measures {
        signed_area2: signed,
        area2: signed.unsigned_abs(),
        orientation: if signed > 0 {
            Orientation::CounterClockwise
        } else if signed < 0 {
            Orientation::Clockwise
        } else {
            Orientation::Degenerate
        },
        edge_milli,
        perimeter_milli: perimeter,
        longest_edge: longest,
    }
}

fn endpoint_pair(a: Point, b: Point) -> (Point, Point) {
    if a <= b { (a, b) } else { (b, a) }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let effective = prepare(points)?;
    Ok(measures_for(&effective))
}

fn on_segment(a: Point, b: Point, q: Point) -> bool {
    cross(a, b, q) == 0
        && q.x >= a.x.min(b.x)
        && q.x <= a.x.max(b.x)
        && q.y >= a.y.min(b.y)
        && q.y <= a.y.max(b.y)
}

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let polygon = prepare(points)?;
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
        if !unique.iter().any(|(p, _)| *p == point) {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|&(point, _)| point);

    if unique.len() <= 1 {
        let vertices: Vec<Point> = unique.iter().map(|x| x.0).collect();
        let source_indices: Vec<usize> = unique.iter().map(|x| x.1).collect();
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
        .all(|&(p, _)| cross(unique[0].0, unique[1].0, p) == 0);

    let output: Vec<(Point, usize)> = if all_collinear {
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
                if turn > 0 || (mode == Collinear::Include && turn == 0) {
                    break;
                }
                lower.pop();
            }
            lower.push(item);
        }

        let mut upper: Vec<(Point, usize)> = Vec::new();
        for &item in unique.iter().rev() {
            while upper.len() >= 2 {
                let n = upper.len();
                let turn = cross(upper[n - 2].0, upper[n - 1].0, item.0);
                if turn > 0 || (mode == Collinear::Include && turn == 0) {
                    break;
                }
                upper.pop();
            }
            upper.push(item);
        }
        lower.pop();
        upper.pop();
        lower.extend(upper);
        lower
    };

    let vertices: Vec<Point> = output.iter().map(|x| x.0).collect();
    let source_indices: Vec<usize> = output.iter().map(|x| x.1).collect();
    let area = signed_area2(&vertices).unsigned_abs();
    let perimeter = if vertices.is_empty() {
        0
    } else {
        (0..vertices.len())
            .map(|i| rounded_edge_milli(vertices[i], vertices[(i + 1) % vertices.len()]))
            .sum()
    };
    Ok(Hull {
        vertices,
        source_indices,
        area2: area,
        perimeter_milli: perimeter,
    })
}

fn dot_at_vertex(prev: Point, current: Point, next: Point) -> i128 {
    (prev.x as i128 - current.x as i128) * (next.x as i128 - current.x as i128)
        + (prev.y as i128 - current.y as i128) * (next.y as i128 - current.y as i128)
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
    let effective = prepare(points)?;
    let n = effective.len();

    for i in 0..n {
        for j in i + 1..n {
            if effective[i] == effective[j] {
                return Err(GeomError::Duplicate {
                    first: i,
                    second: j,
                });
            }
        }
    }

    if signed_area2(&effective) == 0 {
        return Err(GeomError::ZeroArea);
    }

    for i in 0..n {
        let prev = effective[(i + n - 1) % n];
        let current = effective[i];
        let next = effective[(i + 1) % n];
        if cross(prev, current, next) == 0 && dot_at_vertex(prev, current, next) > 0 {
            return Err(GeomError::AdjacentOverlap { vertex: i });
        }
    }

    for i in 0..n {
        for j in i + 1..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            if segments_intersect(
                effective[i],
                effective[(i + 1) % n],
                effective[j],
                effective[(j + 1) % n],
            ) {
                return Err(GeomError::Intersection {
                    edge_a: i,
                    edge_b: j,
                });
            }
        }
    }

    let mut min_index = 0;
    for i in 1..n {
        if effective[i] < effective[min_index] {
            min_index = i;
        }
    }
    let ccw = signed_area2(&effective) > 0;
    let mut vertices = Vec::with_capacity(n);
    let mut source_indices = Vec::with_capacity(n);
    for k in 0..n {
        let index = if ccw {
            (min_index + k) % n
        } else {
            (min_index + n - k) % n
        };
        vertices.push(effective[index]);
        source_indices.push(index);
    }

    let measures = measures_for(&vertices);
    Ok(Validated {
        vertices,
        source_indices,
        measures,
    })
}