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
    fn invalid_port_digits(bytes: &[u8], start: usize) -> Result<u16, ParseError> {
        if bytes.is_empty() {
            return Err(ParseError::InvalidPort { at: start });
        }

        for (i, &byte) in bytes.iter().enumerate() {
            if !byte.is_ascii_digit() {
                return Err(ParseError::InvalidPort { at: start + i });
            }
        }

        if bytes.len() > 1 && bytes[0] == b'0' {
            return Err(ParseError::InvalidPort { at: start });
        }

        let mut number: u32 = 0;
        for &byte in bytes {
            let digit = (byte - b'0') as u32;
            if number > (65535 - digit) / 10 {
                return Err(ParseError::InvalidPort { at: start });
            }
            number = number * 10 + digit;
        }
        if !(1..=65535).contains(&number) {
            return Err(ParseError::InvalidPort { at: start });
        }
        Ok(number as u16)
    }

    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(ParseError::Empty);
    }
    for (i, &byte) in bytes.iter().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(ParseError::InvalidByte { at: i });
        }
    }

    let scheme_end = bytes.iter().position(|&b| b == b':');
    let Some(scheme_end) = scheme_end else {
        return Err(ParseError::InvalidScheme { at: bytes.len() });
    };
    if scheme_end == 0 || !bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for i in 1..scheme_end {
        let b = bytes[i];
        if !(b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.') {
            return Err(ParseError::InvalidScheme { at: i });
        }
    }

    let marker = scheme_end + 1;
    if bytes.get(marker) != Some(&b'/') || bytes.get(marker + 1) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: marker });
    }

    let authority_start = marker + 2;
    let authority_end = (authority_start..bytes.len())
        .find(|&i| matches!(bytes[i], b'/' | b'?' | b'#'))
        .unwrap_or(bytes.len());

    let mut userinfo = None;
    let mut host_start = authority_start;
    let mut first_at = None;
    for i in authority_start..authority_end {
        if bytes[i] == b'@' {
            if first_at.is_some() {
                return Err(ParseError::InvalidUserInfo { at: i });
            }
            first_at = Some(i);
        }
    }
    if let Some(at) = first_at {
        if at == authority_start {
            return Err(ParseError::InvalidUserInfo { at });
        }
        userinfo = Some(input[authority_start..at].to_owned());
        host_start = at + 1;
    }

    let colon = (host_start..authority_end).find(|&i| bytes[i] == b':');
    let host_end = colon.unwrap_or(authority_end);
    if host_start == host_end {
        return Err(ParseError::InvalidHost { at: host_start });
    }
    for i in host_start..host_end {
        let b = bytes[i];
        if !(b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_') {
            return Err(ParseError::InvalidHost { at: i });
        }
    }
    let host = input[host_start..host_end].to_ascii_lowercase();

    let port = match colon {
        None => None,
        Some(colon_at) => Some(invalid_port_digits(
            &bytes[colon_at + 1..authority_end],
            colon_at + 1,
        )?),
    };

    let tail = &input[authority_end..];
    let (before_fragment, fragment) = match tail.find('#') {
        Some(index) => (&tail[..index], Some(tail[index + 1..].to_owned())),
        None => (tail, None),
    };
    let (path_part, query) = match before_fragment.find('?') {
        Some(index) => (
            &before_fragment[..index],
            Some(before_fragment[index + 1..].to_owned()),
        ),
        None => (before_fragment, None),
    };

    Ok(Url {
        scheme: input[..scheme_end].to_ascii_lowercase(),
        userinfo,
        host,
        port,
        path: if path_part.is_empty() {
            "/".to_owned()
        } else {
            path_part.to_owned()
        },
        query,
        fragment,
    })
}