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
const MAX_INPUT: usize = 4096;

fn in_range(p: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&p.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&p.y)
}

fn prepare_polygon(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > MAX_INPUT {
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

fn shoelace(points: &[Point]) -> i128 {
    if points.is_empty() {
        return 0;
    }
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
    let squared = (dx * dx + dy * dy) as u128;
    let scaled = squared * 1_000_000u128;
    let root = integer_sqrt(scaled);
    let lower_square = root * root;
    let upper = root + 1;
    let upper_square = upper * upper;
    let rounded = if scaled - lower_square >= upper_square - scaled {
        upper
    } else {
        root
    };
    rounded as u64
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

fn edge_key(a: Point, b: Point) -> (Point, Point) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn polygon_metrics(points: &[Point]) -> (Vec<u64>, u64, Option<usize>) {
    let n = points.len();
    let mut lengths = Vec::with_capacity(n);
    let mut perimeter = 0u64;
    for i in 0..n {
        let length = rounded_edge_milli(points[i], points[(i + 1) % n]);
        lengths.push(length);
        perimeter += length;
    }

    let mut longest = None;
    for i in 0..n {
        match longest {
            None => longest = Some(i),
            Some(best) => {
                if lengths[i] > lengths[best]
                    || (lengths[i] == lengths[best]
                        && (edge_key(points[i], points[(i + 1) % n])
                            < edge_key(points[best], points[(best + 1) % n])
                            || (edge_key(points[i], points[(i + 1) % n])
                                == edge_key(points[best], points[(best + 1) % n])
                                && i < best)))
                {
                    longest = Some(i);
                }
            }
        }
    }
    (lengths, perimeter, longest)
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare_polygon(points)?;
    let signed_area2 = shoelace(&polygon);
    let area2 = signed_area2.unsigned_abs();
    let orientation = if signed_area2 > 0 {
        Orientation::CounterClockwise
    } else if signed_area2 < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
    };
    let (edge_milli, perimeter_milli, longest_edge) = polygon_metrics(&polygon);
    Ok(Measures {
        signed_area2,
        area2,
        orientation,
        edge_milli,
        perimeter_milli,
        longest_edge,
    })
}

fn on_segment(a: Point, b: Point, q: Point) -> bool {
    cross(a, b, q) == 0
        && q.x >= a.x.min(b.x)
        && q.x <= a.x.max(b.x)
        && q.y >= a.y.min(b.y)
        && q.y <= a.y.max(b.y)
}

fn locate(polygon: &[Point], q: Point) -> Location {
    let n = polygon.len();
    let mut boundary_edge = None;
    for i in 0..n {
        if on_segment(polygon[i], polygon[(i + 1) % n], q) {
            boundary_edge = Some(i);
            break;
        }
    }
    if let Some(edge) = boundary_edge {
        return Location::Boundary { edge };
    }

    let mut inside = false;
    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];
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
}

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let polygon = prepare_polygon(points)?;
    if queries.len() > MAX_INPUT {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (index, &query) in queries.iter().enumerate() {
        if !in_range(query) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }
    Ok(queries.iter().map(|&q| locate(&polygon, q)).collect())
}

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    if points.len() > MAX_INPUT {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (index, &point) in points.iter().enumerate() {
        if let Some(entry) = unique.iter_mut().find(|(p, _)| *p == point) {
            entry.1 = entry.1.min(index);
        } else {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|entry| entry.0);

    let mut vertices_with_sources = if unique.len() <= 1 {
        unique.clone()
    } else {
        let all_collinear = unique
            .iter()
            .skip(2)
            .all(|entry| cross(unique[0].0, unique[1].0, entry.0) == 0);

        if all_collinear {
            match mode {
                Collinear::Include => unique.clone(),
                Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
            }
        } else {
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
            lower
        }
    };

    let vertices: Vec<Point> = vertices_with_sources.iter().map(|entry| entry.0).collect();
    let source_indices: Vec<usize> = vertices_with_sources.drain(..).map(|entry| entry.1).collect();
    let signed = shoelace(&vertices);
    let ( _, perimeter_milli, _) = polygon_metrics(&vertices);

    Ok(Hull {
        vertices,
        source_indices,
        area2: signed.unsigned_abs(),
        perimeter_milli,
    })
}

fn main() {}