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

const LIMIT: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

fn in_range(p: Point) -> bool {
    (-LIMIT..=LIMIT).contains(&p.x) && (-LIMIT..=LIMIT).contains(&p.y)
}

fn prepare(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &p) in points.iter().enumerate() {
        if !in_range(p) {
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
    (b.x as i128 - a.x as i128) * (c.y as i128 - a.y as i128)
        - (b.y as i128 - a.y as i128) * (c.x as i128 - a.x as i128)
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
    // Integer square root by binary search; all values here are bounded by
    // the accepted coordinate range.
    let mut lo = 0u128;
    let mut hi = n.min(u64::MAX as u128) + 1;
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if mid <= n / mid {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let remainder = n - lo * lo;
    if remainder >= lo + 1 {
        lo + 1
    } else {
        lo
    }
}

fn edge_length_milli(a: Point, b: Point) -> u64 {
    let dx = (a.x as i128 - b.x as i128).unsigned_abs();
    let dy = (a.y as i128 - b.y as i128).unsigned_abs();
    let scaled = (dx * dx + dy * dy) * 1_000_000u128;
    rounded_sqrt(scaled) as u64
}

fn measure_prepared(points: &[Point]) -> Measures {
    let signed = signed_area2(points);
    let mut edge_milli = Vec::with_capacity(points.len());
    let mut perimeter_milli = 0u64;

    for i in 0..points.len() {
        let length = edge_length_milli(points[i], points[(i + 1) % points.len()]);
        edge_milli.push(length);
        perimeter_milli += length;
    }

    let longest_edge = if edge_milli.is_empty() {
        None
    } else {
        let mut best = 0usize;
        for i in 1..edge_milli.len() {
            let pair_i = ordered_endpoints(points[i], points[(i + 1) % points.len()]);
            let pair_best =
                ordered_endpoints(points[best], points[(best + 1) % points.len()]);
            if edge_milli[i] > edge_milli[best]
                || (edge_milli[i] == edge_milli[best]
                    && (pair_i < pair_best || (pair_i == pair_best && i < best)))
            {
                best = i;
            }
        }
        Some(best)
    };

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
        perimeter_milli,
        longest_edge,
    }
}

fn ordered_endpoints(a: Point, b: Point) -> (Point, Point) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let prepared = prepare(points)?;
    Ok(measure_prepared(&prepared))
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
    for (index, &q) in queries.iter().enumerate() {
        if !in_range(q) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    let mut results = Vec::with_capacity(queries.len());
    for &q in queries {
        let mut boundary = None;
        for i in 0..polygon.len() {
            if on_segment(polygon[i], polygon[(i + 1) % polygon.len()], q) {
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
            if (a.y > q.y) != (b.y > q.y) {
                let turn = cross(a, b, q);
                if (b.y > a.y && turn > 0) || (b.y < a.y && turn < 0) {
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
    for (index, &p) in points.iter().enumerate() {
        if !in_range(p) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut first_index = BTreeMap::<Point, usize>::new();
    for (i, &p) in points.iter().enumerate() {
        first_index.entry(p).or_insert(i);
    }
    let unique: Vec<Point> = first_index.keys().copied().collect();

    let (hull_points, all_collinear) = if unique.len() <= 2 {
        (unique.clone(), true)
    } else {
        let collinear = unique
            .iter()
            .skip(2)
            .all(|&p| cross(unique[0], unique[1], p) == 0);
        if collinear {
            (unique.clone(), true)
        } else {
            let mut lower: Vec<Point> = Vec::new();
            for &p in &unique {
                while lower.len() >= 2 {
                    let c = cross(lower[lower.len() - 2], lower[lower.len() - 1], p);
                    let should_pop = match mode {
                        Collinear::Exclude => c <= 0,
                        Collinear::Include => c < 0,
                    };
                    if should_pop {
                        lower.pop();
                    } else {
                        break;
                    }
                }
                lower.push(p);
            }

            let mut upper: Vec<Point> = Vec::new();
            for &p in unique.iter().rev() {
                while upper.len() >= 2 {
                    let c = cross(upper[upper.len() - 2], upper[upper.len() - 1], p);
                    let should_pop = match mode {
                        Collinear::Exclude => c <= 0,
                        Collinear::Include => c < 0,
                    };
                    if should_pop {
                        upper.pop();
                    } else {
                        break;
                    }
                }
                upper.push(p);
            }
            lower.pop();
            upper.pop();
            lower.extend(upper);
            (lower, false)
        }
    };

    let final_points = if all_collinear && mode == Collinear::Exclude && hull_points.len() > 2 {
        vec![hull_points[0], *hull_points.last().unwrap()]
    } else {
        hull_points
    };

    let source_indices = final_points
        .iter()
        .map(|p| first_index[p])
        .collect::<Vec<_>>();
    let area = if final_points.len() >= 3 {
        signed_area2(&final_points).unsigned_abs()
    } else {
        0
    };
    let perimeter = if final_points.is_empty() {
        0
    } else {
        (0..final_points.len())
            .map(|i| edge_length_milli(final_points[i], final_points[(i + 1) % final_points.len()]))
            .sum()
    };

    Ok(Hull {
        vertices: final_points,
        source_indices,
        area2: area,
        perimeter_milli: perimeter,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validated {
    pub vertices: Vec<Point>,
    pub source_indices: Vec<usize>,
    pub measures: Measures,
}

pub fn validate(points: &[Point]) -> Result<Validated, GeomError> {
    let effective = prepare(points)?;

    let mut first_seen = BTreeMap::<Point, usize>::new();
    let mut duplicate: Option<(usize, usize)> = None;
    for (j, &p) in effective.iter().enumerate() {
        if let Some(&i) = first_seen.get(&p) {
            let pair = (i, j);
            if duplicate.map_or(true, |old| pair < old) {
                duplicate = Some(pair);
            }
        } else {
            first_seen.insert(p, j);
        }
    }
    if let Some((first, second)) = duplicate {
        return Err(GeomError::Duplicate { first, second });
    }

    let area = signed_area2(&effective);
    if area == 0 {
        return Err(GeomError::ZeroArea);
    }

    let n = effective.len();
    for i in 0..n {
        let prev = effective[(i + n - 1) % n];
        let curr = effective[i];
        let next = effective[(i + 1) % n];
        if cross(prev, curr, next) == 0 {
            let dot = (prev.x as i128 - curr.x as i128) * (next.x as i128 - curr.x as i128)
                + (prev.y as i128 - curr.y as i128) * (next.y as i128 - curr.y as i128);
            if dot > 0 {
                return Err(GeomError::AdjacentOverlap { vertex: i });
            }
        }
    }

    for i in 0..n {
        let a = effective[i];
        let b = effective[(i + 1) % n];
        for j in (i + 1)..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let c = effective[j];
            let d = effective[(j + 1) % n];
            if segments_intersect(a, b, c, d) {
                return Err(GeomError::Intersection {
                    edge_a: i,
                    edge_b: j,
                });
            }
        }
    }

    let min_index = effective
        .iter()
        .enumerate()
        .min_by_key(|(_, p)| **p)
        .unwrap()
        .0;

    let mut order = Vec::with_capacity(n);
    if area > 0 {
        for k in 0..n {
            order.push((min_index + k) % n);
        }
    } else {
        for k in 0..n {
            order.push((min_index + n - k) % n);
        }
    }

    let vertices = order.iter().map(|&i| effective[i]).collect::<Vec<_>>();
    let source_indices = order;
    let measures = measure_prepared(&vertices);

    Ok(Validated {
        vertices,
        source_indices,
        measures,
    })
}