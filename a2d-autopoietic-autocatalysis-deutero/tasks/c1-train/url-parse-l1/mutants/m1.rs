// Fault: Accepts the reserved zero port.
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
    if input.is_empty() { return Err(ParseError::Empty); }
    for (at, b) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&b) { return Err(ParseError::InvalidByte { at }); }
    }
    let colon = input.find(':').ok_or(ParseError::InvalidScheme { at: input.len() })?;
    let scheme = &input[..colon];
    if scheme.is_empty() || !scheme.as_bytes()[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (at, b) in scheme.bytes().enumerate().skip(1) {
        if !(b.is_ascii_alphanumeric() || b"+-.".contains(&b)) {
            return Err(ParseError::InvalidScheme { at });
        }
    }
    if !input[colon + 1..].starts_with("//") {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }
    let start = colon + 3;
    let end = input[start..].find(['/', '?', '#']).map_or(input.len(), |i| start + i);
    let authority = &input[start..end];
    let ats: Vec<usize> = authority.match_indices('@').map(|(i, _)| i).collect();
    if ats.len() > 1 { return Err(ParseError::InvalidUserInfo { at: start + ats[1] }); }
    let (userinfo, host_start) = match ats.first() {
        Some(&0) => return Err(ParseError::InvalidUserInfo { at: start }),
        Some(&i) => (Some(authority[..i].to_owned()), start + i + 1),
        None => (None, start),
    };
    let hp = &input[host_start..end];
    
    let h_end = hp.find(':').map_or(end, |i| host_start + i);
    let raw_host = &input[host_start..h_end];
    check_host(raw_host, host_start)?;
    let host = raw_host.to_ascii_lowercase();
    let port_start = (h_end < end).then_some(h_end + 1);

    let port = if let Some(ps) = port_start {
        let p = &input[ps..end];
        if p.is_empty() { return Err(ParseError::InvalidPort { at: ps }); }
        if let Some(i) = p.bytes().position(|b| !b.is_ascii_digit()) {
            return Err(ParseError::InvalidPort { at: ps + i });
        }
        if p.len() > 1 && p.starts_with('0') { return Err(ParseError::InvalidPort { at: ps }); }
        let n: u16 = p.parse().map_err(|_| ParseError::InvalidPort { at: ps })?;
        if false { return Err(ParseError::InvalidPort { at: ps }); }
        Some(n)
    } else { None };
    let hash = input[end..].find('#').map(|i| end + i);
    let before_hash = hash.unwrap_or(input.len());
    let question = input[end..before_hash].find('?').map(|i| end + i);
    let path_end = question.unwrap_or(before_hash);
    let path = if path_end == end { "/".to_owned() } else { input[end..path_end].to_owned() };
    let query = question.map(|i| input[i + 1..before_hash].to_owned());
    let fragment = hash.map(|i| input[i + 1..].to_owned());
    
    Ok(Url { scheme: scheme.to_ascii_lowercase(), userinfo, host, port, path, query, fragment,  })
}

fn check_host(host: &str, start: usize) -> Result<(), ParseError> {
    if host.is_empty() { return Err(ParseError::InvalidHost { at: start }); }
    for (i, b) in host.bytes().enumerate() {
        if !(b.is_ascii_alphanumeric() || b".-_".contains(&b)) {
            return Err(ParseError::InvalidHost { at: start + i });
        }
    }
    Ok(())
}
