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
    for (i, &b) in bytes.iter().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ParseError::InvalidByte { at: i });
        }
    }

    let colon = match bytes.iter().position(|&b| b == b':') {
        Some(i) => i,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };

    if colon == 0 || !bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (i, &b) in bytes[..colon].iter().enumerate().skip(1) {
        if !(b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at: i });
        }
    }

    let marker_at = colon + 1;
    if bytes.get(marker_at) != Some(&b'/') || bytes.get(marker_at + 1) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: marker_at });
    }

    let authority_start = marker_at + 2;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|i| authority_start + i)
        .unwrap_or(bytes.len());
    let authority = &bytes[authority_start..authority_end];

    let mut at_positions = authority
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i));
    let first_at = at_positions.next();
    if let Some(second_at) = at_positions.next() {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + second_at,
        });
    }

    let (userinfo, hostport_start) = match first_at {
        Some(i) if i == 0 => {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + i,
            });
        }
        Some(i) => (
            Some(input[authority_start..authority_start + i].to_owned()),
            authority_start + i + 1,
        ),
        None => (None, authority_start),
    };

    let hostport = &bytes[hostport_start..authority_end];
    let port_colon_rel = hostport.iter().position(|&b| b == b':');
    let host_end = port_colon_rel
        .map(|i| hostport_start + i)
        .unwrap_or(authority_end);

    if host_end == hostport_start {
        return Err(ParseError::InvalidHost { at: hostport_start });
    }
    for (i, &b) in bytes[hostport_start..host_end].iter().enumerate() {
        if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
            return Err(ParseError::InvalidHost {
                at: hostport_start + i,
            });
        }
    }

    let port = if let Some(rel) = port_colon_rel {
        let port_start = hostport_start + rel + 1;
        if port_start == authority_end {
            return Err(ParseError::InvalidPort { at: port_start });
        }

        let port_bytes = &bytes[port_start..authority_end];
        for (i, &b) in port_bytes.iter().enumerate() {
            if !b.is_ascii_digit() {
                return Err(ParseError::InvalidPort { at: port_start + i });
            }
        }
        if port_bytes.len() >= 2 && port_bytes[0] == b'0' {
            return Err(ParseError::InvalidPort { at: port_start });
        }

        let mut value: u32 = 0;
        for &b in port_bytes {
            value = match value.checked_mul(10).and_then(|v| v.checked_add((b - b'0') as u32)) {
                Some(v) => v,
                None => return Err(ParseError::InvalidPort { at: port_start }),
            };
            if value > u16::MAX as u32 {
                return Err(ParseError::InvalidPort { at: port_start });
            }
        }
        if value == 0 {
            return Err(ParseError::InvalidPort { at: port_start });
        }
        Some(value as u16)
    } else {
        None
    };

    let fragment_mark = bytes[authority_end..].iter().position(|&b| b == b'#')
        .map(|i| authority_end + i);
    let before_fragment_end = fragment_mark.unwrap_or(bytes.len());
    let query_mark = bytes[authority_end..before_fragment_end]
        .iter()
        .position(|&b| b == b'?')
        .map(|i| authority_end + i);

    let path_end = query_mark.unwrap_or(before_fragment_end);
    let path = if path_end == authority_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };

    let query = query_mark.map(|q| input[q + 1..before_fragment_end].to_owned());
    let fragment = fragment_mark.map(|f| input[f + 1..].to_owned());

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
