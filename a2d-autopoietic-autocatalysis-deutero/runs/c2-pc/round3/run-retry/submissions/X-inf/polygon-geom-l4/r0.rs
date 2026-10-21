use std::cmp::Ordering;
use std::collections::BTreeMap;

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

const MAX_COORD: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

fn in_range(p: Point) -> bool {
    (-MAX_COORD..=MAX_COORD).contains(&p.x) && (-MAX_COORD..=MAX_COORD).contains(&p.y)
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

    let effective_len = if points.len() >= 2 && points[0] == points[points.len() - 1] {
        points.len() - 1
    } else {
        points.len()
    };
    if effective_len < 3 {
        return Err(GeomError::TooFewVertices { len: effective_len });
    }
    Ok(points[..effective_len].to_vec())
}

fn cross_vec(ax: i128, ay: i128, bx: i128, by: i128) -> i128 {
    ax * by - ay * bx
}

fn cross(a: Point, b: Point, c: Point) -> i128 {
    cross_vec(
        b.x as i128 - a.x as i128,
        b.y as i128 - a.y as i128,
        c.x as i128 - a.x as i128,
        c.y as i128 - a.y as i128,
    )
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
    let rem = n - lo * lo;
    if rem >= lo + 1 {
        lo + 1
    } else {
        lo
    }
}

fn edge_mill(a: Point, b: Point) -> u64 {
    let dx = (b.x as i128 - a.x as i128) as i128;
    let dy = (b.y as i128 - a.y as i128) as i128;
    let scaled = ((dx * dx + dy * dy) as u128) * 1_000_000u128;
    rounded_sqrt(scaled) as u64
}

