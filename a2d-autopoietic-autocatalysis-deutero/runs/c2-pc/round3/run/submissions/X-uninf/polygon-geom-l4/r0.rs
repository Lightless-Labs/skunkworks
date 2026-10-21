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

fn cross(a: Point, b: Point, q: Point) -> i128 {
    (b.x as i128 - a.x as i128) * (q.y as i128 - a.y as i128)
        - (b.y as i128 - a.y as i128) * (q.x as i128 - a.x as i128)
}

fn shoelace(points: &[Point]) -> i128 {
    let mut sum = 0_i128;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }
    sum
}

fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = 1_u128 << ((128 - n.leading_zeros() as usize + 1) / 2);
    loop {
        let y = (x + n / x) / 2;
        if y >= x {
            return x;
        }
        x = y;
    }
}

fn edge_length_milli(a: Point, b: Point) -> u64 {
    let dx = a.x as i128 - b.x as i128;
    let dy = a.y as i128 - b.y as i128;
    let squared = (dx * dx + dy * dy) as u128 * 1_000_000;
    let root = isqrt(squared);
    let rounded = if squared - root * root >= root + 1 {
        root + 1
    } else {
        root
    };
    rounded as u64
}

fn measures_for(points: &[Point]) -> Measures {
    let signed_area2 = shoelace(points);
    let area2 = signed_area2.unsigned_abs();
    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
    };

    let mut edge_milli = Vec::with_capacity(points.len());
    let mut perimeter_milli = 0_u64;
    for i in 0..points.len() {
        let length = edge_length_milli(points[i], points[(i + 1) % points.len()]);
        edge_milli.push(length);
        perimeter_milli += length;
    }

    let longest_edge = (0..points.len()).max_by(|&i, &j| {
        edge_milli[i].cmp(&edge_milli[j]).then_with(|| {
            let pair_i = ordered_pair(points[i], points[(i + 1) % points.len()]);
            let pair_j = ordered_pair(points[j], points[(j + 1) % points.len()]);
            pair_j.cmp(&pair_i).then_with(|| j.cmp(&i))
        })
    });

    Measures {
        signed_area2,
        area2,
        orientation,
        edge_milli,
        perimeter_milli,
        longest_edge,
    }
}

fn ordered_pair(a: Point, b: Point) -> (Point, Point) {
    if a <= b { (a, b) } else { (b, a) }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let effective = prepare(points)?;
    Ok(measures_for(&effective))
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

    Ok(queries
        .iter()
        .map(|&q| {
            let mut owner = None;
            for i in 0..polygon.len() {
                let a = polygon[i];
                let b = polygon[(i + 1) % polygon.len()];
                if cross(a, b, q) == 0
                    && q.x >= a.x.min(b.x)
                    && q.x <= a.x.max(b.x)
                    && q.y >= a.y.min(b.y)
                    && q.y <= a.y.max(b.y)
                {
                    owner = Some(i);
                    break;
                }
            }
            if let Some(edge) = owner {
                return Location::Boundary { edge };
            }

            let mut inside = false;
            for i in 0..polygon.len() {
                let a = polygon[i];
                let b = polygon[(i + 1) % polygon.len()];
                if (a.y > q.y) != (b.y > q.y) {
                    let c = cross(a, b, q);
                    if (b.y > a.y && c > 0) || (b.y < a.y && c < 0) {
                        inside = !inside;
                    }
                }
            }
            if inside { Location::Inside } else { Location::Outside }
        })
        .collect())
}

fn point_on_segment(p: Point, a: Point, b: Point) -> bool {
    cross(a, b, p) == 0
        && p.x >= a.x.min(b.x)
        && p.x <= a.x.max(b.x)
        && p.y >= a.y.min(b.y)
        && p.y <= a.y.max(b.y)
}

fn orient(a: Point, b: Point, c: Point) -> i128 {
    cross(a, b, c)
}

fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let o1 = orient(a, b, c);
    let o2 = orient(a, b, d);
    let o3 = orient(c, d, a);
    let o4 = orient(c, d, b);
    if o1 == 0 && point_on_segment(c, a, b) {
        return true;
    }
    if o2 == 0 && point_on_segment(d, a, b) {
        return true;
    }
    if o3 == 0 && point_on_segment(a, c, d) {
        return true;
    }
    if o4 == 0 && point_on_segment(b, c, d) {
        return true;
    }
    (o1 < 0) != (o2 < 0) && (o3 < 0) != (o4 < 0)
}

pub fn validate(points: &[Point]) -> Result<Validated, GeomError> {
    let polygon = prepare(points)?;

    for i in 0..polygon.len() {
        for j in i + 1..polygon.len() {
            if polygon[i] == polygon[j] {
                return Err(GeomError::Duplicate {
                    first: i,
                    second: j,
                });
            }
        }
    }

    let signed = shoelace(&polygon);
    if signed == 0 {
        return Err(GeomError::ZeroArea);
    }

    let n = polygon.len();
    for i in 0..n {
        let prev = polygon[(i + n - 1) % n];
        let current = polygon[i];
        let next = polygon[(i + 1) % n];
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
        for j in i + 1..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            if segments_intersect(
                polygon[i],
                polygon[(i + 1) % n],
                polygon[j],
                polygon[(j + 1) % n],
            ) {
                return Err(GeomError::Intersection {
                    edge_a: i,
                    edge_b: j,
                });
            }
        }
    }

    let min_index = (0..n).min_by_key(|&i| polygon[i]).unwrap();
    let forward = signed > 0;
    let mut vertices = Vec::with_capacity(n);
    let mut source_indices = Vec::with_capacity(n);
    for k in 0..n {
        let index = if forward {
            (min_index + k) % n
        } else {
            (min_index + n - (k % n)) % n
        };
        vertices.push(polygon[index]);
        source_indices.push(index);
    }
    let measures = measures_for(&vertices);
    Ok(Validated {
        vertices,
        source_indices,
        measures,
    })
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
        if let Some(entry) = unique.iter_mut().find(|entry| entry.0 == p) {
            entry.1 = entry.1.min(index);
        } else {
            unique.push((p, index));
        }
    }
    unique.sort_by_key(|entry| entry.0);

    let mut vertices_with_source: Vec<(Point, usize)> = if unique.len() <= 2 {
        unique
    } else {
        let all_collinear = unique
            .iter()
            .skip(2)
            .all(|entry| cross(unique[0].0, unique[1].0, entry.0) == 0);

        if all_collinear {
            match mode {
                Collinear::Include => unique,
                Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
            }
        } else {
            let mut lower: Vec<(Point, usize)> = Vec::new();
            for &entry in &unique {
                while lower.len() >= 2 {
                    let c = cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, entry.0);
                    if if mode == Collinear::Include { c < 0 } else { c <= 0 } {
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
                    let c = cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, entry.0);
                    if if mode == Collinear::Include { c < 0 } else { c <= 0 } {
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

    if vertices_with_source.len() > 1
        && vertices_with_source.first().unwrap().0 == vertices_with_source.last().unwrap().0
    {
        vertices_with_source.pop();
    }

    let vertices: Vec<Point> = vertices_with_source.iter().map(|entry| entry.0).collect();
    let source_indices: Vec<usize> = vertices_with_source.iter().map(|entry| entry.1).collect();
    let area2 = if vertices.len() >= 3 {
        shoelace(&vertices).unsigned_abs()
    } else {
        0
    };
    let perimeter_milli = if vertices.is_empty() {
        0
    } else if vertices.len() == 1 {
        0
    } else {
        (0..vertices.len())
            .map(|i| edge_length_milli(vertices[i], vertices[(i + 1) % vertices.len()]))
            .sum()
    };

    Ok(Hull {
        vertices,
        source_indices,
        area2,
        perimeter_milli,
    })
}