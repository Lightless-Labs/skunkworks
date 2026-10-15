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

    let colon = match bytes.iter().position(|&byte| byte == b':') {
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
        .position(|&byte| matches!(byte, b'/' | b'?' | b'#'))
        .map_or(bytes.len(), |offset| authority_start + offset);

    let authority = &bytes[authority_start..authority_end];
    let at_positions: Vec<usize> = authority
        .iter()
        .enumerate()
        .filter_map(|(offset, &byte)| (byte == b'@').then_some(offset))
        .collect();

    if at_positions.len() > 1 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }

    let (userinfo, hostport_start) = if let Some(&offset) = at_positions.first() {
        if offset == 0 {
            return Err(ParseError::InvalidUserInfo { at: authority_start });
        }
        (
            Some(input[authority_start..authority_start + offset].to_owned()),
            authority_start + offset + 1,
        )
    } else {
        (None, authority_start)
    };

    let hostport = &bytes[hostport_start..authority_end];
    let hostport_colon = hostport.iter().position(|&byte| byte == b':');
    let host_end = hostport_colon.map_or(authority_end, |offset| hostport_start + offset);

    if host_end == hostport_start {
        return Err(ParseError::InvalidHost { at: hostport_start });
    }

    for (offset, &byte) in bytes[hostport_start..host_end].iter().enumerate() {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
            return Err(ParseError::InvalidHost {
                at: hostport_start + offset,
            });
        }
    }

    let port = if let Some(offset) = hostport_colon {
        let port_start = hostport_start + offset + 1;
        if port_start == authority_end {
            return Err(ParseError::InvalidPort { at: port_start });
        }

        let port_bytes = &bytes[port_start..authority_end];
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
                .and_then(|n| n.checked_add((byte - b'0') as u32))
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

    let fragment_start = bytes[authority_end..]
        .iter()
        .position(|&byte| byte == b'#')
        .map(|offset| authority_end + offset);

    let component_end = fragment_start.unwrap_or(bytes.len());
    let query_start = bytes[authority_end..component_end]
        .iter()
        .position(|&byte| byte == b'?')
        .map(|offset| authority_end + offset);

    let path_end = query_start.unwrap_or(component_end);
    let path = if path_end == authority_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };

    let query = query_start.map(|start| input[start + 1..component_end].to_owned());
    let fragment = fragment_start.map(|start| input[start + 1..].to_owned());

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