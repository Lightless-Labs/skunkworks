use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Vec<String>,
    pub build: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VersionError {
    Empty,
    MissingCore { at: usize },
    LeadingZero { at: usize },
    Overflow { at: usize },
    EmptyIdentifier { at: usize },
    InvalidChar { at: usize },
}

fn split_identifiers(s: &str, base: usize, pre: bool) -> Result<Vec<String>, VersionError> {
    let mut out = Vec::new();
    let mut start = 0;
    loop {
        let end = s[start..].find('.').map(|i| start + i).unwrap_or(s.len());
        let id = &s[start..end];
        if id.is_empty() {
            return Err(VersionError::EmptyIdentifier { at: base + start });
        }
        for (i, b) in id.bytes().enumerate() {
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: base + start + i });
            }
        }
        if pre && id.bytes().all(|b| b.is_ascii_digit()) && id.len() > 1 && id.starts_with('0') {
            return Err(VersionError::LeadingZero { at: base + start });
        }
        out.push(id.to_string());
        if end == s.len() {
            break;
        }
        start = end + 1;
        if start == s.len() {
            return Err(VersionError::EmptyIdentifier { at: base + start });
        }
    }
    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let bytes = s.as_bytes();
    let mut pos = 0usize;
    let mut nums = [0u64; 3];
    for field in 0..3 {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if start == pos {
            if pos == bytes.len() || matches!(bytes[pos], b'.' | b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            return Err(VersionError::InvalidChar { at: pos });
        }
        let text = &s[start..pos];
        if text.len() > 1 && text.starts_with('0') {
            return Err(VersionError::LeadingZero { at: start });
        }
        nums[field] = text.parse().map_err(|_| VersionError::Overflow { at: start })?;
        if field < 2 {
            if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            if bytes[pos] != b'.' {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        }
    }
    let mut pre = Vec::new();
    let mut build = Vec::new();
    if pos < bytes.len() {
        match bytes[pos] {
            b'-' => {
                let pre_start = pos + 1;
                let rest = &s[pre_start..];
                let plus = rest.find('+').map(|i| pre_start + i).unwrap_or(s.len());
                pre = split_identifiers(&s[pre_start..plus], pre_start, true)?;
                pos = plus;
                if pos < s.len() {
                    let build_start = pos + 1;
                    build = split_identifiers(&s[build_start..], build_start, false)?;
                    pos = s.len();
                }
            }
            b'+' => {
                let build_start = pos + 1;
                build = split_identifiers(&s[build_start..], build_start, false)?;
                pos = s.len();
            }
            _ => return Err(VersionError::InvalidChar { at: pos }),
        }
    }
    if pos != s.len() {
        return Err(VersionError::InvalidChar { at: pos });
    }
    Ok(Version { major: nums[0], minor: nums[1], patch: nums[2], pre, build })
}

fn numeric_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for (x, y) in a.iter().zip(b) {
        let xn = numeric_id(x);
        let yn = numeric_id(y);
        let c = match (xn, yn) {
            (true, true) => x.len().cmp(&y.len()).then_with(|| x.as_bytes().cmp(y.as_bytes())),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => x.as_bytes().cmp(y.as_bytes()),
        };
        if c != Ordering::Equal {
            return c;
        }
    }
    a.len().cmp(&b.len())
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch)
        .cmp(&(b.major, b.minor, b.patch))
        .then_with(|| cmp_pre(&a.pre, &b.pre))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
    anchor: bool,
}
#[derive(Clone, Copy, Debug)]
enum Op { Eq, Lt, Le, Gt, Ge }
#[derive(Clone, Debug)]
struct Arm {
    comps: Vec<Comparator>,
}
#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

fn ws(b: u8) -> bool { matches!(b, b' ' | b'\t' | b'\r' | b'\n') }

fn core(v: &Version) -> (u64, u64, u64) { (v.major, v.minor, v.patch) }

