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

fn prepare(points: &[Point]) -> Result<&[Point], GeomError> {
    if points.len() > 4096 {
        return Err(GeomError::TooManyVertices { len: points.len() });
    }

    for (index, point) in points.iter().enumerate() {
        if !(-1_000_000_000..=1_000_000_000).contains(&point.x)
            || !(-1_000_000_000..=1_000_000_000).contains(&point.y)
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

fn rounded_sqrt(n: u128) -> u128 {
    // Find floor(sqrt(n)) using binary search with division to avoid overflow.
    let mut low = 0u128;
    let mut high = 1u128 << 64;

    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if mid <= n / mid {
            low = mid;
        } else {
            high = mid;
        }
    }

    let square = low * low;
    let remainder = n - square;
    if remainder >= low + 1 {
        low + 1
    } else {
        low
    }
}

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    let polygon = prepare(points)?;
    let n = polygon.len();

    let mut signed_area2 = 0i128;
    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];
        signed_area2 += (a.x as i128) * (b.y as i128) - (a.y as i128) * (b.x as i128);
    }

    let orientation = match signed_area2.cmp(&0) {
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Degenerate,
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
    };

    let mut edge_milli = Vec::with_capacity(n);
    let mut perimeter_milli = 0u64;
    let mut longest_edge = None;

    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];

        let dx = (a.x as i128 - b.x as i128).unsigned_abs();
        let dy = (a.y as i128 - b.y as i128).unsigned_abs();
        let d2 = dx * dx + dy * dy;
        let scaled = d2 * 1_000_000u128;
        let rounded = rounded_sqrt(scaled) as u64;

        edge_milli.push(rounded);
        perimeter_milli += rounded;

        let replace = match longest_edge {
            None => true,
            Some(best) => {
                if rounded != edge_milli[best] {
                    rounded > edge_milli[best]
                } else {
                    let (low, high) = if a <= b { (a, b) } else { (b, a) };
                    let best_a = polygon[best];
                    let best_b = polygon[(best + 1) % n];
                    let (best_low, best_high) = if best_a <= best_b {
                        (best_a, best_b)
                    } else {
                        (best_b, best_a)
                    };
                    (low, high, i) < (best_low, best_high, best)
                }
            }
        };

        if replace {
            longest_edge = Some(i);
        }
    }

    Ok(Measures {
        signed_area2,
        area2: signed_area2.unsigned_abs(),
        orientation,
        edge_milli,
        perimeter_milli,
        longest_edge,
    })
}