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
const INPUT_LIMIT: usize = 4096;

fn in_range(p: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&p.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&p.y)
}

fn prepare_polygon(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > INPUT_LIMIT {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
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

fn edge_mill(a: Point, b: Point) -> u64 {
    let dx = a.x as i128 - b.x as i128;
    let dy = a.y as i128 - b.y as i128;
    let d2 = (dx * dx + dy * dy) as u128;
    let scaled = d2 * 1_000_000;
    let mut root = scaled.isqrt();
    let remainder = scaled - root * root;
    if remainder * 4 >= root * 4 + 1 {
        root += 1;
    }
    root as u64
}

fn perimeter_and_edges(points: &[Point]) -> (Vec<u64>, u64) {
    if points.is_empty() {
        return (Vec::new(), 0);
    }
    let mut edges = Vec::with_capacity(points.len());
    let mut sum = 0u64;
    for i in 0..points.len() {
        let length = edge_mill(points[i], points[(i + 1) % points.len()]);
        edges.push(length);
        sum += length;
    }
    (edges, sum)
}

fn signed_area2(points: &[Point]) -> i128 {
    let mut area = 0i128;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        area += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }
    area
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare_polygon(points)?;
    let signed = signed_area2(&polygon);
    let (edge_milli, perimeter_milli) = perimeter_and_edges(&polygon);

    let longest_edge = (0..polygon.len()).max_by(|&i, &j| {
        edge_milli[i]
            .cmp(&edge_milli[j])
            .then_with(|| {
                let pair = |k: usize| {
                    let a = polygon[k];
                    let b = polygon[(k + 1) % polygon.len()];
                    if a <= b { (a, b) } else { (b, a) }
                };
                pair(j).cmp(&pair(i))
            })
            .then_with(|| j.cmp(&i))
    });

    let orientation = if signed > 0 {
        Orientation::CounterClockwise
    } else if signed < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
    };

    Ok(Measures {
        signed_area2: signed,
        area2: signed.unsigned_abs(),
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

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let polygon = prepare_polygon(points)?;
    if queries.len() > INPUT_LIMIT {
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
                let side = cross(a, b, query);
                if (b.y > a.y && side > 0) || (b.y < a.y && side < 0) {
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

fn hull_cross(a: Point, b: Point, mode: Collinear) -> bool {
    match mode {
        Collinear::Exclude => cross(a, b, Point { x: 0, y: 0 }) > 0,
        Collinear::Include => cross(a, b, Point { x: 0, y: 0 }) >= 0,
    }
}

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    if points.len() > INPUT_LIMIT {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (index, &point) in points.iter().enumerate() {
        if !unique.iter().any(|&(existing, _)| existing == point) {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|&(point, _)| point);

    let mut vertices: Vec<Point> = Vec::new();
    let mut source_indices = Vec::new();

    if unique.len() <= 2 {
        for &(point, index) in &unique {
            vertices.push(point);
            source_indices.push(index);
        }
    } else {
        let all_collinear = unique
            .iter()
            .skip(2)
            .all(|&(point, _)| cross(unique[0].0, unique[1].0, point) == 0);

        if all_collinear {
            let selected: Vec<(Point, usize)> = match mode {
                Collinear::Include => unique,
                Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
            };
            for (point, index) in selected {
                vertices.push(point);
                source_indices.push(index);
            }
        } else {
            let should_pop = |a: Point, b: Point, c: Point| match mode {
                Collinear::Exclude => cross(a, b, c) <= 0,
                Collinear::Include => cross(a, b, c) < 0,
            };

            let mut lower: Vec<(Point, usize)> = Vec::new();
            for &entry in &unique {
                while lower.len() >= 2
                    && should_pop(lower[lower.len() - 2].0, lower[lower.len() - 1].0, entry.0)
                {
                    lower.pop();
                }
                lower.push(entry);
            }

            let mut upper: Vec<(Point, usize)> = Vec::new();
            for &entry in unique.iter().rev() {
                while upper.len() >= 2
                    && should_pop(upper[upper.len() - 2].0, upper[upper.len() - 1].0, entry.0)
                {
                    upper.pop();
                }
                upper.push(entry);
            }

            lower.pop();
            upper.pop();
            lower.extend(upper);

            for (point, index) in lower {
                vertices.push(point);
                source_indices.push(index);
            }
        }
    }

    let area = signed_area2(&vertices).unsigned_abs();
    let (_, perimeter_milli) = perimeter_and_edges(&vertices);
    Ok(Hull {
        vertices,
        source_indices,
        area2: area,
        perimeter_milli,
    })
}

fn main() {}