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

const MAX_VERTICES: usize = 4096;
const COORD_LIMIT: i64 = 1_000_000_000;

fn in_range(p: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&p.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&p.y)
}

fn prepare_polygon(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &p) in points.iter().enumerate() {
        if !in_range(p) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut effective = points.to_vec();
    if effective.len() >= 2 && effective.last() == effective.first() {
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

fn shoelace(points: &[Point]) -> i128 {
    let mut sum = 0i128;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }
    sum
}

fn rounded_length_milli(a: Point, b: Point) -> u64 {
    let dx = a.x as i128 - b.x as i128;
    let dy = a.y as i128 - b.y as i128;
    let squared = (dx * dx + dy * dy) as u128 * 1_000_000u128;

    let mut lo = 0u128;
    let mut hi = 3_000_000_000u128;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if mid * mid < squared {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let ceil_root = lo;
    if ceil_root * ceil_root == squared {
        ceil_root as u64
    } else {
        let floor_root = ceil_root - 1;
        let lower_distance = squared - floor_root * floor_root;
        let upper_distance = ceil_root * ceil_root - squared;
        if lower_distance >= upper_distance {
            ceil_root as u64
        } else {
            floor_root as u64
        }
    }
}

fn measures_for(points: &[Point]) -> Measures {
    let signed_area2 = shoelace(points);
    let orientation = if signed_area2 > 0 {
        Orientation::CounterClockwise
    } else if signed_area2 < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
    };

    let mut edge_milli = Vec::with_capacity(points.len());
    let mut perimeter_milli = 0u64;
    let mut longest_edge: Option<usize> = None;

    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let length = rounded_length_milli(a, b);
        edge_milli.push(length);
        perimeter_milli += length;

        let replace = match longest_edge {
            None => true,
            Some(best) if length > edge_milli[best] => true,
            Some(best) if length < edge_milli[best] => false,
            Some(best) => {
                let (low, high) = if a <= b { (a, b) } else { (b, a) };
                let ba = points[best];
                let bb = points[(best + 1) % points.len()];
                let (best_low, best_high) = if ba <= bb { (ba, bb) } else { (bb, ba) };
                (low, high, i) < (best_low, best_high, best)
            }
        };
        if replace {
            longest_edge = Some(i);
        }
    }

    Measures {
        signed_area2,
        area2: signed_area2.unsigned_abs(),
        orientation,
        edge_milli,
        perimeter_milli,
        longest_edge,
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let effective = prepare_polygon(points)?;
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
    let polygon = prepare_polygon(points)?;

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
                let side = cross(a, b, q);
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

fn between(a: Point, b: Point, p: Point) -> bool {
    p.x >= a.x.min(b.x)
        && p.x <= a.x.max(b.x)
        && p.y >= a.y.min(b.y)
        && p.y <= a.y.max(b.y)
}

fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let ab_c = cross(a, b, c);
    let ab_d = cross(a, b, d);
    let cd_a = cross(c, d, a);
    let cd_b = cross(c, d, b);

    if ab_c == 0 && between(a, b, c) {
        return true;
    }
    if ab_d == 0 && between(a, b, d) {
        return true;
    }
    if cd_a == 0 && between(c, d, a) {
        return true;
    }
    if cd_b == 0 && between(c, d, b) {
        return true;
    }

    (ab_c > 0) != (ab_d > 0) && (cd_a > 0) != (cd_b > 0)
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

    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (index, &p) in points.iter().enumerate() {
        if !unique.iter().any(|(q, _)| *q == p) {
            unique.push((p, index));
        }
    }
    unique.sort_by_key(|(p, _)| *p);

    let mut output: Vec<(Point, usize)> = match unique.len() {
        0 => Vec::new(),
        1 => unique.clone(),
        _ => {
            let all_collinear = unique
                .iter()
                .skip(2)
                .all(|(p, _)| cross(unique[0].0, unique[1].0, *p) == 0);

            if all_collinear {
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
                    lower.push(item);
                }

                let mut upper: Vec<(Point, usize)> = Vec::new();
                for &item in unique.iter().rev() {
                    while upper.len() >= 2 {
                        let n = upper.len();
                        let turn = cross(upper[n - 2].0, upper[n - 1].0, item.0);
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
                    upper.push(item);
                }

                lower.pop();
                upper.pop();
                lower.extend(upper);
                lower
            }
        }
    };

    let vertices: Vec<Point> = output.iter().map(|(p, _)| *p).collect();
    let source_indices: Vec<usize> = output.drain(..).map(|(_, i)| i).collect();
    let area2 = if vertices.len() >= 3 {
        shoelace(&vertices).unsigned_abs()
    } else {
        0
    };
    let perimeter_milli = if vertices.len() >= 2 {
        (0..vertices.len())
            .map(|i| rounded_length_milli(vertices[i], vertices[(i + 1) % vertices.len()]))
            .sum()
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

pub fn validate(points: &[Point]) -> Result<Validated, GeomError> {
    let effective = prepare_polygon(points)?;
    let n = effective.len();

    for i in 0..n {
        for j in (i + 1)..n {
            if effective[i] == effective[j] {
                return Err(GeomError::Duplicate {
                    first: i,
                    second: j,
                });
            }
        }
    }

    let signed_area2 = shoelace(&effective);
    if signed_area2 == 0 {
        return Err(GeomError::ZeroArea);
    }

    for i in 0..n {
        let prev = effective[(i + n - 1) % n];
        let current = effective[i];
        let next = effective[(i + 1) % n];
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

    let start = (0..n).min_by_key(|&i| effective[i]).unwrap();
    let forward = signed_area2 > 0;
    let mut vertices = Vec::with_capacity(n);
    let mut source_indices = Vec::with_capacity(n);
    for step in 0..n {
        let index = if forward {
            (start + step) % n
        } else {
            (start + n - step) % n
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

fn main() {}