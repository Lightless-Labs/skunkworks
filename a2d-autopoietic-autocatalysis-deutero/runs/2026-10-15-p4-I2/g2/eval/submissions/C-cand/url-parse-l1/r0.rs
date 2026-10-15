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
    let authority = &input[authority_start..authority_end];

    let mut at_positions = authority.match_indices('@').map(|(at, _)| at);
    let first_at = at_positions.next();
    if let Some(second_at) = at_positions.next() {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + second_at,
        });
    }

    let (userinfo, hostport, hostport_start) = match first_at {
        Some(at) if at == 0 => {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
            });
        }
        Some(at) => (
            Some(authority[..at].to_owned()),
            &authority[at + 1..],
            authority_start + at + 1,
        ),
        None => (None, authority, authority_start),
    };

    let host_colon = hostport.find(':');
    let (host_text, port_text, port_start) = match host_colon {
        Some(at) => (
            &hostport[..at],
            Some(&hostport[at + 1..]),
            hostport_start + at + 1,
        ),
        None => (hostport, None, hostport_start),
    };

    if host_text.is_empty() {
        return Err(ParseError::InvalidHost { at: hostport_start });
    }

    for (offset, &byte) in host_text.as_bytes().iter().enumerate() {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
            return Err(ParseError::InvalidHost {
                at: hostport_start + offset,
            });
        }
    }

    let port = match port_text {
        None => None,
        Some("") => return Err(ParseError::InvalidPort { at: port_start }),
        Some(text) => {
            for (offset, &byte) in text.as_bytes().iter().enumerate() {
                if !byte.is_ascii_digit() {
                    return Err(ParseError::InvalidPort {
                        at: port_start + offset,
                    });
                }
            }

            if text.len() >= 2 && text.as_bytes()[0] == b'0' {
                return Err(ParseError::InvalidPort { at: port_start });
            }

            let mut value: u32 = 0;
            for &byte in text.as_bytes() {
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
        }
    };

    let after_authority = &input[authority_end..];
    let fragment_marker = after_authority.find('#');
    let before_fragment = fragment_marker
        .map_or(after_authority, |at| &after_authority[..at]);
    let fragment = fragment_marker.map(|at| after_authority[at + 1..].to_owned());

    let query_marker = before_fragment.find('?');
    let path = query_marker
        .map_or(before_fragment, |at| &before_fragment[..at]);
    let query = query_marker.map(|at| before_fragment[at + 1..].to_owned());

    let path = if path.is_empty() {
        "/".to_owned()
    } else {
        path.to_owned()
    };

    Ok(Url {
        scheme: input[..colon].to_ascii_lowercase(),
        userinfo,
        host: host_text.to_ascii_lowercase(),
        port,
        path,
        query,
        fragment,
    })
}

fn main() {}