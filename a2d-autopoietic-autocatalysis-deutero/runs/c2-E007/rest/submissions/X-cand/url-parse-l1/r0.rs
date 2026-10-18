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
        None => return Err(ParseError::InvalidScheme { at: bytes.len() }),
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
    let authority = &bytes[authority_start..authority_end];

    let at_positions: Vec<usize> = authority
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i))
        .collect();

    if at_positions.len() > 1 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }

    let (userinfo, hostport_start) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo { at: authority_start });
        }
        (Some(input[authority_start..authority_start + at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let hostport = &authority[hostport_start..];
    let host_start = authority_start + hostport_start;
    let host_colon = hostport.iter().position(|&b| b == b':');
    let host_end = host_colon.unwrap_or(hostport.len());
    let host_bytes = &hostport[..host_end];

    if host_bytes.is_empty() {
        return Err(ParseError::InvalidHost { at: host_start });
    }

    for (offset, &byte) in host_bytes.iter().enumerate() {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
            return Err(ParseError::InvalidHost {
                at: host_start + offset,
            });
        }
    }

    let port = if let Some(colon_offset) = host_colon {
        let port_start = host_start + colon_offset + 1;
        let port_bytes = &hostport[colon_offset + 1..];

        if port_bytes.is_empty() {
            return Err(ParseError::InvalidPort { at: port_start });
        }

        for (offset, &byte) in port_bytes.iter().enumerate() {
            if !byte.is_ascii_digit() {
                return Err(ParseError::InvalidPort {
                    at: port_start + offset,
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
                .and_then(|v| v.checked_add((byte - b'0') as u32))
            {
                Some(value) => value,
                None => return Err(ParseError::InvalidPort { at: port_start }),
            };
        }

        if !(1..=65535).contains(&value) {
            return Err(ParseError::InvalidPort { at: port_start });
        }

        Some(value as u16)
    } else {
        None
    };

    let component_start = authority_end;
    let fragment_delimiter = bytes[component_start..]
        .iter()
        .position(|&b| b == b'#')
        .map(|offset| component_start + offset);
    let before_fragment_end = fragment_delimiter.unwrap_or(bytes.len());

    let query_delimiter = bytes[component_start..before_fragment_end]
        .iter()
        .position(|&b| b == b'?')
        .map(|offset| component_start + offset);

    let path_end = query_delimiter.unwrap_or(before_fragment_end);
    let path = if component_start == path_end {
        "/".to_owned()
    } else {
        input[component_start..path_end].to_owned()
    };

    let query = query_delimiter.map(|at| {
        let end = fragment_delimiter.unwrap_or(bytes.len());
        input[at + 1..end].to_owned()
    });

    let fragment = fragment_delimiter.map(|at| input[at + 1..].to_owned());

    Ok(Url {
        scheme: input[..colon].to_ascii_lowercase(),
        userinfo,
        host: input[host_start..host_start + host_bytes.len()].to_ascii_lowercase(),
        port,
        path,
        query,
        fragment,
    })
}