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
}

const LIMIT: i64 = 1_000_000_000;
const MAX_VERTICES: usize = 4096;

fn prepare(points: &[Point]) -> Result<&[Point], GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }
    for (index, p) in points.iter().enumerate() {
        if !(-LIMIT..=LIMIT).contains(&p.x) || !(-LIMIT..=LIMIT).contains(&p.y) {
            return Err(GeomError::OutOfRange { index });
        }
    }
    let len = if points.len() >= 2 && points[0] == points[points.len() - 1] {
        points.len() - 1
    } else {
        points.len()
    };
    if len < 3 {
        return Err(GeomError::TooFewVertices { len });
    }
    Ok(&points[..len])
}

fn rounded_length(d2: u128) -> u64 {
    let target = d2 * 1_000_000;
    let mut low = 0u128;
    let mut high = target + 1;
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if mid == 0 || mid <= target / mid {
            low = mid;
        } else {
            high = mid;
        }
    }
    let floor = low;
    // Compare sqrt(target) with floor + 1/2 without floating point:
    // target >= floor^2 + floor + 1/4; since target is integer this is
    // equivalent to target - floor^2 >= floor + 1.
    if target - floor * floor >= floor + 1 {
        (floor + 1) as u64
    } else {
        floor as u64
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare(points)?;
    let n = polygon.len();
    let mut signed_area2 = 0i128;
    let mut edge_milli = Vec::with_capacity(n);
    let mut perimeter_milli = 0u64;

    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];
        signed_area2 += a.x as i128 * b.y as i128 - a.y as i128 * b.x as i128;
        let dx = b.x as i128 - a.x as i128;
        let dy = b.y as i128 - a.y as i128;
        let d2 = (dx * dx + dy * dy) as u128;
        let len = rounded_length(d2);
        edge_milli.push(len);
        perimeter_milli += len;
    }

    let mut longest_edge = None;
    for i in 0..n {
        let Some(best) = longest_edge else {
            longest_edge = Some(i);
            continue;
        };
        let a = polygon[i];
        let b = polygon[(i + 1) % n];
        let (low, high) = if a <= b { (a, b) } else { (b, a) };

        let c = polygon[best];
        let d = polygon[(best + 1) % n];
        let (best_low, best_high) = if c <= d { (c, d) } else { (d, c) };

        if edge_milli[i] > edge_milli[best]
            || (edge_milli[i] == edge_milli[best]
                && (low, high, i) < (best_low, best_high, best))
        {
            longest_edge = Some(i);
        }
    }

    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
    };

    Ok(Measures {
        signed_area2,
        area2: signed_area2.unsigned_abs(),
        orientation,
        edge_milli,
        perimeter_milli,
        longest_edge,
    })
}
