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
        Some(colon) => colon,
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

    let authority_start = colon + 1;
    if bytes.get(authority_start) != Some(&b'/')
        || bytes.get(authority_start + 1) != Some(&b'/')
    {
        return Err(ParseError::MissingAuthority { at: authority_start });
    }

    let authority_start = authority_start + 2;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|&byte| matches!(byte, b'/' | b'?' | b'#'))
        .map(|offset| authority_start + offset)
        .unwrap_or(bytes.len());

    let authority = &input[authority_start..authority_end];
    let authority_bytes = authority.as_bytes();

    let mut at_signs = authority_bytes
        .iter()
        .enumerate()
        .filter_map(|(index, &byte)| (byte == b'@').then_some(index));

    let first_at = at_signs.next();
    let second_at = at_signs.next();

    if let Some(second_at) = second_at {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + second_at,
        });
    }

    let (userinfo, hostport_start) = if let Some(at) = first_at {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo { at: authority_start });
        }
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let hostport = &authority[hostport_start..];
    let hostport_bytes = hostport.as_bytes();

    let host_colon = hostport_bytes.iter().position(|&byte| byte == b':');
    let host_end = host_colon.unwrap_or(hostport.len());
    let host = &hostport[..host_end];
    let host_start = authority_start + hostport_start;

    if host.is_empty() {
        return Err(ParseError::InvalidHost { at: host_start });
    }

    for (offset, &byte) in host.as_bytes().iter().enumerate() {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
            return Err(ParseError::InvalidHost {
                at: host_start + offset,
            });
        }
    }

    let port = if let Some(colon_offset) = host_colon {
        let port_start = host_start + colon_offset + 1;
        let port_text = &hostport[colon_offset + 1..];

        if port_text.is_empty() {
            return Err(ParseError::InvalidPort { at: port_start });
        }

        for (offset, &byte) in port_text.as_bytes().iter().enumerate() {
            if !byte.is_ascii_digit() {
                return Err(ParseError::InvalidPort {
                    at: port_start + offset,
                });
            }
        }

        if port_text.len() >= 2 && port_text.as_bytes()[0] == b'0' {
            return Err(ParseError::InvalidPort { at: port_start });
        }

        let mut value: u32 = 0;
        for &byte in port_text.as_bytes() {
            value = match value
                .checked_mul(10)
                .and_then(|current| current.checked_add((byte - b'0') as u32))
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
    let fragment = remainder.find('#').map(|index| remainder[index + 1..].to_owned());
    let before_fragment = remainder
        .find('#')
        .map(|index| &remainder[..index])
        .unwrap_or(remainder);

    let query = before_fragment
        .find('?')
        .map(|index| before_fragment[index + 1..].to_owned());
    let path = before_fragment
        .find('?')
        .map(|index| &before_fragment[..index])
        .unwrap_or(before_fragment);

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