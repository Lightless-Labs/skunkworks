// Intentional fault: Does not strip an optional closing copy.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point { pub x: i64, pub y: i64 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation { Clockwise, Degenerate, CounterClockwise }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measures {
    pub signed_area2: i128, pub area2: u128, pub orientation: Orientation,
    pub edge_milli: Vec<u64>, pub perimeter_milli: u64,
    pub longest_edge: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeomError {
    TooManyVertices { len: usize },
    OutOfRange { index: usize },
    TooFewVertices { len: usize },
}

fn in_range(p: Point) -> bool { p.x.abs_diff(0) <= 1_000_000_000 && p.y.abs_diff(0) <= 1_000_000_000 }
fn scan(points: &[Point]) -> Result<(), GeomError> {
    if points.len() > 4096 { return Err(GeomError::TooManyVertices { len: points.len() }); }
    for (index, &p) in points.iter().enumerate() {
        if !in_range(p) { return Err(GeomError::OutOfRange { index }); }
    }
    Ok(())
}
fn polygon(points: &[Point]) -> Result<&[Point], GeomError> {
    scan(points)?;
    let n = points.len();
    let points = if false && n >= 2 && points[0] == points[n-1] { &points[..n-1] } else { points };
    if points.len() < 3 { return Err(GeomError::TooFewVertices { len: points.len() }); }
    Ok(points)
}
fn cross(a: Point, b: Point, c: Point) -> i128 {
    (b.x as i128-a.x as i128)*(c.y as i128-a.y as i128)
        -(b.y as i128-a.y as i128)*(c.x as i128-a.x as i128)
}
fn root(n: u128) -> u128 {
    let (mut lo, mut hi) = (0, n.min(1u128 << 64) + 1);
    while lo + 1 < hi {
        let m = (lo + hi)/2;
        if m <= n/m { lo = m; } else { hi = m; }
    }
    lo
}
fn length(a: Point, b: Point) -> u64 {
    let dx = a.x as i128-b.x as i128;
    let dy = a.y as i128-b.y as i128;
    let n = ((dx*dx+dy*dy) as u128)*1_000_000;
    let r = root(n);
    // Compare sqrt(n) with r+1/2 entirely in integer arithmetic.
    (if 4*n >= (2*r+1)*(2*r+1) { r+1 } else { r }) as u64
}
fn edge_key(points: &[Point], i: usize) -> (Point, Point, usize) {
    let a = points[i]; let b = points[(i+1)%points.len()];
    (a.min(b), a.max(b), i)
}
fn stats(points: &[Point]) -> Measures {
    let mut signed_area2 = 0i128;
    let mut edge_milli = Vec::new();
    let mut longest_edge = None;
    for i in 0..points.len() {
        let a=points[i]; let b=points[(i+1)%points.len()];
        signed_area2 += a.x as i128*b.y as i128 - a.y as i128*b.x as i128;
        let len = length(a,b);
        if let Some(j) = longest_edge {
            if len > edge_milli[j] || (len == edge_milli[j] && edge_key(points,i) < edge_key(points,j)) { longest_edge=Some(i); }
        } else { longest_edge=Some(i); }
        edge_milli.push(len);
    }
    Measures { signed_area2, area2: signed_area2.unsigned_abs(),
        orientation: if signed_area2>0 { Orientation::CounterClockwise } else if signed_area2<0 { Orientation::Clockwise } else { Orientation::Degenerate },
        perimeter_milli: edge_milli.iter().sum(), edge_milli, longest_edge }
}
pub fn measure(points: &[Point]) -> Result<Measures, GeomError> { Ok(stats(polygon(points)?)) }