fn parse_full_operand(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

fn add_comp(arm: &mut Arm, op: Op, version: Version, anchor: bool) {
    arm.comps.push(Comparator { op, version, anchor });
}
fn endpoint(major: u64, minor: u64, patch: u64) -> Version {
    Version { major, minor, patch, pre: vec!["0".into()], build: vec![] }
}
fn plain(major: u64, minor: u64, patch: u64) -> Version {
    Version { major, minor, patch, pre: vec![], build: vec![] }
}
fn inc(v: u64, at: usize) -> Result<u64, RangeError> {
    v.checked_add(1).ok_or(RangeError::Syntax { at })
}

#[derive(Clone, Debug)]
struct Partial {
    nums: Vec<u64>,
    full: Option<Version>,
}
impl Partial {
    fn k(&self) -> usize { self.nums.len() }
    fn lower(&self) -> Version {
        let a = *self.nums.get(0).unwrap_or(&0);
        let b = *self.nums.get(1).unwrap_or(&0);
        let c = *self.nums.get(2).unwrap_or(&0);
        self.full.clone().unwrap_or_else(|| plain(a,b,c))
    }
}
fn parse_partial(s: &str, at: usize) -> Result<Partial, RangeError> {
    if s.contains('-') || s.contains('+') {
        return Ok(Partial { nums: vec![], full: Some(parse_full_operand(s, at)?) });
    }
    let fields: Vec<&str> = s.split('.').collect();
    if fields.len() >= 3 && !fields.iter().any(|x| matches!(*x, "*" | "x" | "X")) {
        return Ok(Partial { nums: vec![], full: Some(parse_full_operand(s, at)?) });
    }
    if fields.is_empty() || fields.len() > 3 || s.is_empty() {
        return Err(RangeError::Syntax { at });
    }
    let mut nums = Vec::new();
    let mut wildcard = false;
    for f in fields {
        if f.is_empty() { return Err(RangeError::Syntax { at }); }
        if matches!(f, "*" | "x" | "X") {
            wildcard = true;
        } else {
            if wildcard || !f.bytes().all(|b| b.is_ascii_digit()) || (f.len() > 1 && f.starts_with('0')) {
                return Err(RangeError::Syntax { at });
            }
            let n = f.parse::<u64>().map_err(|_| RangeError::Syntax { at })?;
            nums.push(n);
        }
    }
    Ok(Partial { nums, full: None })
}

fn parse_token(token: &str, start: usize, arm: &mut Arm) -> Result<(), RangeError> {
    let bytes = token.as_bytes();
    if let Some(op) = if token.starts_with(">=") { Some((Op::Ge,2)) }
        else if token.starts_with("<=") { Some((Op::Le,2)) }
        else if token.starts_with('>') { Some((Op::Gt,1)) }
        else if token.starts_with('<') { Some((Op::Lt,1)) }
        else if token.starts_with('=') { Some((Op::Eq,1)) } else { None } {
        let operand = &token[op.1..];
        let operand_at = start + op.1;
        if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
            return Err(RangeError::Syntax { at: start });
        }
        let v = parse_full_operand(operand, operand_at)?;
        add_comp(arm, op.0, v.clone(), !v.pre.is_empty());
        return Ok(());
    }
    if token.starts_with('^') || token.starts_with('~') {
        let sym = bytes[0];
        let operand = &token[1..];
        let operand_at = start + 1;
        let p = parse_partial(operand, operand_at)?;
        if p.full.is_none() && p.k() == 0 {
            return Err(RangeError::Syntax { at: start });
        }
        let lower = p.lower();
        let k = p.k();
        add_comp(arm, Op::Ge, lower.clone(), !lower.pre.is_empty());
        let (a,b,c) = core(&lower);
        let (ua,ub,uc) = if sym == b'^' {
            if a > 0 || k == 1 { (inc(a,start)?,0,0) }
            else if b > 0 || k == 2 { (a,inc(b,start)?,0) }
            else { (a,b,inc(c,start)?) }
        } else if k == 1 { (inc(a,start)?,0,0) }
        else { (a,inc(b,start)?,0) };
        add_comp(arm, Op::Lt, endpoint(ua,ub,uc), false);
        return Ok(());
    }
    let p = parse_partial(token, start)?;
    if let Some(v) = p.full {
        add_comp(arm, Op::Eq, v.clone(), !v.pre.is_empty());
        return Ok(());
    }
    let k = p.k();
    if k == 0 { return Ok(()); }
    let lower = p.lower();
    add_comp(arm, Op::Ge, lower.clone(), false);
    let (a,b,c) = core(&lower);
    let upper = if k == 1 { endpoint(inc(a,start)?,0,0) }
        else { endpoint(a,inc(b,start)?,0) };
    let _ = c;
    add_comp(arm, Op::Lt, upper, false);
    Ok(())
}

