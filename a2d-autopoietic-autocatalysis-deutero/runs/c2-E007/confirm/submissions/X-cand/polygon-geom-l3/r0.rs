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

const MAX_VERTICES: usize = 4096;
const COORD_LIMIT: i64 = 1_000_000_000;

fn in_range(p: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&p.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&p.y)
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

fn cross(a: Point, b: Point, q: Point) -> i128 {
    (b.x as i128 - a.x as i128) * (q.y as i128 - a.y as i128)
        - (b.y as i128 - a.y as i128) * (q.x as i128 - a.x as i128)
}

fn squared_distance(a: Point, b: Point) -> u128 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    (dx * dx + dy * dy) as u128
}

fn rounded_sqrt(n: u128) -> u128 {
    let mut low = 0u128;
    let mut high = 1u128;

    while high.saturating_mul(high) < n {
        high *= 2;
    }

    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if mid * mid <= n {
            low = mid;
        } else {
            high = mid;
        }
    }

    let lower_square = low * low;
    let upper_square = high * high;
    if n - lower_square >= upper_square - n {
        high
    } else {
        low
    }
}

fn edge_length_milli(a: Point, b: Point) -> u64 {
    rounded_sqrt(squared_distance(a, b) * 1_000_000) as u64
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

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare_polygon(points)?;
    let signed_area2 = shoelace(polygon);
    let area2 = signed_area2.unsigned_abs();
    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
    };

    let mut edge_milli = Vec::with_capacity(polygon.len());
    let mut perimeter_milli = 0u64;
    let mut longest_edge = None;

    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        let length = edge_length_milli(a, b);
        edge_milli.push(length);
        perimeter_milli += length;

        let should_replace = match longest_edge {
            None => true,
            Some(current) => {
                let current_length = edge_milli[current];
                if length != current_length {
                    length > current_length
                } else {
                    let (low, high) = if a <= b { (a, b) } else { (b, a) };
                    let ca = polygon[current];
                    let cb = polygon[(current + 1) % polygon.len()];
                    let (clow, chigh) = if ca <= cb { (ca, cb) } else { (cb, ca) };
                    (low, high, i) < (clow, chigh, current)
                }
            }
        };
        if should_replace {
            longest_edge = Some(i);
        }
    }

    Ok(Measures {
        signed_area2,
        area2,
        orientation,
        edge_milli,
        perimeter_milli,
        longest_edge,
    })
}

fn point_on_segment(q: Point, a: Point, b: Point) -> bool {
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

            if point_on_segment(query, a, b) {
                boundary = Some(i);
                break;
            }

            if (a.y > query.y) != (b.y > query.y) {
                let side = cross(a, b, query);
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
        if !unique.iter().any(|(existing, _)| *existing == point) {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|&(point, _)| point);

    let (vertices_with_indices, all_collinear) = match unique.len() {
        0 => (Vec::new(), true),
        1 => (unique.clone(), true),
        2 => (unique.clone(), true),
        _ => {
            let mut collinear = true;
            for i in 2..unique.len() {
                if cross(unique[0].0, unique[1].0, unique[i].0) != 0 {
                    collinear = false;
                    break;
                }
            }

            if collinear {
                let result = match mode {
                    Collinear::Include => unique.clone(),
                    Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
                };
                (result, true)
            } else {
                let mut lower: Vec<(Point, usize)> = Vec::new();
                for &point in &unique {
                    while lower.len() >= 2 {
                        let turn = cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, point.0);
                        let remove = match mode {
                            Collinear::Exclude => turn <= 0,
                            Collinear::Include => turn < 0,
                        };
                        if remove {
                            lower.pop();
                        } else {
                            break;
                        }
                    }
                    lower.push(point);
                }

                let mut upper: Vec<(Point, usize)> = Vec::new();
                for &point in unique.iter().rev() {
                    while upper.len() >= 2 {
                        let turn = cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, point.0);
                        let remove = match mode {
                            Collinear::Exclude => turn <= 0,
                            Collinear::Include => turn < 0,
                        };
                        if remove {
                            upper.pop();
                        } else {
                            break;
                        }
                    }
                    upper.push(point);
                }

                lower.pop();
                upper.pop();
                lower.extend(upper);
                (lower, false)
            }
        }
    };

    let vertices: Vec<Point> = vertices_with_indices.iter().map(|&(point, _)| point).collect();
    let source_indices: Vec<usize> = vertices_with_indices.iter().map(|&(_, index)| index).collect();

    let signed_area = if vertices.len() >= 3 {
        shoelace(&vertices)
    } else {
        0
    };
    let area2 = signed_area.unsigned_abs();

    let mut perimeter_milli = 0u64;
    if vertices.len() >= 2 {
        for i in 0..vertices.len() {
            perimeter_milli += edge_length_milli(vertices[i], vertices[(i + 1) % vertices.len()]);
        }
    }

    let _ = all_collinear;

    Ok(Hull {
        vertices,
        source_indices,
        area2,
        perimeter_milli,
    })
}