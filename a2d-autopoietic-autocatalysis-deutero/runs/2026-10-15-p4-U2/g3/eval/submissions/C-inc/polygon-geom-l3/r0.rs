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
    let effective_len = if points.len() >= 2 && points[0] == points[points.len() - 1] {
        points.len() - 1
    } else {
        points.len()
    };
    if effective_len < 3 {
        return Err(GeomError::TooFewVertices { len: effective_len });
    }
    Ok(points[..effective_len].to_vec())
}

fn cross(a: Point, b: Point, c: Point) -> i128 {
    (b.x as i128 - a.x as i128) * (c.y as i128 - a.y as i128)
        - (b.y as i128 - a.y as i128) * (c.x as i128 - a.x as i128)
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

fn integer_sqrt(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    let mut x = value;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + value / x) / 2;
    }
    x
}

fn rounded_length_milli(a: Point, b: Point) -> u64 {
    let dx = b.x as i128 - a.x as i128;
    let dy = b.y as i128 - a.y as i128;
    let squared = (dx * dx + dy * dy) as u128;
    let scaled = squared * 1_000_000;
    let mut root = integer_sqrt(scaled);
    let remainder = scaled - root * root;
    if remainder * 4 >= root * 4 + 1 {
        root += 1;
    }
    root as u64
}

fn edge_lengths(points: &[Point]) -> (Vec<u64>, u64, Option<usize>) {
    let mut lengths = Vec::with_capacity(points.len());
    for i in 0..points.len() {
        lengths.push(rounded_length_milli(points[i], points[(i + 1) % points.len()]));
    }
    let perimeter = lengths.iter().copied().sum();
    let longest = (0..points.len()).max_by(|&i, &j| {
        lengths[i].cmp(&lengths[j]).then_with(|| {
            let pair_i = if points[i] <= points[(i + 1) % points.len()] {
                (points[i], points[(i + 1) % points.len()])
            } else {
                (points[(i + 1) % points.len()], points[i])
            };
            let pair_j = if points[j] <= points[(j + 1) % points.len()] {
                (points[j], points[(j + 1) % points.len()])
            } else {
                (points[(j + 1) % points.len()], points[j])
            };
            pair_j.cmp(&pair_i).then_with(|| j.cmp(&i))
        })
    });
    (lengths, perimeter, longest)
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare_polygon(points)?;
    let signed_area2 = shoelace(&polygon);
    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
    };
    let (edge_milli, perimeter_milli, longest_edge) = edge_lengths(&polygon);
    Ok(Measures {
        signed_area2,
        area2: signed_area2.unsigned_abs(),
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
    if queries.len() > MAX_INPUT {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (index, &query) in queries.iter().enumerate() {
        if !in_range(query) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    let mut result = Vec::with_capacity(queries.len());
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
        result.push(match boundary {
            Some(edge) => Location::Boundary { edge },
            None if inside => Location::Inside,
            None => Location::Outside,
        });
    }
    Ok(result)
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
        if !unique.iter().any(|(p, _)| *p == point) {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|(point, _)| *point);

    let mut hull: Vec<(Point, usize)> = if unique.len() <= 1 {
        unique
    } else {
        let all_collinear = unique
            .iter()
            .skip(2)
            .all(|(p, _)| cross(unique[0].0, unique[1].0, *p) == 0);
        if all_collinear {
            match mode {
                Collinear::Include => unique,
                Collinear::Exclude => vec![unique[0], *unique.last().unwrap()],
            }
        } else {
            let mut lower: Vec<(Point, usize)> = Vec::new();
            for &point in &unique {
                while lower.len() >= 2 {
                    let turn = cross(
                        lower[lower.len() - 2].0,
                        lower[lower.len() - 1].0,
                        point.0,
                    );
                    if turn > 0 || (mode == Collinear::Include && turn == 0) {
                        break;
                    }
                    lower.pop();
                }
                lower.push(point);
            }

            let mut upper: Vec<(Point, usize)> = Vec::new();
            for &point in unique.iter().rev() {
                while upper.len() >= 2 {
                    let turn = cross(
                        upper[upper.len() - 2].0,
                        upper[upper.len() - 1].0,
                        point.0,
                    );
                    if turn > 0 || (mode == Collinear::Include && turn == 0) {
                        break;
                    }
                    upper.pop();
                }
                upper.push(point);
            }

            lower.pop();
            upper.pop();
            lower.extend(upper);
            lower
        }
    };

    if hull.len() >= 3 && shoelace(&hull.iter().map(|v| v.0).collect::<Vec<_>>()) < 0 {
        hull.reverse();
    }

    let vertices: Vec<Point> = hull.iter().map(|v| v.0).collect();
    let source_indices: Vec<usize> = hull.iter().map(|v| v.1).collect();
    let area2 = if vertices.len() >= 3 {
        shoelace(&vertices).unsigned_abs()
    } else {
        0
    };
    let perimeter_milli = if vertices.len() <= 1 {
        0
    } else {
        edge_lengths(&vertices).1
    };

    Ok(Hull {
        vertices,
        source_indices,
        area2,
        perimeter_milli,
    })
}

fn main() {}