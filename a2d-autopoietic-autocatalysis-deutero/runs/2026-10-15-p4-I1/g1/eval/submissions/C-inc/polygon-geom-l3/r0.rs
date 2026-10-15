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
const MAX_POINTS: usize = 4096;

fn in_range(p: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&p.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&p.y)
}

fn prepare_polygon(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > MAX_POINTS {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut effective = points.to_vec();
    if effective.len() >= 2 && effective.first() == effective.last() {
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

fn squared_distance(a: Point, b: Point) -> u128 {
    let dx = (b.x as i128 - a.x as i128) as i128;
    let dy = (b.y as i128 - a.y as i128) as i128;
    (dx * dx + dy * dy) as u128
}

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let target = squared_distance(a, b) * 1_000_000;
    let mut lo = 0u128;
    let mut hi = 3_000_000_001u128;

    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if mid * mid <= target {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    // Round sqrt(target) to nearest integer, with exact half ties upward.
    // Comparing 4*target with (2*lo+1)^2 avoids floating point.
    let midpoint_twice = 2 * lo + 1;
    let rounded = if 4 * target >= midpoint_twice * midpoint_twice {
        lo + 1
    } else {
        lo
    };
    rounded as u64
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

fn perimeter_milli(points: &[Point]) -> u64 {
    if points.is_empty() {
        return 0;
    }
    let mut sum = 0u64;
    for i in 0..points.len() {
        sum += rounded_edge_milli(points[i], points[(i + 1) % points.len()]);
    }
    sum
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare_polygon(points)?;
    let signed_area2 = shoelace(&polygon);
    let orientation = if signed_area2 > 0 {
        Orientation::CounterClockwise
    } else if signed_area2 < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
    };

    let mut edge_milli = Vec::with_capacity(polygon.len());
    let mut perimeter = 0u64;
    let mut longest_edge = None;

    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        let length = rounded_edge_milli(a, b);
        edge_milli.push(length);
        perimeter += length;

        let replace = match longest_edge {
            None => true,
            Some(previous) if length > edge_milli[previous] => true,
            Some(previous) if length < edge_milli[previous] => false,
            Some(previous) => {
                let mut pair = (a, b);
                if pair.1 < pair.0 {
                    pair = (pair.1, pair.0);
                }
                let old_a = polygon[previous];
                let old_b = polygon[(previous + 1) % polygon.len()];
                let mut old_pair = (old_a, old_b);
                if old_pair.1 < old_pair.0 {
                    old_pair = (old_pair.1, old_pair.0);
                }
                pair < old_pair || (pair == old_pair && i < previous)
            }
        };
        if replace {
            longest_edge = Some(i);
        }
    }

    Ok(Measures {
        signed_area2,
        area2: signed_area2.unsigned_abs(),
        orientation,
        edge_milli,
        perimeter_milli: perimeter,
        longest_edge,
    })
}

fn on_segment(a: Point, b: Point, q: Point) -> bool {
    cross(a, b, q) == 0
        && (q.x as i128) >= (a.x.min(b.x) as i128)
        && (q.x as i128) <= (a.x.max(b.x) as i128)
        && (q.y as i128) >= (a.y.min(b.y) as i128)
        && (q.y as i128) <= (a.y.max(b.y) as i128)
}

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let polygon = prepare_polygon(points)?;

    if queries.len() > MAX_POINTS {
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
                let c = cross(a, b, query);
                if (b.y > a.y && c > 0) || (b.y < a.y && c < 0) {
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
    if points.len() > MAX_POINTS {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (index, &point) in points.iter().enumerate() {
        if !unique.iter().any(|(existing, _)| *existing == point) {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|(point, _)| *point);

    let mut hull: Vec<(Point, usize)> = Vec::new();
    if unique.len() <= 1 {
        hull = unique;
    } else {
        let all_collinear = unique
            .iter()
            .skip(2)
            .all(|(point, _)| cross(unique[0].0, unique[1].0, *point) == 0);

        if all_collinear {
            hull = match mode {
                Collinear::Include => unique,
                Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
            };
        } else {
            let should_pop = |a: Point, b: Point, c: Point| match mode {
                Collinear::Exclude => cross(a, b, c) <= 0,
                Collinear::Include => cross(a, b, c) < 0,
            };

            let mut lower: Vec<(Point, usize)> = Vec::new();
            for &item in &unique {
                while lower.len() >= 2
                    && should_pop(lower[lower.len() - 2].0, lower[lower.len() - 1].0, item.0)
                {
                    lower.pop();
                }
                lower.push(item);
            }

            let mut upper: Vec<(Point, usize)> = Vec::new();
            for &item in unique.iter().rev() {
                while upper.len() >= 2
                    && should_pop(upper[upper.len() - 2].0, upper[upper.len() - 1].0, item.0)
                {
                    upper.pop();
                }
                upper.push(item);
            }

            lower.pop();
            upper.pop();
            lower.extend(upper);
            hull = lower;
        }
    }

    let vertices: Vec<Point> = hull.iter().map(|(point, _)| *point).collect();
    let source_indices: Vec<usize> = hull.iter().map(|(_, index)| *index).collect();
    let area2 = if vertices.len() < 3 {
        0
    } else {
        shoelace(&vertices).unsigned_abs()
    };
    let perimeter_milli = perimeter_milli(&vertices);

    Ok(Hull {
        vertices,
        source_indices,
        area2,
        perimeter_milli,
    })
}

fn main() {}