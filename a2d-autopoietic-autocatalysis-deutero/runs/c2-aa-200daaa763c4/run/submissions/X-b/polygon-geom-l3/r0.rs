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
const MAX_VERTICES: usize = 4096;

fn in_range(p: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&p.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&p.y)
}

fn prepare_polygon(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > MAX_VERTICES {
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
    (b.x as i128 - a.x as i128) * (c.y as i128 - a.y as i128)
        - (b.y as i128 - a.y as i128) * (c.x as i128 - a.x as i128)
}

fn squared_distance(a: Point, b: Point) -> u128 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    (dx * dx + dy * dy) as u128
}

fn integer_sqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = 1u128 << ((128 - n.leading_zeros() as usize + 1) / 2);
    loop {
        let next = (x + n / x) / 2;
        if next >= x {
            return x;
        }
        x = next;
    }
}

fn edge_length_milli(a: Point, b: Point) -> u64 {
    let scaled_squared = squared_distance(a, b) * 1_000_000;
    let floor = integer_sqrt(scaled_squared);
    let midpoint_test = 4 * scaled_squared;
    let threshold = 4 * floor * floor + 4 * floor + 1;
    (floor + u128::from(midpoint_test >= threshold)) as u64
}

fn perimeter_and_edges(vertices: &[Point]) -> (Vec<u64>, u64) {
    if vertices.is_empty() {
        return (Vec::new(), 0);
    }
    let mut edges = Vec::with_capacity(vertices.len());
    let mut perimeter = 0u64;
    for i in 0..vertices.len() {
        let length = edge_length_milli(vertices[i], vertices[(i + 1) % vertices.len()]);
        edges.push(length);
        perimeter += length;
    }
    (edges, perimeter)
}

fn signed_area2(vertices: &[Point]) -> i128 {
    let mut area = 0i128;
    for i in 0..vertices.len() {
        let a = vertices[i];
        let b = vertices[(i + 1) % vertices.len()];
        area += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }
    area
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let vertices = prepare_polygon(points)?;
    let signed = signed_area2(&vertices);
    let (edge_milli, perimeter_milli) = perimeter_and_edges(&vertices);

    let longest_edge = (0..edge_milli.len()).max_by(|&i, &j| {
        edge_milli[i].cmp(&edge_milli[j]).then_with(|| {
            let pair = |k: usize| {
                let a = vertices[k];
                let b = vertices[(k + 1) % vertices.len()];
                if a <= b { (a, b) } else { (b, a) }
            };
            pair(j).cmp(&pair(i)).then_with(|| j.cmp(&i))
        })
    });

    Ok(Measures {
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
    let vertices = prepare_polygon(points)?;
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
        for i in 0..vertices.len() {
            if on_segment(vertices[i], vertices[(i + 1) % vertices.len()], query) {
                boundary = Some(i);
                break;
            }
        }
        if let Some(edge) = boundary {
            results.push(Location::Boundary { edge });
            continue;
        }

        let mut inside = false;
        for i in 0..vertices.len() {
            let a = vertices[i];
            let b = vertices[(i + 1) % vertices.len()];
            if (a.y > query.y) != (b.y > query.y) {
                let side = cross(a, b, query);
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

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut indexed = points
        .iter()
        .copied()
        .enumerate()
        .map(|(index, point)| (point, index))
        .collect::<Vec<_>>();
    indexed.sort_by_key(|&(point, index)| (point, index));
    indexed.dedup_by(|a, b| a.0 == b.0);

    let distinct = indexed.len();
    let mut hull_indices: Vec<usize> = Vec::new();

    if distinct <= 1 {
        hull_indices = (0..distinct).collect();
    } else {
        let all_collinear = indexed[2..]
            .iter()
            .all(|entry| cross(indexed[0].0, indexed[distinct - 1].0, entry.0) == 0);

        if all_collinear {
            hull_indices = match mode {
                Collinear::Include => (0..distinct).collect(),
                Collinear::Exclude => vec![0, distinct - 1],
            };
        } else {
            let mut lower: Vec<usize> = Vec::new();
            for i in 0..distinct {
                while lower.len() >= 2 {
                    let n = lower.len();
                    let turn = cross(indexed[lower[n - 2]].0, indexed[lower[n - 1]].0, indexed[i].0);
                    if turn > 0 || (mode == Collinear::Include && turn == 0) {
                        break;
                    }
                    lower.pop();
                }
                lower.push(i);
            }

            let mut upper: Vec<usize> = Vec::new();
            for i in (0..distinct).rev() {
                while upper.len() >= 2 {
                    let n = upper.len();
                    let turn = cross(indexed[upper[n - 2]].0, indexed[upper[n - 1]].0, indexed[i].0);
                    if turn > 0 || (mode == Collinear::Include && turn == 0) {
                        break;
                    }
                    upper.pop();
                }
                upper.push(i);
            }

            lower.pop();
            upper.pop();
            lower.extend(upper);
            hull_indices = lower;
        }
    }

    let vertices = hull_indices.iter().map(|&i| indexed[i].0).collect::<Vec<_>>();
    let source_indices = hull_indices
        .iter()
        .map(|&i| indexed[i].1)
        .collect::<Vec<_>>();
    let area = if vertices.len() >= 3 {
        signed_area2(&vertices).unsigned_abs()
    } else {
        0
    };
    let (_, perimeter_milli) = perimeter_and_edges(&vertices);

    Ok(Hull {
        vertices,
        source_indices,
        area2: area,
        perimeter_milli,
    })
}