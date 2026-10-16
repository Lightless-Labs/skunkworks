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

fn is_num(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn parse_ids(s: &str, start: usize, is_pre: bool) -> Result<Vec<String>, VersionError> {
    let mut out = Vec::new();
    let mut base = 0;
    for part in s.split('.') {
        if part.is_empty() {
            return Err(VersionError::EmptyIdentifier { at: start + base });
        }
        for (i, b) in part.bytes().enumerate() {
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: start + base + i });
            }
        }
        if is_pre && is_num(part) && part.len() > 1 && part.starts_with('0') {
            return Err(VersionError::LeadingZero { at: start + base });
        }
        out.push(part.to_owned());
        base += part.len() + 1;
    }
    Ok(out)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let bytes = s.as_bytes();
    let mut pos = 0;
    let mut core = [0u64; 3];
    for (field, slot) in core.iter_mut().enumerate() {
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
        let raw = &s[start..pos];
        if raw.len() > 1 && raw.starts_with('0') {
            return Err(VersionError::LeadingZero { at: start });
        }
        *slot = raw.parse().map_err(|_| VersionError::Overflow { at: start })?;
        if field < 2 {
            if pos < bytes.len() && bytes[pos] == b'.' {
                pos += 1;
            } else if pos == bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            } else {
                return Err(VersionError::InvalidChar { at: pos });
            }
        }
    }
    let mut pre = Vec::new();
    let mut build = Vec::new();
    if pos < bytes.len() {
        if bytes[pos] == b'-' {
            let pstart = pos + 1;
            pos = s[pstart..].find('+').map(|i| pstart + i).unwrap_or(bytes.len());
            pre = parse_ids(&s[pstart..pos], pstart, true)?;
        }
        if pos < bytes.len() {
            if bytes[pos] != b'+' {
                return Err(VersionError::InvalidChar { at: pos });
            }
            let bstart = pos + 1;
            build = parse_ids(&s[bstart..], bstart, false)?;
        }
    }
    Ok(Version { major: core[0], minor: core[1], patch: core[2], pre, build })
}

fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for (x, y) in a.iter().zip(b) {
        let xn = is_num(x);
        let yn = is_num(y);
        let c = match (xn, yn) {
            (true, true) => x.len().cmp(&y.len()).then_with(|| x.as_bytes().cmp(y.as_bytes())),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => x.as_bytes().cmp(y.as_bytes()),
        };
        if c != Ordering::Equal { return c; }
    }
    a.len().cmp(&b.len())
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch))
        .then_with(|| cmp_pre(&a.pre, &b.pre))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug)]
struct Bound {
    v: Version,
    inclusive: bool,
    lower: bool,
}

#[derive(Clone, Debug)]
struct Cmp {
    op: Op,
    v: Version,
    explicit_anchor: bool,
}

#[derive(Clone, Copy, Debug)]
enum Op { Eq, Lt, Le, Gt, Ge }

