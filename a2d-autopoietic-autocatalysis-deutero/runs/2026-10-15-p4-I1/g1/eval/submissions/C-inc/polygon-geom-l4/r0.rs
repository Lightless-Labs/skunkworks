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

fn cross_vec(ax: i128, ay: i128, bx: i128, by: i128) -> i128 {
    ax * by - ay * bx
}

fn orient(a: Point, b: Point, c: Point) -> i128 {
    cross_vec(
        b.x as i128 - a.x as i128,
        b.y as i128 - a.y as i128,
        c.x as i128 - a.x as i128,
        c.y as i128 - a.y as i128,
    )
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
    let mut low = 1u128;
    let mut high = n.min(u64::MAX as u128);
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if mid <= n / mid {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}

fn edge_mill(a: Point, b: Point) -> u64 {
    let dx = (a.x as i128 - b.x as i128).unsigned_abs();
    let dy = (a.y as i128 - b.y as i128).unsigned_abs();
    let square = dx * dx + dy * dy;

    // Find the nearest integer to 1000 * sqrt(square) without forming
    // square * 1_000_000, which can exceed u128.
    let scaled_floor_root = isqrt(square) * 1000;
    let mut rounded = scaled_floor_root;
    let threshold = scaled_floor_root * 2 + 1;
    let lhs = square * 4_000_000;
    let rhs = threshold * threshold;
    if lhs >= rhs {
        rounded += 1;
    }
    rounded as u64
}

fn pair_key(a: Point, b: Point) -> (Point, Point) {
    if a <= b { (a, b) } else { (b, a) }
}

fn measures_for(points: &[Point]) -> Measures {
    let signed = signed_area2(points);
    let mut edge_milli = Vec::with_capacity(points.len());
    let mut perimeter = 0u64;
    let mut longest: Option<usize> = None;

    for i in 0..points.len() {
        let value = edge_mill(points[i], points[(i + 1) % points.len()]);
        edge_milli.push(value);
        perimeter += value;
        match longest {
            None => longest = Some(i),
            Some(j) => {
                let key_i = pair_key(points[i], points[(i + 1) % points.len()]);
                let key_j = pair_key(points[j], points[(j + 1) % points.len()]);
                if value > edge_milli[j]
                    || (value == edge_milli[j] && (key_i < key_j || (key_i == key_j && i < j)))
                {
                    longest = Some(i);
                }
            }
        }
    }

    Measures {
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
        perimeter_milli: perimeter,
        longest_edge: longest,
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let effective = prepare(points)?;
    Ok(measures_for(&effective))
}

fn on_segment(a: Point, b: Point, q: Point) -> bool {
    orient(a, b, q) == 0
        && q.x >= a.x.min(b.x)
        && q.x <= a.x.max(b.x)
        && q.y >= a.y.min(b.y)
        && q.y <= a.y.max(b.y)
}

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let polygon = prepare(points)?;
    if queries.len() > MAX_VERTICES {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (index, &query) in queries.iter().enumerate() {
        if !in_range(query) {
            return Err(GeomError::QueryOutOfRange { index });
        }
    }

    let mut result = Vec::with_capacity(queries.len());
    for &q in queries {
        let mut boundary = None;
        let mut inside = false;
        for i in 0..polygon.len() {
            let a = polygon[i];
            let b = polygon[(i + 1) % polygon.len()];
            if on_segment(a, b, q) {
                if boundary.is_none() {
                    boundary = Some(i);
                }
            } else if (a.y > q.y) != (b.y > q.y) {
                let c = orient(a, b, q);
                if (b.y > a.y && c > 0) || (b.y < a.y && c < 0) {
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

fn cloud(points: &[Point]) -> Result<Vec<(Point, usize)>, GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, &point) in points.iter().enumerate() {
        if !in_range(point) {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (i, &p) in points.iter().enumerate() {
        if !unique.iter().any(|(q, _)| *q == p) {
            unique.push((p, i));
        }
    }
    unique.sort_by_key(|(p, _)| *p);
    Ok(unique)
}

fn hull_area2(points: &[Point]) -> u128 {
    if points.len() < 3 {
        0
    } else {
        signed_area2(points).unsigned_abs()
    }
}

fn hull_perimeter(points: &[Point]) -> u64 {
    match points.len() {
        0 | 1 => 0,
        2 => edge_mill(points[0], points[1]) * 2,
        n => (0..n)
            .map(|i| edge_mill(points[i], points[(i + 1) % n]))
            .sum(),
    }
}

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    let unique = cloud(points)?;
    let pts: Vec<Point> = unique.iter().map(|x| x.0).collect();

    if pts.len() <= 1 {
        return Ok(Hull {
            vertices: pts,
            source_indices: unique.iter().map(|x| x.1).collect(),
            area2: 0,
            perimeter_milli: 0,
        });
    }

    let all_collinear =
        (2..pts.len()).all(|i| orient(pts[0], pts[1], pts[i]) == 0);
    let output = if all_collinear {
        match mode {
            Collinear::Include => pts.clone(),
            Collinear::Exclude => vec![pts[0], *pts.last().unwrap()],
        }
    } else {
        let mut lower: Vec<Point> = Vec::new();
        for &p in &pts {
            while lower.len() >= 2 {
                let turn = orient(lower[lower.len() - 2], lower[lower.len() - 1], p);
                if turn > 0 || (mode == Collinear::Include && turn == 0) {
                    break;
                }
                lower.pop();
            }
            lower.push(p);
        }

        let mut upper: Vec<Point> = Vec::new();
        for &p in pts.iter().rev() {
            while upper.len() >= 2 {
                let turn = orient(upper[upper.len() - 2], upper[upper.len() - 1], p);
                if turn > 0 || (mode == Collinear::Include && turn == 0) {
                    break;
                }
                upper.pop();
            }
            upper.push(p);
        }
        lower.pop();
        upper.pop();
        lower.extend(upper);
        lower
    };

    let source_indices: Vec<usize> = output
        .iter()
        .map(|p| unique.iter().find(|x| x.0 == *p).unwrap().1)
        .collect();

    Ok(Hull {
        area2: hull_area2(&output),
        perimeter_milli: hull_perimeter(&output),
        vertices: output,
        source_indices,
    })
}

fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let o1 = orient(a, b, c);
    let o2 = orient(a, b, d);
    let o3 = orient(c, d, a);
    let o4 = orient(c, d, b);

    if o1 == 0 && on_segment(a, b, c) {
        return true;
    }
    if o2 == 0 && on_segment(a, b, d) {
        return true;
    }
    if o3 == 0 && on_segment(c, d, a) {
        return true;
    }
    if o4 == 0 && on_segment(c, d, b) {
        return true;
    }
    (o1 > 0) != (o2 > 0) && (o3 > 0) != (o4 > 0)
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

    let signed = signed_area2(&polygon);
    if signed == 0 {
        return Err(GeomError::ZeroArea);
    }

    let n = polygon.len();
    for i in 0..n {
        let prev = polygon[(i + n - 1) % n];
        let cur = polygon[i];
        let next = polygon[(i + 1) % n];
        if orient(prev, cur, next) == 0 {
            let ax = prev.x as i128 - cur.x as i128;
            let ay = prev.y as i128 - cur.y as i128;
            let bx = next.x as i128 - cur.x as i128;
            let by = next.y as i128 - cur.y as i128;
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

    let start = (0..n).min_by_key(|&i| polygon[i]).unwrap();
    let mut vertices = Vec::with_capacity(n);
    let mut source_indices = Vec::with_capacity(n);
    if signed > 0 {
        for k in 0..n {
            let index = (start + k) % n;
            vertices.push(polygon[index]);
            source_indices.push(index);
        }
    } else {
        for k in 0..n {
            let index = (start + n - k) % n;
            vertices.push(polygon[index]);
            source_indices.push(index);
        }
    }

    let measures = measures_for(&vertices);
    Ok(Validated {
        vertices,
        source_indices,
        measures,
    })
}

fn main() {}