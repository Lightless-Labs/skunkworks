#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point { pub x: i64, pub y: i64 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation { Clockwise, Degenerate, CounterClockwise }

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
pub enum Location { Outside, Inside, Boundary { edge: usize } }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collinear { Exclude, Include }

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

fn in_range(p: Point) -> bool {
    (-LIMIT..=LIMIT).contains(&p.x) && (-LIMIT..=LIMIT).contains(&p.y)
}

fn prepare(points: &[Point]) -> Result<Vec<Point>, GeomError> {
    if points.len() > 4096 {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (i, &p) in points.iter().enumerate() {
        if !in_range(p) { return Err(GeomError::OutOfRange { index: i }); }
    }
    let mut v = points.to_vec();
    if v.len() >= 2 && v[0] == *v.last().unwrap() { v.pop(); }
    if v.len() < 3 { return Err(GeomError::TooFewVertices { len: v.len() }); }
    Ok(v)
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

fn signed_area2(v: &[Point]) -> i128 {
    let mut s = 0i128;
    for i in 0..v.len() {
        let a = v[i];
        let b = v[(i + 1) % v.len()];
        s += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
    }
    s
}

fn isqrt(n: u128) -> u128 {
    if n < 2 { return n; }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

fn edge_milli(a: Point, b: Point) -> u64 {
    let dx = (b.x as i128 - a.x as i128) as i128;
    let dy = (b.y as i128 - a.y as i128) as i128;
    let d2 = (dx * dx + dy * dy) as u128;
    let scaled = d2 * 1_000_000u128;
    let r = isqrt(scaled);
    let threshold = 4 * scaled;
    let midpoint = 4 * r * r + 4 * r + 1;
    (if threshold >= midpoint { r + 1 } else { r }) as u64
}

fn measures_unchecked(v: &[Point]) -> Measures {
    let s = signed_area2(v);
    let mut lengths = Vec::with_capacity(v.len());
    let mut perimeter = 0u64;
    for i in 0..v.len() {
        let len = edge_milli(v[i], v[(i + 1) % v.len()]);
        lengths.push(len);
        perimeter += len;
    }
    let longest = if v.is_empty() {
        None
    } else {
        let mut best = 0usize;
        for i in 1..v.len() {
            if lengths[i] > lengths[best] {
                best = i;
            } else if lengths[i] == lengths[best] {
                let pair_i = if v[i] <= v[(i + 1) % v.len()] {
                    (v[i], v[(i + 1) % v.len()])
                } else { (v[(i + 1) % v.len()], v[i]) };
                let pair_b = if v[best] <= v[(best + 1) % v.len()] {
                    (v[best], v[(best + 1) % v.len()])
                } else { (v[(best + 1) % v.len()], v[best]) };
                if pair_i < pair_b || (pair_i == pair_b && i < best) { best = i; }
            }
        }
        Some(best)
    };
    Measures {
        signed_area2: s,
        area2: s.unsigned_abs(),
        orientation: if s > 0 { Orientation::CounterClockwise }
            else if s < 0 { Orientation::Clockwise } else { Orientation::Degenerate },
        edge_milli: lengths,
        perimeter_milli: perimeter,
        longest_edge: longest,
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let v = prepare(points)?;
    Ok(measures_unchecked(&v))
}

fn on_segment(a: Point, b: Point, q: Point) -> bool {
    orient(a, b, q) == 0
        && q.x >= a.x.min(b.x) && q.x <= a.x.max(b.x)
        && q.y >= a.y.min(b.y) && q.y <= a.y.max(b.y)
}

pub fn classify(points: &[Point], queries: &[Point]) -> Result<Vec<Location>, GeomError> {
    let v = prepare(points)?;
    if queries.len() > 4096 {
        return Err(GeomError::TooManyQueries { len: queries.len() });
    }
    for (i, &q) in queries.iter().enumerate() {
        if !in_range(q) { return Err(GeomError::QueryOutOfRange { index: i }); }
    }
    let mut out = Vec::with_capacity(queries.len());
    for &q in queries {
        let mut boundary = None;
        for i in 0..v.len() {
            if on_segment(v[i], v[(i + 1) % v.len()], q) {
                boundary = Some(i);
                break;
            }
        }
        if let Some(edge) = boundary {
            out.push(Location::Boundary { edge });
            continue;
        }
        let mut odd = false;
        for i in 0..v.len() {
            let a = v[i];
            let b = v[(i + 1) % v.len()];
            if (a.y > q.y) != (b.y > q.y) {
                let c = orient(a, b, q);
                if (b.y > a.y && c > 0) || (b.y < a.y && c < 0) {
                    odd = !odd;
                }
            }
        }
        out.push(if odd { Location::Inside } else { Location::Outside });
    }
    Ok(out)
}

pub fn convex_hull(points: &[Point], mode: Collinear) -> Result<Hull, GeomError> {
    if points.len() > 4096 { return Err(GeomError::TooManyVertices { len: points.len() }); }
    for (i, &p) in points.iter().enumerate() {
        if !in_range(p) { return Err(GeomError::OutOfRange { index: i }); }
    }
    let mut unique: Vec<(Point, usize)> = Vec::new();
    for (i, &p) in points.iter().enumerate() {
        if let Some(existing) = unique.iter_mut().find(|x| x.0 == p) {
            existing.1 = existing.1.min(i);
        } else {
            unique.push((p, i));
        }
    }
    unique.sort_by_key(|x| x.0);
    if unique.len() <= 1 {
        let vertices: Vec<Point> = unique.iter().map(|x| x.0).collect();
        let source_indices: Vec<usize> = unique.iter().map(|x| x.1).collect();
        return Ok(Hull { vertices, source_indices, area2: 0, perimeter_milli: 0 });
    }
    let all_collinear = unique.iter().all(|x| orient(unique[0].0, unique[1].0, x.0) == 0);
    let selected: Vec<(Point, usize)> = if all_collinear {
        if mode == Collinear::Include { unique.clone() }
        else { vec![unique[0], *unique.last().unwrap()] }
    } else {
        let mut lower: Vec<(Point, usize)> = Vec::new();
        for &pt in &unique {
            while lower.len() >= 2 {
                let c = orient(lower[lower.len()-2].0, lower[lower.len()-1].0, pt.0);
                if if mode == Collinear::Include { c < 0 } else { c <= 0 } { lower.pop(); } else { break; }
            }
            lower.push(pt);
        }
        let mut upper: Vec<(Point, usize)> = Vec::new();
        for &pt in unique.iter().rev() {
            while upper.len() >= 2 {
                let c = orient(upper[upper.len()-2].0, upper[upper.len()-1].0, pt.0);
                if if mode == Collinear::Include { c < 0 } else { c <= 0 } { upper.pop(); } else { break; }
            }
            upper.push(pt);
        }
        lower.pop();
        upper.pop();
        lower.extend(upper);
        lower
    };
    let vertices: Vec<Point> = selected.iter().map(|x| x.0).collect();
    let source_indices: Vec<usize> = selected.iter().map(|x| x.1).collect();
    let area2 = signed_area2(&vertices).unsigned_abs();
    let perimeter_milli = measures_unchecked(&vertices).perimeter_milli;
    Ok(Hull { vertices, source_indices, area2, perimeter_milli })
}

fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let o1 = orient(a, b, c);
    let o2 = orient(a, b, d);
    let o3 = orient(c, d, a);
    let o4 = orient(c, d, b);
    if o1 == 0 && on_segment(a, b, c) { return true; }
    if o2 == 0 && on_segment(a, b, d) { return true; }
    if o3 == 0 && on_segment(c, d, a) { return true; }
    if o4 == 0 && on_segment(c, d, b) { return true; }
    (o1 > 0) != (o2 > 0) && (o3 > 0) != (o4 > 0)
}

pub fn validate(points: &[Point]) -> Result<Validated, GeomError> {
    let v = prepare(points)?;
    let n = v.len();
    let mut duplicate: Option<(usize, usize)> = None;
    for i in 0..n {
        for j in i+1..n {
            if v[i] == v[j] {
                duplicate = Some((i, j));
                break;
            }
        }
        if duplicate.is_some() { break; }
    }
    if let Some((first, second)) = duplicate {
        return Err(GeomError::Duplicate { first, second });
    }
    let area = signed_area2(&v);
    if area == 0 { return Err(GeomError::ZeroArea); }
    for i in 0..n {
        let prev = v[(i+n-1)%n];
        let curr = v[i];
        let next = v[(i+1)%n];
        if orient(prev, curr, next) == 0 {
            let dot = (prev.x as i128 - curr.x as i128) * (next.x as i128 - curr.x as i128)
                + (prev.y as i128 - curr.y as i128) * (next.y as i128 - curr.y as i128);
            if dot > 0 { return Err(GeomError::AdjacentOverlap { vertex: i }); }
        }
    }
    for i in 0..n {
        for j in i+1..n {
            if j == i+1 || (i == 0 && j == n-1) { continue; }
            if segments_intersect(v[i], v[(i+1)%n], v[j], v[(j+1)%n]) {
                return Err(GeomError::Intersection { edge_a: i, edge_b: j });
            }
        }
    }
    let min_index = (0..n).min_by_key(|&i| v[i]).unwrap();
    let mut order = Vec::with_capacity(n);
    if area > 0 {
        for k in 0..n { order.push((min_index+k)%n); }
    } else {
        for k in 0..n { order.push((min_index+n-k)%n); }
    }
    let vertices: Vec<Point> = order.iter().map(|&i| v[i]).collect();
    let source_indices = order;
    let measures = measures_unchecked(&vertices);
    Ok(Validated { vertices, source_indices, measures })
}