fn token_ranges(s: &str, begin: usize, end: usize) -> Vec<(usize,usize)> {
    let bytes=s.as_bytes(); let mut out=Vec::new(); let mut i=begin;
    while i<end {
        while i<end && ws(bytes[i]) { i+=1; }
        if i>=end { break; }
        let st=i; while i<end && !ws(bytes[i]) { i+=1; }
        out.push((st,i));
    }
    out
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes=s.as_bytes(); let mut arms=Vec::new(); let mut arm_start=0usize; let mut i=0usize;
    let mut separators=Vec::new();
    while i+1<bytes.len() {
        if bytes[i]==b'|' && bytes[i+1]==b'|' && (i==0 || ws(bytes[i-1])) && (i+2==bytes.len() || ws(bytes[i+2])) {
            separators.push(i); i+=2;
        } else { i+=1; }
    }
    let mut bounds=Vec::new(); let mut prev=0;
    for &sp in &separators { bounds.push((prev,sp)); prev=sp+2; }
    bounds.push((prev,s.len()));
    for (bi,(lo,hi)) in bounds.iter().copied().enumerate() {
        let toks=token_ranges(s,lo,hi);
        if toks.is_empty() {
            let at=if bi==bounds.len()-1 && !separators.is_empty() { s.len() } else if bi>0 { separators[bi-1] } else {0};
            return Err(RangeError::Empty { at });
        }
        let mut arm=Arm{comps:vec![]};
        let hyphens:Vec<usize>=toks.iter().filter(|&&(a,b)| &s[a..b]=="-").map(|&(a,_)|a).collect();
        if !hyphens.is_empty() {
            if toks.len()!=3 || &s[toks[1].0..toks[1].1]!="-" {
                return Err(RangeError::Syntax{at:hyphens[0]});
            }
            let (ls,le)=toks[0]; let (us,ue)=toks[2];
            let lp=parse_partial(&s[ls..le],ls)?;
            let up=parse_partial(&s[us..ue],us)?;
            if lp.full.is_none() && lp.k()==0 { return Err(RangeError::Syntax{at:ls}); }
            if up.full.is_none() && up.k()==0 { return Err(RangeError::Syntax{at:us}); }
            let lv=lp.lower();
            add_comp(&mut arm,Op::Ge,lv.clone(),!lv.pre.is_empty());
            if let Some(v)=up.full {
                add_comp(&mut arm,Op::Le,v.clone(),!v.pre.is_empty());
            } else {
                let k=up.k(); let (a,b,_)=core(&up.lower());
                let bound=if k==1 { endpoint(inc(a,us)?,0,0) } else { endpoint(a,inc(b,us)?,0) };
                add_comp(&mut arm,Op::Lt,bound,false);
            }
        } else {
            for (st,en) in toks {
                let tok=&s[st..en];
                let first=tok.as_bytes()[0];
                if !(first.is_ascii_digit() || matches!(first,b'*'|b'x'|b'X'|b'^'|b'~'|b'<'|b'>'|b'=')) {
                    return Err(RangeError::Syntax{at:st});
                }
                parse_token(tok,st,&mut arm)?;
            }
        }
        arms.push(arm);
    }
    Ok(Range{arms})
}