#[derive(Clone, Debug)]
struct Arm {
    cmps: Vec<Cmp>,
    anchors: Vec<(u64,u64,u64)>,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

fn core(v: &Version) -> (u64,u64,u64) { (v.major,v.minor,v.patch) }
fn endpoint(a: u64,b: u64,c: u64) -> Version {
    Version { major:a, minor:b, patch:c, pre:Vec::new(), build:Vec::new() }
}
fn version_string(v: &Version) -> String {
    let mut s=format!("{}.{}.{}",v.major,v.minor,v.patch);
    if !v.pre.is_empty() { s.push('-'); s.push_str(&v.pre.join(".")); }
    s
}
fn fail_syntax(at: usize) -> RangeError { RangeError::Syntax { at } }
fn bump(v: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let (mut a,mut b,mut c)=core(v);
    match field {
        0 => { a=a.checked_add(1).ok_or_else(||fail_syntax(at))?; b=0;c=0; }
        1 => { b=b.checked_add(1).ok_or_else(||fail_syntax(at))?; c=0; }
        _ => { c=c.checked_add(1).ok_or_else(||fail_syntax(at))?; }
    }
    Ok(endpoint(a,b,c))
}

fn full_operand(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

#[derive(Clone)]
struct Partial { nums: Vec<u64>, full: Option<Version> }

fn parse_partial(s: &str, at: usize) -> Result<Partial, RangeError> {
    if s.contains('-') || s.contains('+') {
        return Ok(Partial { nums: Vec::new(), full: Some(full_operand(s, at)?) });
    }
    let fields: Vec<&str> = s.split('.').collect();
    if fields.len() >= 3 && !fields.iter().any(|x| matches!(*x, "*"|"x"|"X")) {
        return Ok(Partial { nums: Vec::new(), full: Some(full_operand(s, at)?) });
    }
    if fields.len() > 3 || fields.iter().any(|x| x.is_empty()) {
        return Err(fail_syntax(at));
    }
    let mut nums=Vec::new();
    let mut wildcard=false;
    for f in fields {
        if matches!(f,"*"|"x"|"X") { wildcard=true; }
        else {
            if wildcard || !f.bytes().all(|b| b.is_ascii_digit()) || (f.len()>1 && f.starts_with('0')) {
                return Err(fail_syntax(at));
            }
            let n=f.parse::<u64>().map_err(|_|fail_syntax(at))?;
            nums.push(n);
        }
    }
    Ok(Partial { nums, full: None })
}

fn padded(p: &Partial) -> Version {
    endpoint(*p.nums.get(0).unwrap_or(&0), *p.nums.get(1).unwrap_or(&0), *p.nums.get(2).unwrap_or(&0))
}
fn add_cmp(arm: &mut Arm, op: Op, v: Version, explicit_anchor: bool) {
    if explicit_anchor && !v.pre.is_empty() { arm.anchors.push(core(&v)); }
    arm.cmps.push(Cmp { op, v, explicit_anchor });
}
fn expand_bare(p: Partial, arm: &mut Arm, at: usize) -> Result<(), RangeError> {
    if let Some(v)=p.full { add_cmp(arm,Op::Eq,v,true); return Ok(()); }
    let k=p.nums.len();
    if k==0 { return Ok(()); }
    let low=padded(&p);
    add_cmp(arm,Op::Ge,low.clone(),false);
    if k<3 {
        add_cmp(arm,Op::Lt,bump(&low,k-1,at)?,false);
    }
    Ok(())
}
fn expand_caret_tilde(p: Partial, caret: bool, arm: &mut Arm, at: usize) -> Result<(), RangeError> {
    if p.full.is_none() && p.nums.is_empty() { return Err(fail_syntax(at)); }
    let (low,k)=if let Some(v)=p.full { (v,3) } else { (padded(&p),p.nums.len()) };
    add_cmp(arm,Op::Ge,low.clone(),true);
    let field=if caret {
        if low.major>0 || k==1 { 0 } else if low.minor>0 || k==2 { 1 } else { 2 }
    } else if k==1 {0} else {1};
    add_cmp(arm,Op::Lt,bump(&low,field,at)?,false);
    Ok(())
}
fn op_matches(op: Op, v: &Version, bound: &Version) -> bool {
    match op {
        Op::Eq => compare_versions(v,bound)==Ordering::Equal,
        Op::Lt => compare_versions(v,bound)==Ordering::Less,
        Op::Le => compare_versions(v,bound)!=Ordering::Greater,
        Op::Gt => compare_versions(v,bound)==Ordering::Greater,
        Op::Ge => compare_versions(v,bound)!=Ordering::Less,
    }
}
fn arm_matches(v: &Version, arm: &Arm, include: bool) -> bool {
    if !include && !v.pre.is_empty() && !arm.anchors.contains(&core(v)) { return false; }
    arm.cmps.iter().all(|c| op_matches(c.op,v,&c.v))
}
pub fn matches_with(v: &Version, r: &Range, include_prerelease: bool) -> bool {
    r.arms.iter().any(|a| arm_matches(v,a,include_prerelease))
}
pub fn matches(v: &Version, r: &Range) -> bool { matches_with(v,r,false) }

#[derive(Clone)]
struct Tok<'a> { s: &'a str, start: usize }
fn is_ws(b:u8)->bool { matches!(b,b' '|b'\t'|b'\r'|b'\n') }
fn tokenize(s:&str)->Vec<Tok<'_>> {
    let b=s.as_bytes(); let mut i=0; let mut v=Vec::new();
    while i<b.len() {
        while i<b.len() && is_ws(b[i]) {i+=1;}
        if i==b.len(){break}
        let st=i; while i<b.len() && !is_ws(b[i]) {i+=1;}
        v.push(Tok{s:&s[st..i],start:st});
    }
    v
}

fn parse_full_cmp(tok: &Tok<'_>, arm:&mut Arm)->Result<(),RangeError> {
    let bytes=tok.s.as_bytes();
    let (op,n)=if tok.s.starts_with(">="){(Op::Ge,2)} else if tok.s.starts_with("<="){(Op::Le,2)}
    else if tok.s.starts_with(">"){(Op::Gt,1)} else if tok.s.starts_with("<"){(Op::Lt,1)}
    else if tok.s.starts_with("="){(Op::Eq,1)} else {(Op::Eq,0)};
    let operand=&tok.s[n..]; let at=tok.start+n;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {return Err(fail_syntax(tok.start));}
    let v=full_operand(operand,at)?;
    add_cmp(arm,op,v,true);
    let _=bytes;
    Ok(())
}

