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

fn cross(a: Point, b: Point, c: Point) -> i128 {
    (b.x as i128 - a.x as i128) * (c.y as i128 - a.y as i128)
        - (b.y as i128 - a.y as i128) * (c.x as i128 - a.x as i128)
}

fn signed_area2(points: &[Point]) -> i128 {
    let mut sum = 0i128;
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
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

fn edge_length_milli(a: Point, b: Point) -> u64 {
    let dx = (a.x as i128 - b.x as i128) as i128;
    let dy = (a.y as i128 - b.y as i128) as i128;
    let squared = (dx * dx + dy * dy) as u128;
    let scaled = squared * 1_000_000;
    let root = isqrt(scaled);
    let remainder = scaled - root * root;
    let rounded = root + u128::from(remainder >= root + 1);
    rounded as u64
}

fn perimeter_milli(points: &[Point]) -> u64 {
    match points.len() {
        0 | 1 => 0,
        2 => edge_length_milli(points[0], points[1]) * 2,
        n => (0..n)
            .map(|i| edge_length_milli(points[i], points[(i + 1) % n]))
            .sum(),
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let points = prepare_polygon(points)?;
    let signed = signed_area2(points);

    let edge_milli: Vec<u64> = (0..points.len())
        .map(|i| edge_length_milli(points[i], points[(i + 1) % points.len()]))
        .collect();
    let perimeter_milli = edge_milli.iter().copied().sum();

    let longest_edge = (0..points.len()).max_by(|&i, &j| {
        edge_milli[i].cmp(&edge_milli[j]).then_with(|| {
            let (ilo, ihi) = if points[i] <= points[(i + 1) % points.len()] {
                (points[i], points[(i + 1) % points.len()])
            } else {
                (points[(i + 1) % points.len()], points[i])
            };
            let (jlo, jhi) = if points[j] <= points[(j + 1) % points.len()] {
                (points[j], points[(j + 1) % points.len()])
            } else {
                (points[(j + 1) % points.len()], points[j])
            };
            (jlo, jhi).cmp(&(ilo, ihi)).then_with(|| j.cmp(&i))
        })
    });

    let orientation = match signed.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
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

    if queries.len() > MAX_VERTICES {
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
        for i in 0..polygon.len() {
            if on_segment(polygon[i], polygon[(i + 1) % polygon.len()], query) {
                boundary = Some(i);
                break;
            }
        }
        if let Some(edge) = boundary {
            result.push(Location::Boundary { edge });
            continue;
        }

        let mut inside = false;
        for i in 0..polygon.len() {
            let a = polygon[i];
            let b = polygon[(i + 1) % polygon.len()];
            if (a.y > query.y) != (b.y > query.y) {
                let side = cross(a, b, query);
                if (b.y > a.y && side > 0) || (b.y < a.y && side < 0) {
                    inside = !inside;
                }
            }
        }
        result.push(if inside {
            Location::Inside
        } else {
            Location::Outside
        });
    }
    Ok(result)
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
        if !unique.iter().any(|&(existing, _)| existing == point) {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|&(point, _)| point);

    let mut hull: Vec<(Point, usize)> = if unique.len() <= 1 {
        unique.clone()
    } else {
        let all_collinear = unique
            .iter()
            .skip(2)
            .all(|&(p, _)| cross(unique[0].0, unique[1].0, p) == 0);

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
                    if if mode == Collinear::Include {
                        turn < 0
                    } else {
                        turn <= 0
                    } {
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
                    if if mode == Collinear::Include {
                        turn < 0
                    } else {
                        turn <= 0
                    } {
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
    };

    let vertices: Vec<Point> = hull.iter().map(|&(point, _)| point).collect();
    let source_indices: Vec<usize> = hull.iter().map(|&(_, index)| index).collect();

    let area = if vertices.len() >= 3 {
        signed_area2(&vertices).unsigned_abs()
    } else {
        0
    };

    Ok(Hull {
        perimeter_milli: perimeter_milli(&vertices),
        vertices,
        source_indices,
        area2: area,
    })
}