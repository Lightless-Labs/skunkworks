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
}

const MAX_VERTICES: usize = 4096;
const COORD_LIMIT: i64 = 1_000_000_000;

fn prepare(points: &[Point]) -> Result<&[Point], GeomError> {
    if points.len() > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }

    for (index, point) in points.iter().enumerate() {
        if point.x < -COORD_LIMIT
            || point.x > COORD_LIMIT
            || point.y < -COORD_LIMIT
            || point.y > COORD_LIMIT
        {
            return Err(GeomError::OutOfRange { index });
        }
    }

    let effective_len = if points.len() >= 2 && points.last() == points.first() {
        points.len() - 1
    } else {
        points.len()
    };

    if effective_len < 3 {
        return Err(GeomError::TooFewVertices { len: effective_len });
    }

    Ok(&points[..effective_len])
}

fn rounded_sqrt(value: u128) -> u128 {
    let mut low = 0u128;
    let mut high = value.min(u64::MAX as u128) + 1;

    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if mid <= value / mid {
            low = mid;
        } else {
            high = mid;
        }
    }

    let lower_square = low * low;
    let upper = low + 1;
    let upper_square = upper * upper;

    if value - lower_square >= upper_square - value {
        upper
    } else {
        low
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare(points)?;
    let n = polygon.len();

    let mut signed_area2 = 0i128;
    let mut edge_milli = Vec::with_capacity(n);
    let mut perimeter_milli = 0u64;
    let mut longest_edge: Option<usize> = None;

    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];

        signed_area2 += (a.x as i128) * (b.y as i128)
            - (a.y as i128) * (b.x as i128);

        let dx = (b.x as i128) - (a.x as i128);
        let dy = (b.y as i128) - (a.y as i128);
        let d2 = (dx * dx + dy * dy) as u128;
        let rounded = rounded_sqrt(d2 * 1_000_000) as u64;

        edge_milli.push(rounded);
        perimeter_milli += rounded;

        let replace = match longest_edge {
            None => true,
            Some(current) if rounded > edge_milli[current] => true,
            Some(current) if rounded < edge_milli[current] => false,
            Some(current) => {
                let endpoints = if a <= b { (a, b) } else { (b, a) };
                let ca = polygon[current];
                let cb = polygon[(current + 1) % n];
                let current_endpoints = if ca <= cb { (ca, cb) } else { (cb, ca) };
                endpoints < current_endpoints
                    || (endpoints == current_endpoints && i < current)
            }
        };
        if replace {
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