fn parse_arm(tokens:&[Tok<'_>])->Result<Arm,RangeError>{
    let mut arm=Arm{cmps:Vec::new(),anchors:Vec::new()};
    if let Some(pos)=tokens.iter().position(|t|t.s=="-") {
        if tokens.len()!=3 || pos!=1 {return Err(fail_syntax(tokens[pos].start));}
        let lo=parse_partial(tokens[0].s,tokens[0].start)?;
        let hi=parse_partial(tokens[2].s,tokens[2].start)?;
        if lo.full.is_none() && lo.nums.is_empty(){return Err(fail_syntax(tokens[0].start));}
        if hi.full.is_none() && hi.nums.is_empty(){return Err(fail_syntax(tokens[2].start));}
        let lv=lo.full.clone().unwrap_or_else(||padded(&lo));
        add_cmp(&mut arm,Op::Ge,lv,true);
        if let Some(h)=hi.full { add_cmp(&mut arm,Op::Le,h,true); }
        else {
            let k=hi.nums.len(); let upper=bump(&padded(&hi),k-1,tokens[2].start)?;
            add_cmp(&mut arm,Op::Lt,upper,false);
        }
        return Ok(arm)
    }
    for t in tokens {
        if t.s=="||" { return Err(fail_syntax(t.start)); }
        let b=t.s.as_bytes();
        match b.first().copied() {
            Some(b'<')|Some(b'>')|Some(b'=') => parse_full_cmp(t,&mut arm)?,
            Some(b'^')|Some(b'~') => {
                let p=parse_partial(&t.s[1..],t.start+1)?;
                expand_caret_tilde(p,b[0]==b'^',&mut arm,t.start+1)?;
            }
            Some(b'-')|Some(b',')|Some(b'(')|Some(b')')|Some(b'&') => return Err(fail_syntax(t.start)),
            Some(c) if c.is_ascii_digit() || matches!(c,b'*'|b'x'|b'X') => {
                let p=parse_partial(t.s,t.start)?;
                expand_bare(p,&mut arm,t.start)?;
            }
            _ => return Err(fail_syntax(t.start)),
        }
    }
    Ok(arm)
}

pub fn parse_range(s:&str)->Result<Range,RangeError>{
    let tokens=tokenize(s);
    if tokens.is_empty(){return Err(RangeError::Empty{at:0});}
    let mut arms=Vec::new();
    let mut current=Vec::new();
    let mut i=0;
    while i<tokens.len() {
        if tokens[i].s=="||" {
            if current.is_empty(){return Err(RangeError::Empty{at:tokens[i].start});}
            arms.push(parse_arm(&current)?);
            current.clear();
        } else { current.push(tokens[i].clone()); }
        i+=1;
    }
    if current.is_empty() {
        let at=if let Some(last)=tokens.last(){s.len()} else {0};
        return Err(RangeError::Empty{at});
    }
    arms.push(parse_arm(&current)?);
    Ok(Range{arms})
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError { pub index: usize, pub error: VersionError }

pub fn max_satisfying(candidates:&[String], r:&Range)->Result<Option<String>,CandidateError>{
    extrema(candidates,r,true)
}
pub fn min_satisfying(candidates:&[String], r:&Range)->Result<Option<String>,CandidateError>{
    extrema(candidates,r,false)
}
fn extrema(candidates:&[String],r:&Range,max:bool)->Result<Option<String>,CandidateError>{
    let mut parsed=Vec::new();
    for (i,s) in candidates.iter().enumerate(){
        let v=parse_version(s).map_err(|error|CandidateError{index:i,error})?;
        parsed.push(v);
    }
    let mut best:Option<usize>=None;
    for i in 0..parsed.len(){
        if matches(&parsed[i],r) {
            if let Some(j)=best {
                let c=compare_versions(&parsed[i],&parsed[j]);
                if (max && c==Ordering::Greater)||(!max && c==Ordering::Less){best=Some(i);}
            } else {best=Some(i);}
        }
    }
    Ok(best.map(|i|candidates[i].clone()))
}

fn bounds(arm:&Arm)->(Option<(Version,bool)>,Option<(Version,bool)>) {
    let mut lo:Option<(Version,bool)>=None; let mut hi:Option<(Version,bool)>=None;
    for c in &arm.cmps {
        let (lower,upper)=match c.op {
            Op::Eq => (Some((c.v.clone(),true)),Some((c.v.clone(),true))),
            Op::Gt => (Some((c.v.clone(),false)),None),
            Op::Ge => (Some((c.v.clone(),true)),None),
            Op::Lt => (None,Some((c.v.clone(),false))),
            Op::Le => (None,Some((c.v.clone(),true))),
        };
        if let Some(x)=lower {
            if lo.as_ref().map_or(true,|(v,inc)| compare_versions(&x.0,v)==Ordering::Greater || (compare_versions(&x.0,v)==Ordering::Equal && !x.1 && *inc)){lo=Some(x);}
        }
        if let Some(x)=upper {
            if hi.as_ref().map_or(true,|(v,inc)| compare_versions(&x.0,v)==Ordering::Less || (compare_versions(&x.0,v)==Ordering::Equal && !x.1 && *inc)){hi=Some(x);}
        }
    }
    (lo,hi)
}

fn tuple_next(t:(u64,u64,u64))->Option<(u64,u64,u64)> {
    let (a,b,c)=t;
    if c<u64::MAX {Some((a,b,c+1))}
    else if b<u64::MAX {Some((a,b+1,0))}
    else if a<u64::MAX {Some((a+1,0,0))}
    else {None}
}
fn arm_possible(arm:&Arm,include:bool)->bool {
    let (lo,hi)=bounds(arm);
    let anchors=&arm.anchors;
    if include { return interval_has_witness(lo.as_ref(),hi.as_ref(),None); }
    if interval_has_witness(lo.as_ref(),hi.as_ref(),None) { return true; }
    anchors.iter().any(|&t| {
        let v=endpoint(t.0,t.1,t.2);
        // Some prerelease at the anchored tuple lies in bounds.
        let mut p=v.clone(); p.pre=vec!["0".into()];
        lower_ok(&p,lo.as_ref()) && upper_ok(&p,hi.as_ref())
    })
}
fn lower_ok(v:&Version,b:Option<&(Version,bool)>)->bool {
    b.map_or(true,|(x,i)| {let c=compare_versions(v,x); c==Ordering::Greater || (*i&&c==Ordering::Equal)})
}
fn upper_ok(v:&Version,b:Option<&(Version,bool)>)->bool {
    b.map_or(true,|(x,i)| {let c=compare_versions(v,x); c==Ordering::Less || (*i&&c==Ordering::Equal)})
}
fn interval_has_witness(lo:Option<&(Version,bool)>,hi:Option<&(Version,bool)>,_:Option<()>) -> bool {
    if let (Some((l,li)),Some((h,hi_inc)))=(lo,hi) {
        let c=compare_versions(l,h);
        if c==Ordering::Greater || (c==Ordering::Equal && !(*li&&*hi_inc)){return false;}
        if c==Ordering::Equal{return true;}
        if *li && *hi_inc {return true;}
        // A strict interval can contain a stable tuple after its lower tuple.
        return tuple_next(core(l)).map_or(false,|t| {
            let v=endpoint(t.0,t.1,t.2);
            lower_ok(&v,lo) && upper_ok(&v,hi)
        });
    }
    true
}
pub fn intersects(a:&Range,b:&Range,include_prerelease:bool)->bool{
    for x in &a.arms { for y in &b.arms {
        let mut z=Arm{cmps:x.cmps.clone(),anchors:x.anchors.clone()};
        z.cmps.extend(y.cmps.clone());
        let both:Vec<(u64,u64,u64)>=x.anchors.iter().copied().filter(|t|y.anchors.contains(t)).collect();
        z.anchors=both;
        if arm_possible(&z,include_prerelease){return true;}
    }}
    false
}

pub fn canonical(r:&Range)->String{
    let mut strings=Vec::new();
    for arm in &r.arms {
        if !arm_possible(arm,false){continue;}
        let (lo,hi)=bounds(arm);
        let lower=lo.map(|(v,i)|format!("{}{}",if i{">="}else{">"},version_string(&v)));
        let upper=hi.map(|(v,i)|format!("{}{}",if i{"<="}else{"<"},version_string(&v)));
        let mut anchors=arm.anchors.clone();
        anchors.sort();
        anchors.dedup();
        let astr=if anchors.is_empty(){"-".to_owned()} else {anchors.iter().map(|(a,b,c)|format!("{a}.{b}.{c}")).collect::<Vec<_>>().join(",")};
        strings.push(format!("[{};{};pre={}]",lower.unwrap_or_else(||"*".into()),upper.unwrap_or_else(||"*".into()),astr));
    }
    strings.sort_by(|a,b|a.as_bytes().cmp(b.as_bytes()));
    strings.dedup();
    if strings.is_empty(){"EMPTY".into()} else {strings.join(" || ")}
}