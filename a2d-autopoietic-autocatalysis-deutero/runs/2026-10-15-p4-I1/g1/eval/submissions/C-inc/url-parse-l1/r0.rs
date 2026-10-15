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
    if input.is_empty() {
        return Err(ParseError::Empty);
    }

    let bytes = input.as_bytes();

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
        .map_or(input.len(), |offset| authority_start + offset);

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

    let (userinfo, host_port_start) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
            });
        }
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let host_port = &authority[host_port_start..];
    let host_port_bytes = host_port.as_bytes();
    let host_start = authority_start + host_port_start;

    let host_port_colon = host_port_bytes.iter().position(|&b| b == b':');
    let (host_part, port_part, port_start) = match host_port_colon {
        Some(relative_colon) => (
            &host_port[..relative_colon],
            Some(&host_port[relative_colon + 1..]),
            Some(host_start + relative_colon + 1),
        ),
        None => (host_port, None, None),
    };

    if host_part.is_empty() {
        return Err(ParseError::InvalidHost { at: host_start });
    }

    for (offset, &byte) in host_part.as_bytes().iter().enumerate() {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
            return Err(ParseError::InvalidHost {
                at: host_start + offset,
            });
        }
    }

    let port = if let (Some(port_text), Some(port_at)) = (port_part, port_start) {
        let port_bytes = port_text.as_bytes();
        if port_bytes.is_empty() {
            return Err(ParseError::InvalidPort { at: port_at });
        }

        for (offset, &byte) in port_bytes.iter().enumerate() {
            if !byte.is_ascii_digit() {
                return Err(ParseError::InvalidPort {
                    at: port_at + offset,
                });
            }
        }

        if port_bytes.len() >= 2 && port_bytes[0] == b'0' {
            return Err(ParseError::InvalidPort { at: port_at });
        }

        let mut value: u32 = 0;
        for &byte in port_bytes {
            value = match value
                .checked_mul(10)
                .and_then(|n| n.checked_add((byte - b'0') as u32))
            {
                Some(value) => value,
                None => return Err(ParseError::InvalidPort { at: port_at }),
            };
        }

        if !(1..=65535).contains(&value) {
            return Err(ParseError::InvalidPort { at: port_at });
        }

        Some(value as u16)
    } else {
        None
    };

    let remainder = &input[authority_end..];
    let fragment_index = remainder.as_bytes().iter().position(|&b| b == b'#');
    let before_fragment = fragment_index.map_or(remainder, |index| &remainder[..index]);
    let fragment = fragment_index.map(|index| remainder[index + 1..].to_owned());

    let query_index = before_fragment.as_bytes().iter().position(|&b| b == b'?');
    let path_part = query_index.map_or(before_fragment, |index| &before_fragment[..index]);
    let query = query_index.map(|index| before_fragment[index + 1..].to_owned());

    let path = if path_part.is_empty() {
        "/".to_owned()
    } else {
        path_part.to_owned()
    };

    Ok(Url {
        scheme: input[..colon].to_ascii_lowercase(),
        userinfo,
        host: host_part.to_ascii_lowercase(),
        port,
        path,
        query,
        fragment,
    })
}

fn main() {}