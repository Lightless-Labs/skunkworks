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

fn prepare(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !valid_point(point) {
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

fn cross(a: Point, b: Point, q: Point) -> i128 {
    let abx = b.x as i128 - a.x as i128;
    let aby = b.y as i128 - a.y as i128;
    let aqx = q.x as i128 - a.x as i128;
    let aqy = q.y as i128 - a.y as i128;
    abx * aqy - aby * aqx
}

fn squared_distance(a: Point, b: Point) -> u128 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    (dx * dx + dy * dy) as u128
}

fn rounded_edge_milli(a: Point, b: Point) -> u64 {
    let target = squared_distance(a, b) * 1_000_000;
    let mut lo = 0u128;
    let mut hi = 3_000_000_000_000u128;
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if mid * mid <= target {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }

    // Compare sqrt(target) with lo + 1/2 without using floating point:
    // target >= (lo + 1/2)^2 iff 4*target >= (2*lo + 1)^2.
    let midpoint_twice = 2 * lo + 1;
    if 4 * target >= midpoint_twice * midpoint_twice {
        (lo + 1) as u64
    } else {
        lo as u64
    }
}

fn measures_for(points: &[Point]) -> Measures {
    let n = points.len();
    let mut signed_area2 = 0i128;
    let mut edge_milli = Vec::with_capacity(n);
    let mut perimeter_milli = 0u64;
    let mut longest_edge = None;

    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        signed_area2 += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;

        let length = rounded_edge_milli(a, b);
        edge_milli.push(length);
        perimeter_milli += length;

        let replace = match longest_edge {
            None => true,
            Some(previous) if length > edge_milli[previous] => true,
            Some(previous) if length < edge_milli[previous] => false,
            Some(previous) => {
                let old_a = points[previous];
                let old_b = points[(previous + 1) % n];
                let new_pair = if a <= b { (a, b) } else { (b, a) };
                let old_pair = if old_a <= old_b {
                    (old_a, old_b)
                } else {
                    (old_b, old_a)
                };
                new_pair < old_pair || (new_pair == old_pair && i < previous)
            }
        };
        if replace {
            longest_edge = Some(i);
        }
    }

    let orientation = if signed_area2 > 0 {
        Orientation::CounterClockwise
    } else if signed_area2 < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
    };

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
    let effective = prepare(points)?;
    Ok(measures_for(&effective))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Outside,
    Inside,
    Boundary { edge: usize },
}

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let polygon = prepare(points)?;

    if queries.len() > MAX_VERTICES {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (index, &query) in queries.iter().enumerate() {
        if !valid_point(query) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    let n = polygon.len();
    let mut results = Vec::with_capacity(queries.len());
    for &q in queries {
        let mut boundary = None;
        let mut inside = false;

        for i in 0..n {
            let a = polygon[i];
            let b = polygon[(i + 1) % n];
            if cross(a, b, q) == 0
                && q.x >= a.x.min(b.x)
                && q.x <= a.x.max(b.x)
                && q.y >= a.y.min(b.y)
                && q.y <= a.y.max(b.y)
            {
                boundary = Some(i);
                break;
            }
            if (a.y > q.y) != (b.y > q.y) {
                let side = cross(a, b, q);
                if (b.y > a.y && side > 0) || (b.y < a.y && side < 0) {
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
        if !valid_point(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut indexed: Vec<(Point, usize)> = Vec::new();
    for (index, &point) in points.iter().enumerate() {
        if let Some(existing) = indexed.iter_mut().find(|entry| entry.0 == point) {
            existing.1 = existing.1.min(index);
        } else {
            indexed.push((point, index));
        }
    }
    indexed.sort_by_key(|entry| entry.0);

    if indexed.len() <= 1 {
        let vertices: Vec<Point> = indexed.iter().map(|entry| entry.0).collect();
        let source_indices: Vec<usize> = indexed.iter().map(|entry| entry.1).collect();
        return Ok(Hull {
            vertices,
            source_indices,
            area2: 0,
            perimeter_milli: 0,
        });
    }

    let all_collinear = indexed
        .iter()
        .skip(2)
        .all(|entry| cross(indexed[0].0, indexed[1].0, entry.0) == 0);

    let selected: Vec<(Point, usize)> = if all_collinear {
        match mode {
            Collinear::Include => indexed.clone(),
            Collinear::Exclude => vec![indexed[0], *indexed.last().unwrap()],
        }
    } else {
        let mut lower = Vec::<(Point, usize)>::new();
        for &entry in &indexed {
            while lower.len() >= 2 {
                let turn = cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, entry.0);
                let remove = match mode {
                    Collinear::Exclude => turn <= 0,
                    Collinear::Include => turn < 0,
                };
                if !remove {
                    break;
                }
                lower.pop();
            }
            lower.push(entry);
        }

        let mut upper = Vec::<(Point, usize)>::new();
        for &entry in indexed.iter().rev() {
            while upper.len() >= 2 {
                let turn = cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, entry.0);
                let remove = match mode {
                    Collinear::Exclude => turn <= 0,
                    Collinear::Include => turn < 0,
                };
                if !remove {
                    break;
                }
                upper.pop();
            }
            upper.push(entry);
        }
        lower.pop();
        upper.pop();
        lower.extend(upper);
        lower
    };

    let vertices: Vec<Point> = selected.iter().map(|entry| entry.0).collect();
    let source_indices: Vec<usize> = selected.iter().map(|entry| entry.1).collect();
    let (area2, perimeter_milli) = if vertices.len() >= 3 {
        let measures = measures_for(&vertices);
        (measures.area2, measures.perimeter_milli)
    } else if vertices.len() == 2 {
        (0, rounded_edge_milli(vertices[0], vertices[1]) * 2)
    } else {
        (0, 0)
    };

    Ok(Hull {
        vertices,
        source_indices,
        area2,
        perimeter_milli,
    })
}