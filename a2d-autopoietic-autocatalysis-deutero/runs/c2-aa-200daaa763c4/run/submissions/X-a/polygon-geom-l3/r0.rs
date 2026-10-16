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

const MAX_VERTICES: usize = 4096;
const COORD_LIMIT: i64 = 1_000_000_000;

fn valid_point(p: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&p.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&p.y)
}

fn prepare_polygon(points: &[Point]) -> Result<&[Point], GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !valid_point(point) {
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
    Ok(&points[..effective_len])
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

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    let squared = (dx * dx + dy * dy) as u128 * 1_000_000u128;

    let mut lo = 0u128;
    let mut hi = 3_000_000_000_000u128;
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if mid * mid <= squared {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let lower_square = lo * lo;
    let next = lo + 1;
    let upper_square = next * next;
    let rounded = if squared - lower_square >= upper_square - squared {
        next
    } else {
        lo
    };
    rounded as u64
}

fn edge_lengths(points: &[Point]) -> (Vec<u64>, u64) {
    let mut lengths = Vec::with_capacity(points.len());
    let mut perimeter = 0u64;
    for i in 0..points.len() {
        let length = rounded_edge_milli(points[i], points[(i + 1) % points.len()]);
        lengths.push(length);
        perimeter += length;
    }
    (lengths, perimeter)
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare_polygon(points)?;
    let signed_area2 = shoelace(polygon);
    let orientation = if signed_area2 > 0 {
        Orientation::CounterClockwise
    } else if signed_area2 < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
    };
    let (edge_milli, perimeter_milli) = edge_lengths(polygon);

    let mut longest_edge = None;
    for i in 0..polygon.len() {
        match longest_edge {
            None => longest_edge = Some(i),
            Some(best) => {
                let candidate_length = edge_milli[i];
                let best_length = edge_milli[best];
                let candidate_pair = if polygon[i] <= polygon[(i + 1) % polygon.len()] {
                    (polygon[i], polygon[(i + 1) % polygon.len()])
                } else {
                    (polygon[(i + 1) % polygon.len()], polygon[i])
                };
                let best_pair = if polygon[best] <= polygon[(best + 1) % polygon.len()] {
                    (polygon[best], polygon[(best + 1) % polygon.len()])
                } else {
                    (polygon[(best + 1) % polygon.len()], polygon[best])
                };
                if candidate_length > best_length
                    || (candidate_length == best_length
                        && (candidate_pair < best_pair
                            || (candidate_pair == best_pair && i < best)))
                {
                    longest_edge = Some(i);
                }
            }
        }
    }

    Ok(Measures {
        signed_area2,
        area2: signed_area2.unsigned_abs(),
        orientation,
        edge_milli,
        perimeter_milli,
        longest_edge,
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
        if !valid_point(query) {
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
                if boundary.is_none() {
                    boundary = Some(i);
                }
            } else if (a.y > query.y) != (b.y > query.y) {
                let c = cross(a, b, query);
                if (b.y > a.y && c > 0) || (b.y < a.y && c < 0) {
                    inside = !inside;
                }
            }
        }
        results.push(if let Some(edge) = boundary {
            Location::Boundary { edge }
        } else if inside {
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
    for (index, &point) in points.iter().enumerate() {
        if !valid_point(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (index, &point) in points.iter().enumerate() {
        if !unique.iter().any(|(existing, _)| *existing == point) {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|entry| entry.0);

    if unique.len() <= 1 {
        let vertices: Vec<Point> = unique.iter().map(|entry| entry.0).collect();
        let source_indices: Vec<usize> = unique.iter().map(|entry| entry.1).collect();
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
        .all(|entry| cross(unique[0].0, unique[1].0, entry.0) == 0);

    if all_collinear {
        let selected: Vec<(Point, usize)> = match mode {
            Collinear::Include => unique,
            Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
        };
        let vertices: Vec<Point> = selected.iter().map(|entry| entry.0).collect();
        let source_indices: Vec<usize> = selected.iter().map(|entry| entry.1).collect();
        let (_, perimeter_milli) = edge_lengths(&vertices);
        return Ok(Hull {
            vertices,
            source_indices,
            area2: 0,
            perimeter_milli,
        });
    }

    let mut lower: Vec<(Point, usize)> = Vec::new();
    for &entry in &unique {
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
    for &entry in unique.iter().rev() {
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

    let vertices: Vec<Point> = lower.iter().map(|entry| entry.0).collect();
    let source_indices: Vec<usize> = lower.iter().map(|entry| entry.1).collect();
    let area2 = shoelace(&vertices).unsigned_abs();
    let (_, perimeter_milli) = edge_lengths(&vertices);

    Ok(Hull {
        vertices,
        source_indices,
        area2,
        perimeter_milli,
    })
}