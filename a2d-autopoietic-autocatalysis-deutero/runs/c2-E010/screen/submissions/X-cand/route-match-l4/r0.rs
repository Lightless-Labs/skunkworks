use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidPattern { at: usize },
    InvalidPath { at: usize },
    NotFound,
    InvalidMethod,
    MethodNotAllowed { allowed: Vec<String> },
    InvalidName,
    DuplicateName,
    Conflict { with: usize },
    UnknownName,
    DuplicateParam { name: String },
    MissingParam { name: String },
    ExtraParam { name: String },
    InvalidValue { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub route: usize,
    pub params: Vec<(String, String)>,
    pub redirect: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashPolicy { Strict, Ignore, Redirect }

#[derive(Clone)]
enum Part { Lit(String), Arg(String), Tail(String) }
struct Entry {
    id: usize,
    name: String,
    method: String,
    parts: Vec<Part>,
    slash: bool,
    rank: Vec<u8>,
}
pub struct Router { policy: SlashPolicy, entries: Vec<Entry> }

impl Router {
    pub fn new(policy: SlashPolicy) -> Self { Self { policy, entries: Vec::new() } }

    pub fn add(&mut self, name: &str, method: &str, pattern: &str) -> Result<usize, Error> {
        if !method_ok(method) { return Err(Error::InvalidMethod); }
        if !name_ok(name) { return Err(Error::InvalidName); }
        let (parts, slash) = parse(pattern, true).map_err(|at| Error::InvalidPattern { at })?;
        if self.entries.iter().any(|e| e.name == name) { return Err(Error::DuplicateName); }
        let id = self.entries.len();
        for e in &self.entries {
            if e.method == method && shape_equal(&e.parts, &parts)
                && (self.policy != SlashPolicy::Strict || e.slash == slash) {
                return Err(Error::Conflict { with: e.id });
            }
        }
        let rank = make_rank(&parts);
        self.entries.push(Entry { id, name: name.to_owned(), method: method.to_owned(), parts, slash, rank });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !method_ok(method) { return Err(Error::InvalidMethod); }
        let cut = path.bytes().position(|b| b == b'?' || b == b'#').unwrap_or(path.len());
        let head = &path[..cut];
        let (segments, slash) = parse(head, false).map_err(|at| Error::InvalidPath { at })?;

        let mut found: Vec<(&Entry, Vec<(String, String)>)> = Vec::new();
        for e in &self.entries {
            if self.policy == SlashPolicy::Strict && e.slash != slash { continue; }
            if let Some(p) = matches_parts(&e.parts, &segments) { found.push((e, p)); }
        }
        if found.is_empty() { return Err(Error::NotFound); }
        let use_head = method == "HEAD" && found.iter().any(|(e, _)| e.method == "HEAD");
        let wanted = if method == "HEAD" && !use_head { "GET" } else { method };
        let mut choices: Vec<usize> = found.iter().enumerate()
            .filter(|(_, (e, _))| e.method == wanted).map(|(i, _)| i).collect();
        if choices.is_empty() {
            let mut set = BTreeSet::new();
            for (e, _) in &found {
                set.insert(e.method.clone());
                if e.method == "GET" { set.insert("HEAD".to_string()); }
            }
            return Err(Error::MethodNotAllowed { allowed: set.into_iter().collect() });
        }
        choices.sort_by(|a, b| found[*b].0.rank.cmp(&found[*a].0.rank)
            .then_with(|| found[*a].0.id.cmp(&found[*b].0.id)));
        let (winner, params) = &found[choices[0]];
        let redirect = if self.policy == SlashPolicy::Redirect && winner.slash != slash
            && !segments.is_empty() {
            let mut p = head.to_string();
            if winner.slash { p.push('/'); } else { p.pop(); }
            Some(p)
        } else { None };
        Ok(Match { route: winner.id, params: params.clone(), redirect })
    }

    pub fn build(&self, name: &str, params: &[(&str, &str)]) -> Result<String, Error> {
        let e = self.entries.iter().find(|e| e.name == name).ok_or(Error::UnknownName)?;
        let mut values: HashMap<&str, &str> = HashMap::new();
        for &(k, v) in params {
            if values.contains_key(k) { return Err(Error::DuplicateParam { name: k.to_owned() }); }
            values.insert(k, v);
        }
        let names: Vec<&str> = e.parts.iter().filter_map(|p| match p {
            Part::Arg(n) | Part::Tail(n) => Some(n.as_str()), _ => None
        }).collect();
        for n in &names {
            if !values.contains_key(n) { return Err(Error::MissingParam { name: (*n).to_owned() }); }
        }
        let valid: HashSet<&str> = names.iter().copied().collect();
        if let Some(k) = values.keys().filter(|k| !valid.contains(**k)).min() {
            return Err(Error::ExtraParam { name: (*k).to_owned() });
        }
        for n in &names {
            let tail = e.parts.iter().any(|p| matches!(p, Part::Tail(x) if x == n));
            if !build_value_ok(values[n], tail) { return Err(Error::InvalidValue { name: (*n).to_owned() }); }
        }

        let mut out = String::from("/");
        for (i, part) in e.parts.iter().enumerate() {
            if i > 0 { out.push('/'); }
            match part {
                Part::Lit(s) => out.push_str(&encode(s)),
                Part::Arg(n) => out.push_str(&encode(values[n.as_str()])),
                Part::Tail(n) => {
                    for (j, component) in values[n.as_str()].split('/').enumerate() {
                        if j > 0 { out.push('/'); }
                        out.push_str(&encode(component));
                    }
                }
            }
        }
        if e.slash && !e.parts.is_empty() { out.push('/'); }
        Ok(out)
    }
}

fn method_ok(s: &str) -> bool { !s.is_empty() && s.bytes().all(|b| b'A' <= b && b <= b'Z') }
fn name_ok(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty() && b[0].is_ascii_lowercase()
        && b.iter().all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || *x == b'_' || *x == b'-')
}
fn capture_ok(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty() && b[0].is_ascii_lowercase()
        && b.iter().all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || *x == b'_')
}
fn unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}
fn hex(b: u8) -> Option<u8> {
    match b { b'0'..=b'9' => Some(b - b'0'), b'A'..=b'F' => Some(b - b'A' + 10), _ => None }
}
fn decode(raw: &[u8]) -> Result<String, usize> {
    let mut v = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%' {
            if i + 2 >= raw.len() { return Err(i); }
            let h = hex(raw[i + 1]).ok_or(i)?;
            let l = hex(raw[i + 2]).ok_or(i)?;
            let b = h * 16 + l;
            if !(0x20..=0x7e).contains(&b) || b == b'/' || unreserved(b) { return Err(i); }
            v.push(b);
            i += 3;
        } else {
            if !unreserved(raw[i]) { return Err(i); }
            v.push(raw[i]);
            i += 1;
        }
    }
    Ok(String::from_utf8(v).unwrap())
}
fn parse(s: &str, pattern: bool) -> Result<(Vec<Part>, bool), usize> {
    let b = s.as_bytes();
    if b.first() != Some(&b'/') { return Err(0); }
    if s == "/" { return Ok((Vec::new(), false)); }
    let slash = b.last() == Some(&b'/');
    let stop = if slash { b.len() - 1 } else { b.len() };
    let mut result = Vec::new();
    let mut names = HashSet::new();
    let mut start = 1;
    while start < stop {
        let end = b[start..stop].iter().position(|x| *x == b'/').map(|x| start + x).unwrap_or(stop);
        if end == start { return Err(start); }
        let raw = &s[start..end];
        if pattern && (raw.starts_with(':') || raw.starts_with('*')) {
            let wildcard = raw.as_bytes()[0] == b'*';
            let n = &raw[1..];
            if !capture_ok(n) || !names.insert(n.to_owned()) { return Err(start); }
            if wildcard && end != stop { return Err(start); }
            result.push(if wildcard { Part::Tail(n.to_owned()) } else { Part::Arg(n.to_owned()) });
        } else {
            result.push(Part::Lit(decode(&b[start..end]).map_err(|x| start + x)?));
        }
        if end == stop { break; }
        start = end + 1;
    }
    Ok((result, slash))
}
fn make_rank(parts: &[Part]) -> Vec<u8> {
    let mut r = Vec::new();
    for p in parts {
        match p { Part::Lit(_) => r.push(2), Part::Arg(_) => r.push(1), Part::Tail(_) => { r.push(0); return r; } }
    }
    r.push(3);
    r
}
fn shape_equal(a: &[Part], b: &[Part]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| match (x, y) {
        (Part::Lit(a), Part::Lit(b)) => a == b,
        (Part::Arg(_), Part::Arg(_)) | (Part::Tail(_), Part::Tail(_)) => true,
        _ => false,
    })
}
fn matches_parts(pattern: &[Part], path: &[Part]) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    let mut i = 0;
    for p in pattern {
        match p {
            Part::Tail(n) => {
                let tail = path[i..].iter().map(|x| match x { Part::Lit(s) => s.as_str(), _ => "" }).collect::<Vec<_>>().join("/");
                out.push((n.clone(), tail));
                return Some(out);
            }
            Part::Lit(s) => match path.get(i) { Some(Part::Lit(v)) if v == s => (), _ => return None },
            Part::Arg(n) => match path.get(i) { Some(Part::Lit(v)) => out.push((n.clone(), v.clone())), _ => return None },
        }
        i += 1;
    }
    if i == path.len() { Some(out) } else { None }
}
fn build_value_ok(s: &str, tail: bool) -> bool {
    if !s.bytes().all(|b| (0x20..=0x7e).contains(&b)) { return false; }
    if tail { s.is_empty() || s.split('/').all(|p| !p.is_empty()) }
    else { !s.is_empty() && !s.contains('/') }
}
fn encode(s: &str) -> String {
    let mut out = String::new();
    const H: &[u8; 16] = b"0123456789ABCDEF";
    for b in s.bytes() {
        if unreserved(b) { out.push(b as char); }
        else { out.push('%'); out.push(H[(b >> 4) as usize] as char); out.push(H[(b & 15) as usize] as char); }
    }
    out
}
