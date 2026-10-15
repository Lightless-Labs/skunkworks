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

fn in_range(point: Point) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&point.x)
        && (-COORD_LIMIT..=COORD_LIMIT).contains(&point.y)
}

fn prepare_polygon(points: &[Point]) -> Result<&[Point], GeomError> {
    if points.len() > INPUT_LIMIT {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let effective_len = if points.len() >= 2 && points.first() == points.last() {
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

fn distance2(a: Point, b: Point) -> u128 {
    let dx = (b.x as i128 - a.x as i128).unsigned_abs();
    let dy = (b.y as i128 - a.y as i128).unsigned_abs();
    dx * dx + dy * dy
}

fn rounded_length_milli(a: Point, b: Point) -> u64 {
    let scaled = distance2(a, b) * 1_000_000;
    let mut lo = 0u128;
    let mut hi = 3_000_000_000u128;

    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if mid * mid <= scaled {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }

    let remainder = scaled - lo * lo;
    if 4 * remainder >= (2 * lo + 1) * (2 * lo + 1) {
        (lo + 1) as u64
    } else {
        lo as u64
    }
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

fn perimeter_and_edges(points: &[Point]) -> (Vec<u64>, u64) {
    let mut edges = Vec::with_capacity(points.len());
    let mut perimeter = 0u64;
    for i in 0..points.len() {
        let length = rounded_length_milli(points[i], points[(i + 1) % points.len()]);
        edges.push(length);
        perimeter += length;
    }
    (edges, perimeter)
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let points = prepare_polygon(points)?;
    let signed = signed_area2(points);
    let orientation = if signed > 0 {
        Orientation::CounterClockwise
    } else if signed < 0 {
        Orientation::Clockwise
    } else {
        Orientation::Degenerate
    };
    let (edge_milli, perimeter_milli) = perimeter_and_edges(points);

    let mut longest_edge: Option<usize> = None;
    for i in 0..edge_milli.len() {
        let Some(current) = longest_edge else {
            longest_edge = Some(i);
            continue;
        };
        if edge_milli[i] > edge_milli[current] {
            longest_edge = Some(i);
        } else if edge_milli[i] == edge_milli[current] {
            let endpoint_pair = |index: usize| {
                let a = points[index];
                let b = points[(index + 1) % points.len()];
                if a <= b { (a, b) } else { (b, a) }
            };
            if endpoint_pair(i) < endpoint_pair(current)
                || (endpoint_pair(i) == endpoint_pair(current) && i < current)
            {
                longest_edge = Some(i);
            }
        }
    }

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

fn classify_one(points: &[Point], q: Point) -> Location {
    for i in 0..points.len() {
        if on_segment(points[i], points[(i + 1) % points.len()], q) {
            return Location::Boundary { edge: i };
        }
    }

    let mut inside = false;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        if (a.y > q.y) != (b.y > q.y) {
            let side = cross(a, b, q);
            if (b.y > a.y && side > 0) || (b.y < a.y && side < 0) {
                inside = !inside;
            }
        }
    }

    if inside { Location::Inside } else { Location::Outside }
}

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let points = prepare_polygon(points)?;
    if queries.len() > INPUT_LIMIT {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (index, &query) in queries.iter().enumerate() {
        if !in_range(query) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    Ok(queries
        .iter()
        .copied()
        .map(|query| classify_one(points, query))
        .collect())
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
        if !unique.iter().any(|(existing, _)| *existing == point) {
            unique.push((point, index));
        }
    }
    unique.sort_by_key(|&(point, _)| point);

    let n = unique.len();
    let (vertices, source_indices): (Vec<Point>, Vec<usize>) = if n <= 1 {
        (
            unique.iter().map(|&(point, _)| point).collect(),
            unique.iter().map(|&(_, index)| index).collect(),
        )
    } else {
        let all_collinear =
            (2..n).all(|i| cross(unique[0].0, unique[n - 1].0, unique[i].0) == 0);

        if all_collinear {
            match mode {
                Collinear::Include => (
                    unique.iter().map(|&(point, _)| point).collect(),
                    unique.iter().map(|&(_, index)| index).collect(),
                ),
                Collinear::Exclude => {
                    let selected = [unique[0], unique[n - 1]];
                    (
                        selected.iter().map(|&(point, _)| point).collect(),
                        selected.iter().map(|&(_, index)| index).collect(),
                    )
                }
            }
        } else {
            let retain_collinear = mode == Collinear::Include;
            let mut lower: Vec<usize> = Vec::new();
            for i in 0..n {
                while lower.len() >= 2 {
                    let a = unique[lower[lower.len() - 2]].0;
                    let b = unique[lower[lower.len() - 1]].0;
                    let turn = cross(a, b, unique[i].0);
                    if if retain_collinear { turn < 0 } else { turn <= 0 } {
                        lower.pop();
                    } else {
                        break;
                    }
                }
                lower.push(i);
            }

            let mut upper: Vec<usize> = Vec::new();
            for i in (0..n).rev() {
                while upper.len() >= 2 {
                    let a = unique[upper[upper.len() - 2]].0;
                    let b = unique[upper[upper.len() - 1]].0;
                    let turn = cross(a, b, unique[i].0);
                    if if retain_collinear { turn < 0 } else { turn <= 0 } {
                        upper.pop();
                    } else {
                        break;
                    }
                }
                upper.push(i);
            }

            lower.pop();
            upper.pop();
            lower.extend(upper);
            (
                lower.iter().map(|&i| unique[i].0).collect(),
                lower.iter().map(|&i| unique[i].1).collect(),
            )
        }
    };

    let area2 = if vertices.len() >= 3 {
        signed_area2(&vertices).unsigned_abs()
    } else {
        0
    };
    let (_, perimeter_milli) = perimeter_and_edges(&vertices);

    Ok(Hull {
        vertices,
        source_indices,
        area2,
        perimeter_milli,
    })
}

fn main() {}