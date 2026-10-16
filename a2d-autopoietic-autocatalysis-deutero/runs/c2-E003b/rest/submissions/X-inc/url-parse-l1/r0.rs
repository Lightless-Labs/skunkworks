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
        .map(|offset| authority_start + offset)
        .unwrap_or(bytes.len());

    let authority = &input[authority_start..authority_end];
    let authority_bytes = authority.as_bytes();

    let at_positions: Vec<usize> = authority_bytes
        .iter()
        .enumerate()
        .filter_map(|(index, &byte)| (byte == b'@').then_some(index))
        .collect();

    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }

    let (userinfo, hostport_start) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo { at: authority_start });
        }
        (
            Some(authority[..at].to_owned()),
            authority_start + at + 1,
        )
    } else {
        (None, authority_start)
    };

    let hostport = &input[hostport_start..authority_end];
    let hostport_bytes = hostport.as_bytes();
    let host_colon = hostport_bytes.iter().position(|&byte| byte == b':');
    let host_end = host_colon.unwrap_or(hostport.len());

    if host_end == 0 {
        return Err(ParseError::InvalidHost { at: hostport_start });
    }

    for (offset, &byte) in hostport_bytes[..host_end].iter().enumerate() {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
            return Err(ParseError::InvalidHost {
                at: hostport_start + offset,
            });
        }
    }

    let port = if let Some(colon_offset) = host_colon {
        let port_start = hostport_start + colon_offset + 1;
        let port_text = &hostport[colon_offset + 1..];
        let port_bytes = port_text.as_bytes();

        if port_bytes.is_empty() {
            return Err(ParseError::InvalidPort { at: port_start });
        }

        if let Some(offset) = port_bytes.iter().position(|byte| !byte.is_ascii_digit()) {
            return Err(ParseError::InvalidPort {
                at: port_start + offset,
            });
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

    let host = input[hostport_start..hostport_start + host_end].to_ascii_lowercase();

    let fragment_start = bytes[authority_end..]
        .iter()
        .position(|&byte| byte == b'#')
        .map(|offset| authority_end + offset);

    let before_fragment_end = fragment_start.unwrap_or(bytes.len());
    let query_start = bytes[authority_end..before_fragment_end]
        .iter()
        .position(|&byte| byte == b'?')
        .map(|offset| authority_end + offset);

    let path_end = query_start.unwrap_or(before_fragment_end);
    let path = if authority_end == path_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };

    let query = query_start.map(|start| {
        let end = fragment_start.unwrap_or(bytes.len());
        input[start + 1..end].to_owned()
    });

    let fragment = fragment_start.map(|start| input[start + 1..].to_owned());

    Ok(Url {
        scheme: input[..colon].to_ascii_lowercase(),
        userinfo,
        host,
        port,
        path,
        query,
        fragment,
    })
}