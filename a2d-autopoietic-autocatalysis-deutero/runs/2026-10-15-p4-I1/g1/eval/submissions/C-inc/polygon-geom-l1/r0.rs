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

    let vertices = &points[..effective_len];
    let mut signed_area2 = 0_i128;
    let mut edge_milli = Vec::with_capacity(effective_len);

    for i in 0..effective_len {
        let a = vertices[i];
        let b = vertices[(i + 1) % effective_len];

        signed_area2 += i128::from(a.x) * i128::from(b.y)
            - i128::from(a.y) * i128::from(b.x);

        let dx = i128::from(b.x) - i128::from(a.x);
        let dy = i128::from(b.y) - i128::from(a.y);
        let d2 = (dx * dx + dy * dy) as u128;

        // Find the floor of sqrt(d2 * 1_000_000), then round to nearest.
        let scaled = d2 * 1_000_000;
        let mut low = 0_u128;
        let mut high = 2_000_000_001_u128;
        while low + 1 < high {
            let mid = low + (high - low) / 2;
            if mid * mid <= scaled {
                low = mid;
            } else {
                high = mid;
            }
        }

        let rounded = if scaled - low * low >= low + 1 {
            low + 1
        } else {
            low
        };
        edge_milli.push(rounded as u64);
    }

    let perimeter_milli = edge_milli.iter().copied().sum();
    let longest_edge = (0..effective_len).max_by(|&i, &j| {
        let length_order = edge_milli[i].cmp(&edge_milli[j]);
        if length_order != std::cmp::Ordering::Equal {
            return length_order;
        }

        let endpoints = |index: usize| {
            let a = vertices[index];
            let b = vertices[(index + 1) % effective_len];
            if a <= b { (a, b) } else { (b, a) }
        };

        endpoints(j)
            .cmp(&endpoints(i))
            .then_with(|| j.cmp(&i))
    });

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

fn main() {}