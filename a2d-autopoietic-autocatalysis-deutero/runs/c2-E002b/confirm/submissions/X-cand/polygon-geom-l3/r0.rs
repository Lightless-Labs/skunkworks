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

const MAX_COORD: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

fn in_range(p: Point) -> bool {
    (-MAX_COORD..=MAX_COORD).contains(&p.x) && (-MAX_COORD..=MAX_COORD).contains(&p.y)
}

fn prepare_polygon(points: &[Point]) -> Result<&[Point], GeomError> {
    if points.len() > MAX_VERTICES {
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

fn cross_vectors(ax: i128, ay: i128, bx: i128, by: i128) -> i128 {
    ax * by - ay * bx
}

fn cross(a: Point, b: Point, q: Point) -> i128 {
    cross_vectors(
        b.x as i128 - a.x as i128,
        b.y as i128 - a.y as i128,
        q.x as i128 - a.x as i128,
        q.y as i128 - a.y as i128,
    )
}

fn squared_distance(a: Point, b: Point) -> u128 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    (dx * dx + dy * dy) as u128
}

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let target = squared_distance(a, b) * 1_000_000;
    let mut lo = 0u128;
    let mut hi = 3_000_000_000u128;
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if mid * mid <= target {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let remainder = target - lo * lo;
    if remainder * 2 >= 2 * lo + 1 {
        (lo + 1) as u64
    } else {
        lo as u64
    }
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

fn perimeter_and_edges(points: &[Point]) -> (Vec<u64>, u64) {
    let n = points.len();
    let mut edges = Vec::with_capacity(n);
    let mut perimeter = 0u64;
    for i in 0..n {
        let length = rounded_edge_milli(points[i], points[(i + 1) % n]);
        edges.push(length);
        perimeter += length;
    }
    (edges, perimeter)
}

fn longest_edge(points: &[Point], lengths: &[u64]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for i in 0..lengths.len() {
        let Some(current) = best else {
            best = Some(i);
            continue;
        };
        let mut pair_i = (points[i], points[(i + 1) % points.len()]);
        if pair_i.1 < pair_i.0 {
            pair_i = (pair_i.1, pair_i.0);
        }
        let mut pair_best = (points[current], points[(current + 1) % points.len()]);
        if pair_best.1 < pair_best.0 {
            pair_best = (pair_best.1, pair_best.0);
        }
        if lengths[i] > lengths[current]
            || (lengths[i] == lengths[current]
                && (pair_i < pair_best || (pair_i == pair_best && i < current)))
        {
            best = Some(i);
        }
    }
    best
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let points = prepare_polygon(points)?;
    let signed = signed_area2(points);
    let (edge_milli, perimeter_milli) = perimeter_and_edges(points);
    let orientation = match signed.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
    };
    Ok(Measures {
        signed_area2: signed,
        area2: signed.unsigned_abs(),
        orientation,
        longest_edge: longest_edge(points, &edge_milli),
        edge_milli,
        perimeter_milli,
    })
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
        let mut inside = false;
        for i in 0..polygon.len() {
            let a = polygon[i];
            let b = polygon[(i + 1) % polygon.len()];
            if on_segment(a, b, query) {
                boundary = Some(i);
                break;
            }
            if (a.y > query.y) != (b.y > query.y) {
                let c = cross(a, b, query);
                if (b.y > a.y && c > 0) || (b.y < a.y && c < 0) {
                    inside = !inside;
                }
            }
        }
        results.push(match boundary {
            Some(edge) => Location::Boundary { edge },
            None if inside => Location::Inside,
            None => Location::Outside,
        });
    }
    Ok(results)
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

    let mut sorted: Vec<(Point, usize)> = Vec::new();
    for (index, &point) in points.iter().enumerate() {
        if let Some(existing) = sorted.iter_mut().find(|(p, _)| *p == point) {
            existing.1 = existing.1.min(index);
        } else {
            sorted.push((point, index));
        }
    }
    sorted.sort_by_key(|&(point, _)| point);

    let n = sorted.len();
    if n <= 1 {
        return Ok(Hull {
            vertices: sorted.iter().map(|&(p, _)| p).collect(),
            source_indices: sorted.iter().map(|&(_, i)| i).collect(),
            area2: 0,
            perimeter_milli: 0,
        });
    }

    let all_collinear = (2..n).all(|i| cross(sorted[0].0, sorted[n - 1].0, sorted[i].0) == 0);
    let selected: Vec<usize> = if all_collinear {
        match mode {
            Collinear::Include => (0..n).collect(),
            Collinear::Exclude => vec![0, n - 1],
        }
    } else {
        let mut lower: Vec<usize> = Vec::new();
        for i in 0..n {
            while lower.len() >= 2 {
                let m = lower.len();
                let turn = cross(sorted[lower[m - 2]].0, sorted[lower[m - 1]].0, sorted[i].0);
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
            lower.push(i);
        }

        let mut upper: Vec<usize> = Vec::new();
        for i in (0..n).rev() {
            while upper.len() >= 2 {
                let m = upper.len();
                let turn = cross(sorted[upper[m - 2]].0, sorted[upper[m - 1]].0, sorted[i].0);
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
            upper.push(i);
        }
        lower.pop();
        upper.pop();
        lower.extend(upper);
        lower
    };

    let vertices: Vec<Point> = selected.iter().map(|&i| sorted[i].0).collect();
    let source_indices: Vec<usize> = selected.iter().map(|&i| sorted[i].1).collect();
    let area2 = signed_area2(&vertices).unsigned_abs();
    let (_, perimeter_milli) = if vertices.is_empty() {
        (Vec::new(), 0)
    } else {
        perimeter_and_edges(&vertices)
    };

    Ok(Hull {
        vertices,
        source_indices,
        area2,
        perimeter_milli,
    })
}