fn cmp_satisfies(v:&Version,c:&Comparator)->bool {
    let ord=compare_versions(v,&c.version);
    match c.op { Op::Eq=>ord==Ordering::Equal, Op::Lt=>ord==Ordering::Less, Op::Le=>ord!=Ordering::Greater, Op::Gt=>ord==Ordering::Greater, Op::Ge=>ord!=Ordering::Less }
}
fn arm_match(v:&Version,a:&Arm,include:bool)->bool {
    if !a.comps.iter().all(|c|cmp_satisfies(v,c)) { return false; }
    if include || v.pre.is_empty() { return true; }
    let t=core(v);
    a.comps.iter().any(|c|c.anchor && !c.version.pre.is_empty() && core(&c.version)==t)
}
pub fn matches_with(v:&Version,r:&Range,include_prerelease:bool)->bool {
    r.arms.iter().any(|a|arm_match(v,a,include_prerelease))
}
pub fn matches(v:&Version,r:&Range)->bool { matches_with(v,r,false) }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError { pub index: usize, pub error: VersionError }

fn satisfying(candidates:&[String],r:&Range,max:bool)->Result<Option<String>,CandidateError> {
    let mut parsed=Vec::with_capacity(candidates.len());
    for (i,s) in candidates.iter().enumerate() {
        let v=parse_version(s).map_err(|error|CandidateError{index:i,error})?;
        parsed.push(v);
    }
    let mut best:Option<usize>=None;
    for i in 0..parsed.len() {
        if !matches(&parsed[i],r) { continue; }
        if let Some(j)=best {
            let ord=compare_versions(&parsed[i],&parsed[j]);
            if (max && ord==Ordering::Greater) || (!max && ord==Ordering::Less) { best=Some(i); }
        } else { best=Some(i); }
    }
    Ok(best.map(|i|candidates[i].clone()))
}
pub fn max_satisfying(candidates:&[String],r:&Range)->Result<Option<String>,CandidateError>{satisfying(candidates,r,true)}
pub fn min_satisfying(candidates:&[String],r:&Range)->Result<Option<String>,CandidateError>{satisfying(candidates,r,false)}

