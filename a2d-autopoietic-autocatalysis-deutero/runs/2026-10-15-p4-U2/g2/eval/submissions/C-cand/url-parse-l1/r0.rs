#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub scheme: String,
    pub userinfo: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    pub path: String,
    pub query: Option<String>,
    pub fragment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    InvalidByte { at: usize },
    InvalidScheme { at: usize },
    MissingAuthority { at: usize },
    InvalidUserInfo { at: usize },
    InvalidHost { at: usize },
    InvalidPort { at: usize },
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    let bytes = input.as_bytes();

    if bytes.is_empty() {
        return Err(ParseError::Empty);
    }

    for (at, &byte) in bytes.iter().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(ParseError::InvalidByte { at });
        }
    }

    let colon = match bytes.iter().position(|&b| b == b':') {
        Some(at) => at,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };

    if colon == 0 || !bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }

    for (at, &byte) in bytes[..colon].iter().enumerate().skip(1) {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at });
        }
    }

    if bytes.get(colon + 1) != Some(&b'/') || bytes.get(colon + 2) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map_or(bytes.len(), |offset| authority_start + offset);

    let authority = &input[authority_start..authority_end];
    let authority_bytes = authority.as_bytes();

    let at_positions: Vec<usize> = authority_bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i))
        .collect();

    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }

    let (userinfo, host_start_rel) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
            });
        }
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let host_port = &authority[host_start_rel..];
    let host_port_bytes = host_port.as_bytes();
    let host_port_colon = host_port_bytes.iter().position(|&b| b == b':');

    let host_end_rel = host_port_colon.unwrap_or(host_port.len());
    let host = &host_port[..host_end_rel];
    let host_start = authority_start + host_start_rel;

    if host.is_empty() {
        return Err(ParseError::InvalidHost { at: host_start });
    }

    for (i, &byte) in host.as_bytes().iter().enumerate() {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
            return Err(ParseError::InvalidHost {
                at: host_start + i,
            });
        }
    }

    let port = if let Some(colon_rel) = host_port_colon {
        let port_start = host_start + colon_rel + 1;
        let port_text = &host_port[colon_rel + 1..];
        let port_bytes = port_text.as_bytes();

        if port_bytes.is_empty() {
            return Err(ParseError::InvalidPort { at: port_start });
        }

        for (i, &byte) in port_bytes.iter().enumerate() {
            if !byte.is_ascii_digit() {
                return Err(ParseError::InvalidPort {
                    at: port_start + i,
                });
            }
        }

        if port_bytes.len() >= 2 && port_bytes[0] == b'0' {
            return Err(ParseError::InvalidPort { at: port_start });
        }

        let mut value: u32 = 0;
        for &byte in port_bytes {
            value = match value
                .checked_mul(10)
                .and_then(|v| v.checked_add(u32::from(byte - b'0')))
            {
                Some(v) => v,
                None => return Err(ParseError::InvalidPort { at: port_start }),
            };
        }

        if !(1..=u16::MAX as u32).contains(&value) {
            return Err(ParseError::InvalidPort { at: port_start });
        }

        Some(value as u16)
    } else {
        None
    };

    let remainder = &input[authority_end..];
    let fragment_marker = remainder.find('#');
    let before_fragment = fragment_marker.map_or(remainder, |at| &remainder[..at]);
    let fragment = fragment_marker.map(|at| remainder[at + 1..].to_owned());

    let query_marker = before_fragment.find('?');
    let path = query_marker.map_or(before_fragment, |at| &before_fragment[..at]);
    let query = query_marker.map(|at| before_fragment[at + 1..].to_owned());

    Ok(Url {
        scheme: input[..colon].to_ascii_lowercase(),
        userinfo,
        host: host.to_ascii_lowercase(),
        port,
        path: if path.is_empty() {
            "/".to_owned()
        } else {
            path.to_owned()
        },
        query,
        fragment,
    })
}

fn main() {}