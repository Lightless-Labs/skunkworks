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
        .unwrap_or(input.len());

    let authority = &input[authority_start..authority_end];
    let authority_bytes = authority.as_bytes();

    let at_positions: Vec<usize> = authority_bytes
        .iter()
        .enumerate()
        .filter_map(|(offset, &byte)| (byte == b'@').then_some(offset))
        .collect();

    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }

    let (userinfo, host_port_start) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo { at: authority_start });
        }
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let host_port = &authority[host_port_start..];
    let host_port_bytes = host_port.as_bytes();
    let host_port_absolute_start = authority_start + host_port_start;

    let host_colon = host_port_bytes.iter().position(|&byte| byte == b':');
    let host_end = host_colon.unwrap_or(host_port.len());
    let host_part = &host_port[..host_end];

    if host_part.is_empty() {
        return Err(ParseError::InvalidHost {
            at: host_port_absolute_start,
        });
    }

    for (offset, &byte) in host_part.as_bytes().iter().enumerate() {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
            return Err(ParseError::InvalidHost {
                at: host_port_absolute_start + offset,
            });
        }
    }

    let port = if let Some(colon_offset) = host_colon {
        let port_start = host_port_absolute_start + colon_offset + 1;
        let port_text = &host_port[colon_offset + 1..];
        let port_bytes = port_text.as_bytes();

        if port_text.is_empty() {
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
                .and_then(|current| current.checked_add(u32::from(byte - b'0')))
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

    let remainder = &input[authority_end..];
    let (before_fragment, fragment) = match remainder.find('#') {
        Some(offset) => (
            &remainder[..offset],
            Some(remainder[offset + 1..].to_owned()),
        ),
        None => (remainder, None),
    };

    let (path_part, query) = match before_fragment.find('?') {
        Some(offset) => (
            &before_fragment[..offset],
            Some(before_fragment[offset + 1..].to_owned()),
        ),
        None => (before_fragment, None),
    };

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