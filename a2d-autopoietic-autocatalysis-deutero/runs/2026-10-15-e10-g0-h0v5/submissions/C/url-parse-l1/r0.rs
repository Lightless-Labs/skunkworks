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

    let colon = match bytes.iter().position(|&byte| byte == b':') {
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
        .position(|&byte| matches!(byte, b'/' | b'?' | b'#'))
        .map(|offset| authority_start + offset)
        .unwrap_or(bytes.len());

    let authority = &bytes[authority_start..authority_end];
    let at_positions: Vec<usize> = authority
        .iter()
        .enumerate()
        .filter_map(|(offset, &byte)| (byte == b'@').then_some(offset))
        .collect();

    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }

    let (userinfo, hostport_start) = if let Some(&userinfo_at) = at_positions.first() {
        if userinfo_at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + userinfo_at,
            });
        }
        (
            Some(input[authority_start..authority_start + userinfo_at].to_owned()),
            authority_start + userinfo_at + 1,
        )
    } else {
        (None, authority_start)
    };

    let hostport = &bytes[hostport_start..authority_end];
    let port_colon = hostport.iter().position(|&byte| byte == b':');
    let host_end = port_colon
        .map(|offset| hostport_start + offset)
        .unwrap_or(authority_end);

    if host_end == hostport_start {
        return Err(ParseError::InvalidHost { at: hostport_start });
    }

    for (at, &byte) in bytes[hostport_start..host_end].iter().enumerate() {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
            return Err(ParseError::InvalidHost {
                at: hostport_start + at,
            });
        }
    }

    let port = if let Some(offset) = port_colon {
        let port_start = hostport_start + offset + 1;
        if port_start == authority_end {
            return Err(ParseError::InvalidPort { at: port_start });
        }

        for (at, &byte) in bytes[port_start..authority_end].iter().enumerate() {
            if !byte.is_ascii_digit() {
                return Err(ParseError::InvalidPort {
                    at: port_start + at,
                });
            }
        }

        let port_bytes = &bytes[port_start..authority_end];
        if port_bytes.len() >= 2 && port_bytes[0] == b'0' {
            return Err(ParseError::InvalidPort { at: port_start });
        }

        let mut value: u32 = 0;
        for &byte in port_bytes {
            value = match value
                .checked_mul(10)
                .and_then(|value| value.checked_add((byte - b'0') as u32))
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
        .position(|&byte| byte == b'#')
        .map(|offset| component_start + offset);

    let before_fragment_end = fragment_delimiter.unwrap_or(bytes.len());
    let query_delimiter = bytes[component_start..before_fragment_end]
        .iter()
        .position(|&byte| byte == b'?')
        .map(|offset| component_start + offset);

    let path_end = query_delimiter.unwrap_or(before_fragment_end);
    let path = if path_end == component_start {
        "/".to_owned()
    } else {
        input[component_start..path_end].to_owned()
    };

    let query = query_delimiter.map(|at| {
        input[at + 1..before_fragment_end].to_owned()
    });
    let fragment = fragment_delimiter.map(|at| input[at + 1..].to_owned());

    Ok(Url {
        scheme: input[..colon].to_ascii_lowercase(),
        userinfo,
        host: input[hostport_start..host_end].to_ascii_lowercase(),
        port,
        path,
        query,
        fragment,
    })
}

fn main() {}