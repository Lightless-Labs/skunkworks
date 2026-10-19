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

pub fn measure(points: &[Point]) -> Result<Measures, GeomError> {
    const MAX_VERTICES: usize = 4096;
    const COORD_LIMIT: i64 = 1_000_000_000;

    let raw_len = points.len();
    if raw_len > MAX_VERTICES {
        return Err(GeomError::TooManyVertices { len: raw_len });
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

    let effective_len = if raw_len >= 2 && points[0] == points[raw_len - 1] {
        raw_len - 1
    } else {
        raw_len
    };

    if effective_len < 3 {
        return Err(GeomError::TooFewVertices { len: effective_len });
    }

    let vertices = &points[..effective_len];
    let mut signed_area2 = 0_i128;
    let mut edge_milli = Vec::with_capacity(effective_len);
    let mut perimeter_milli = 0_u64;
    let mut longest_edge: Option<usize> = None;

    for index in 0..effective_len {
        let a = vertices[index];
        let b = vertices[(index + 1) % effective_len];

        signed_area2 += i128::from(a.x) * i128::from(b.y)
            - i128::from(a.y) * i128::from(b.x);

        let dx = i128::from(b.x) - i128::from(a.x);
        let dy = i128::from(b.y) - i128::from(a.y);
        let d2 = (dx * dx + dy * dy) as u128;
        let scaled = d2 * 1_000_000;
        let rounded = rounded_sqrt(scaled);
        let length = rounded as u64;

        edge_milli.push(length);
        perimeter_milli += length;

        match longest_edge {
            None => longest_edge = Some(index),
            Some(current) => {
                let current_length = edge_milli[current];
                if length > current_length
                    || (length == current_length
                        && edge_tie_key(a, b, index)
                            < edge_tie_key(
                                vertices[current],
                                vertices[(current + 1) % effective_len],
                                current,
                            ))
                {
                    longest_edge = Some(index);
                }
            }
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

fn rounded_sqrt(value: u128) -> u128 {
    let floor = integer_sqrt(value);
    let threshold = 2 * floor + 1;
    if 4 * value >= threshold * threshold {
        floor + 1
    } else {
        floor
    }
}

fn integer_sqrt(value: u128) -> u128 {
    if value < 2 {
        return value;
    }

    let mut low = 1_u128;
    let mut high = value.min(u64::MAX as u128) + 1;

    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if mid <= value / mid {
            low = mid;
        } else {
            high = mid;
        }
    }

    low
}

fn edge_tie_key(a: Point, b: Point, index: usize) -> (Point, Point, usize) {
    if a <= b {
        (a, b, index)
    } else {
        (b, a, index)
    }
}