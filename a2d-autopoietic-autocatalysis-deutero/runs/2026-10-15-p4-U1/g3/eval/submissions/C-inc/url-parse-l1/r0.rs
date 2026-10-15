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
        Some(index) => index,
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
        .map(|offset| authority_start + offset)
        .unwrap_or(bytes.len());

    let authority = &bytes[authority_start..authority_end];
    let at_positions: Vec<usize> = authority
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i))
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
            Some(input[authority_start..authority_start + at].to_owned()),
            authority_start + at + 1,
        )
    } else {
        (None, authority_start)
    };

    let hostport = &bytes[hostport_start..authority_end];
    let port_colon = hostport.iter().position(|&b| b == b':');
    let host_end = port_colon
        .map(|offset| hostport_start + offset)
        .unwrap_or(authority_end);

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

    let port = if let Some(offset) = port_colon {
        let port_start = hostport_start + offset + 1;
        if port_start == authority_end {
            return Err(ParseError::InvalidPort { at: port_start });
        }

        for (i, &byte) in bytes[port_start..authority_end].iter().enumerate() {
            if !byte.is_ascii_digit() {
                return Err(ParseError::InvalidPort { at: port_start + i });
            }
        }

        let port_bytes = &bytes[port_start..authority_end];
        if port_bytes.len() >= 2 && port_bytes[0] == b'0' {
            return Err(ParseError::InvalidPort { at: port_start });
        }

        let mut value: u32 = 0;
        for &digit in port_bytes {
            value = match value
                .checked_mul(10)
                .and_then(|v| v.checked_add((digit - b'0') as u32))
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

    let mut fragment = None;
    let before_fragment_end = if let Some(relative) = bytes[authority_end..]
        .iter()
        .position(|&b| b == b'#')
    {
        let start = authority_end + relative + 1;
        fragment = Some(input[start..].to_owned());
        authority_end + relative
    } else {
        bytes.len()
    };

    let mut query = None;
    let before_query_end = if let Some(relative) = bytes[authority_end..before_fragment_end]
        .iter()
        .position(|&b| b == b'?')
    {
        let start = authority_end + relative + 1;
        query = Some(input[start..before_fragment_end].to_owned());
        authority_end + relative
    } else {
        before_fragment_end
    };

    let path = if before_query_end == authority_end {
        "/".to_owned()
    } else {
        input[authority_end..before_query_end].to_owned()
    };

    let scheme = input[..colon].to_ascii_lowercase();
    let host = input[hostport_start..host_end].to_ascii_lowercase();

    Ok(Url {
        scheme,
        userinfo,
        host,
        port,
        path,
        query,
        fragment,
    })
}

fn main() {}