fn next_core(t:(u64,u64,u64))->Option<(u64,u64,u64)> {
    let (a,b,c)=t;
    if c<u64::MAX {Some((a,b,c+1))}
    else if b<u64::MAX {Some((a,b+1,0))}
    else if a<u64::MAX {Some((a+1,0,0))}
    else {None}
}
fn corever(t:(u64,u64,u64), pre:Vec<String>)->Version {
    Version{major:t.0,minor:t.1,patch:t.2,pre,build:vec![]}
}
fn admitted(a:&Arm,t:(u64,u64,u64))->bool {
    a.comps.iter().any(|c|c.anchor && !c.version.pre.is_empty() && core(&c.version)==t)
}
fn stable_in_interval(a:&Arm, lower:&Option<(Version,bool)>, upper:&Option<(Version,bool)>)->bool {
    let mut t=(0,0,0);
    if let Some((v,inc))=lower {
        let ct=core(v);
        t=ct;
        if !v.pre.is_empty() || !inc {
            if let Some(nt)=next_core(ct) { t=nt; } else { return false; }
        }
    }
    if let Some((v,inc))=upper {
        let cmp=corever(t,vec![]);
        let o=compare_versions(&cmp,v);
        if o==Ordering::Greater || (o==Ordering::Equal && !*inc) { return false; }
    }
    let _=a;
    true
}
fn arm_bounds(a:&Arm)->Option<(Option<(Version,bool)>,Option<(Version,bool)>,Option<Version>)> {
    let mut lo:Option<(Version,bool)>=None; let mut hi:Option<(Version,bool)>=None; let mut eq=None;
    for c in &a.comps {
        match c.op {
            Op::Eq => { eq=Some(c.version.clone()); }
            Op::Gt|Op::Ge => {
                let inc=c.op==Op::Ge;
                if let Some((v,oldinc))=&lo {
                    let o=compare_versions(&c.version,v);
                    if o==Ordering::Greater || (o==Ordering::Equal && !inc && *oldinc) {lo=Some((c.version.clone(),inc));}
                } else {lo=Some((c.version.clone(),inc));}
            }
            Op::Lt|Op::Le => {
                let inc=c.op==Op::Le;
                if let Some((v,oldinc))=&hi {
                    let o=compare_versions(&c.version,v);
                    if o==Ordering::Less || (o==Ordering::Equal && !inc && *oldinc) {hi=Some((c.version.clone(),inc));}
                } else {hi=Some((c.version.clone(),inc));}
            }
        }
    }
    if let Some(v)=eq {
        lo=Some((v.clone(),true)); hi=Some((v.clone(),true));
    }
    Some((lo,hi,None))
}
fn arm_has_witness(a:&Arm, include:bool)->bool {
    let (lo,hi,_)=arm_bounds(a).unwrap();
    if stable_in_interval(a,&lo,&hi) { return true; }
    if include {
        // Search for an admitted prerelease tuple among interval endpoints and adjacent cores.
        for c in &a.comps {
            let t=core(&c.version);
            if !c.version.pre.is_empty() && admitted(a,t) {
                let v=corever(t,vec!["0".into()]);
                if a.comps.iter().all(|x|cmp_satisfies(&v,x)) { return true; }
            }
        }
        return false;
    }
    let mut seen=Vec::new();
    for c in &a.comps { if c.anchor && !c.version.pre.is_empty() { let t=core(&c.version); if !seen.contains(&t){seen.push(t);} } }
    seen.into_iter().any(|t| {
        let v=corever(t,vec!["0".into()]);
        a.comps.iter().all(|c|cmp_satisfies(&v,c))
    })
}
pub fn intersects(a:&Range,b:&Range,include_prerelease:bool)->bool {
    for x in &a.arms {
        for y in &b.arms {
            let mut z=Arm{comps:x.comps.clone()};
            z.comps.extend(y.comps.clone());
            if arm_has_witness(&z,include_prerelease) { return true; }
        }
    }
    false
}

fn strip_build(v:&Version)->String {
    let mut s=format!("{}.{}.{}",v.major,v.minor,v.patch);
    if !v.pre.is_empty(){s.push('-');s.push_str(&v.pre.join("."));}
    s
}
fn canonical_arm(a:&Arm)->Option<String> {
    if !arm_has_witness(a,false) { return None; }
    let (lo,hi,_)=arm_bounds(a).unwrap();
    let lower=lo.map(|(v,inc)|format!("{}{}",if inc{">="}else{">"},strip_build(&v))).unwrap_or("*".into());
    let upper=hi.map(|(v,inc)|format!("{}{}",if inc{"<="}else{"<"},strip_build(&v))).unwrap_or("*".into());
    let mut anchors:Vec<_>=a.comps.iter().filter(|c|c.anchor&&!c.version.pre.is_empty()).map(|c|core(&c.version)).collect();
    anchors.sort(); anchors.dedup();
    let anch=if anchors.is_empty(){"-".into()}else{anchors.iter().map(|(a,b,c)|format!("{a}.{b}.{c}")).collect::<Vec<_>>().join(",")};
    Some(format!("[{lower};{upper};pre={anch}]"))
}
pub fn canonical(r:&Range)->String {
    let mut arms:Vec<String>=r.arms.iter().filter_map(canonical_arm).collect();
    arms.sort(); arms.dedup();
    if arms.is_empty(){"EMPTY".into()}else{arms.join(" || ")}
}