fn measures_for(points: &[Point]) -> Measures {
    let signed = signed_area2(points);
    let orientation = match signed.cmp(&0) {
        Ordering::Less => Orientation::Clockwise,
        Ordering::Equal => Orientation::Degenerate,
        Ordering::Greater => Orientation::CounterClockwise,
    };

    let edge_milli: Vec<u64> = (0..points.len())
        .map(|i| edge_mill(points[i], points[(i + 1) % points.len()]))
        .collect();
    let perimeter_milli = edge_milli.iter().copied().sum();

    let mut longest_edge = None;
    for i in 0..points.len() {
        match longest_edge {
            None => longest_edge = Some(i),
            Some(best) => {
                if edge_milli[i] > edge_milli[best] {
                    longest_edge = Some(i);
                } else if edge_milli[i] == edge_milli[best] {
                    let pair_i = if points[i] <= points[(i + 1) % points.len()] {
                        (points[i], points[(i + 1) % points.len()])
                    } else {
                        (points[(i + 1) % points.len()], points[i])
                    };
                    let pair_best = if points[best] <= points[(best + 1) % points.len()] {
                        (points[best], points[(best + 1) % points.len()])
                    } else {
                        (points[(best + 1) % points.len()], points[best])
                    };
                    if pair_i < pair_best || (pair_i == pair_best && i < best) {
                        longest_edge = Some(i);
                    }
                }
            }
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
    let polygon = prepare(points)?;
    Ok(measures_for(&polygon))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Outside,
    Inside,
    Boundary { edge: usize },
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

    Ok(queries
        .iter()
        .map(|&q| {
            for i in 0..polygon.len() {
                if on_segment(polygon[i], polygon[(i + 1) % polygon.len()], q) {
                    return Location::Boundary { edge: i };
                }
            }
            let mut inside = false;
            for i in 0..polygon.len() {
                let a = polygon[i];
                let b = polygon[(i + 1) % polygon.len()];
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
        })
        .collect())
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

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut first_index = BTreeMap::<Point, usize>::new();
    for (i, &point) in points.iter().enumerate() {
        first_index.entry(point).or_insert(i);
    }
    let unique: Vec<Point> = first_index.keys().copied().collect();

    let mut hull = if unique.len() <= 1 {
        unique
    } else {
        let all_collinear = unique.iter().skip(2).all(|&p| cross(unique[0], unique[1], p) == 0);
        if all_collinear {
            match mode {
                Collinear::Include => unique.clone(),
                Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
            }
        } else {
            let mut lower = Vec::new();
            for &p in &unique {
                while lower.len() >= 2 {
                    let n = lower.len();
                    let c = cross(lower[n - 2], lower[n - 1], p);
                    if c > 0 || (mode == Collinear::Include && c == 0) {
                        break;
                    }
                    lower.pop();
                }
                lower.push(p);
            }

            let mut upper = Vec::new();
            for &p in unique.iter().rev() {
                while upper.len() >= 2 {
                    let n = upper.len();
                    let c = cross(upper[n - 2], upper[n - 1], p);
                    if c > 0 || (mode == Collinear::Include && c == 0) {
                        break;
                    }
                    upper.pop();
                }
                upper.push(p);
            }

            lower.pop();
            upper.pop();
            lower.extend(upper);
            lower
        }
    };

    if hull.len() > 1 {
        let start = hull
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| **p)
            .map(|(i, _)| i)
            .unwrap();
        hull.rotate_left(start);
    }

    let source_indices = hull.iter().map(|p| first_index[p]).collect();
    let area = if hull.len() >= 3 {
        signed_area2(&hull).unsigned_abs()
    } else {
        0
    };
    let perimeter = match hull.len() {
        0 | 1 => 0,
        n => (0..n)
            .map(|i| edge_mill(hull[i], hull[(i + 1) % n]))
            .sum(),
    };

    Ok(Hull {
        vertices: hull,
        source_indices,
        area2: area,
        perimeter_milli: perimeter,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validated {
    pub vertices: Vec<Point>,
    pub source_indices: Vec<usize>,
    pub measures: Measures,
}

fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let o1 = cross(a, b, c);
    let o2 = cross(a, b, d);
    let o3 = cross(c, d, a);
    let o4 = cross(c, d, b);

    if o1 == 0 && on_segment(a, b, c) {
        return true;
    }
    if o2 == 0 && on_segment(a, b, d) {
        return true;
    }
    if o3 == 0 && on_segment(c, d, a) {
        return true;
    }
    if o4 == 0 && on_segment(c, d, b) {
        return true;
    }
    (o1 > 0) != (o2 > 0) && (o3 > 0) != (o4 > 0)
}

pub fn validate(points: &[Point]) -> Result<Validated, GeomError> {
    let polygon = prepare(points)?;
    let n = polygon.len();

    let mut duplicate = None;
    for i in 0..n {
        for j in (i + 1)..n {
            if polygon[i] == polygon[j] {
                duplicate = Some((i, j));
                break;
            }
        }
        if duplicate.is_some() {
            break;
        }
    }
    if let Some((first, second)) = duplicate {
        return Err(GeomError::Duplicate { first, second });
    }

    let signed = signed_area2(&polygon);
    if signed == 0 {
        return Err(GeomError::ZeroArea);
    }

    for i in 0..n {
        let prev = polygon[(i + n - 1) % n];
        let current = polygon[i];
        let next = polygon[(i + 1) % n];
        if cross(prev, current, next) == 0 {
            let dx1 = prev.x as i128 - current.x as i128;
            let dy1 = prev.y as i128 - current.y as i128;
            let dx2 = next.x as i128 - current.x as i128;
            let dy2 = next.y as i128 - current.y as i128;
            if dx1 * dx2 + dy1 * dy2 > 0 {
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

    let min_index = (0..n).min_by_key(|&i| polygon[i]).unwrap();
    let mut order: Vec<usize> = if signed > 0 {
        (0..n).map(|offset| (min_index + offset) % n).collect()
    } else {
        (0..n).map(|offset| (min_index + n - offset) % n).collect()
    };

    let vertices: Vec<Point> = order.iter().map(|&i| polygon[i]).collect();
    let source_indices = std::mem::take(&mut order);
    let measures = measures_for(&vertices);

    Ok(Validated {
        vertices,
        source_indices,
        measures,
    })
}