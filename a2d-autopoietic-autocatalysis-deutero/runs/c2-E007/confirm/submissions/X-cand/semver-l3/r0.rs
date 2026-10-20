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

fn bytes(s: &str) -> &[u8] { s.as_bytes() }
fn is_digit(b: u8) -> bool { b.is_ascii_digit() }
fn is_ident_byte(b: u8) -> bool { b.is_ascii_alphanumeric() || b == b'-' }

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() { return Err(VersionError::Empty); }
    let b = bytes(s);
    let mut p = 0usize;
    let mut nums = [0u64; 3];
    for i in 0..3 {
        let start = p;
        while p < b.len() && is_digit(b[p]) { p += 1; }
        if p == start {
            if p == b.len() || matches!(b[p], b'.' | b'-' | b'+') {
                return Err(VersionError::MissingCore { at: p });
            }
            return Err(VersionError::InvalidChar { at: p });
        }
        if p - start > 1 && b[start] == b'0' { return Err(VersionError::LeadingZero { at: start }); }
        let mut n = 0u64;
        for &d in &b[start..p] {
            n = n.checked_mul(10).and_then(|x| x.checked_add((d - b'0') as u64))
                .ok_or(VersionError::Overflow { at: start })?;
        }
        nums[i] = n;
        if i < 2 {
            if p >= b.len() || b[p] != b'.' {
                if p == b.len() || matches!(b[p], b'-' | b'+') {
                    return Err(VersionError::MissingCore { at: p });
                }
                return Err(VersionError::InvalidChar { at: p });
            }
            p += 1;
        }
    }
    if p < b.len() && !matches!(b[p], b'-' | b'+') { return Err(VersionError::InvalidChar { at: p }); }

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if p < b.len() && b[p] == b'-' {
        let start = p + 1;
        let plus = b[start..].iter().position(|x| *x == b'+').map(|x| start + x);
        let end = plus.unwrap_or(b.len());
        pre = parse_identifiers(s, start, end, true)?;
        p = end;
    }
    if p < b.len() && b[p] == b'+' {
        build = parse_identifiers(s, p + 1, b.len(), false)?;
        p = b.len();
    }
    if p != b.len() { return Err(VersionError::InvalidChar { at: p }); }
    Ok(Version { major: nums[0], minor: nums[1], patch: nums[2], pre, build })
}

fn parse_identifiers(s: &str, start: usize, end: usize, prerelease: bool) -> Result<Vec<String>, VersionError> {
    let b = bytes(s);
    let mut out = Vec::new();
    let mut p = start;
    loop {
        let q = b[p..end].iter().position(|x| *x == b'.').map(|x| p + x).unwrap_or(end);
        if q == p { return Err(VersionError::EmptyIdentifier { at: p }); }
        for i in p..q {
            if !is_ident_byte(b[i]) { return Err(VersionError::InvalidChar { at: i }); }
        }
        if prerelease && q - p > 1 && b[p] == b'0' && b[p..q].iter().all(|x| is_digit(*x)) {
            return Err(VersionError::LeadingZero { at: p });
        }
        out.push(s[p..q].to_string());
        if q == end { break; }
        p = q + 1;
        if p == end { return Err(VersionError::EmptyIdentifier { at: p }); }
    }
    Ok(out)
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch))
        .then_with(|| compare_pre(&a.pre, &b.pre))
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for (x, y) in a.iter().zip(b) {
        let xd = x.bytes().all(|c| c.is_ascii_digit());
        let yd = y.bytes().all(|c| c.is_ascii_digit());
        let ord = match (xd, yd) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (true, true) => x.len().cmp(&y.len()).then_with(|| x.as_bytes().cmp(y.as_bytes())),
            (false, false) => x.as_bytes().cmp(y.as_bytes()),
        };
        if ord != Ordering::Equal { return ord; }
    }
    a.len().cmp(&b.len())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug)]
enum Cmp { Lt, Le, Eq, Ge, Gt }

#[derive(Clone, Debug)]
struct Constraint { op: Cmp, v: Version, anchor: bool }

#[derive(Clone, Debug)]
pub struct Range { arms: Vec<Vec<Constraint>> }

fn cmp_matches(v: &Version, c: &Constraint) -> bool {
    let o = compare_versions(v, &c.v);
    match c.op { Cmp::Lt => o == Ordering::Less, Cmp::Le => o != Ordering::Greater,
        Cmp::Eq => o == Ordering::Equal, Cmp::Ge => o != Ordering::Less, Cmp::Gt => o == Ordering::Greater }
}
fn parse_full_at(s: &str, start: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at: start, error })
}

fn whitespace(b: u8) -> bool { matches!(b, b' ' | b'\t' | b'\r' | b'\n') }
fn tokenize(s: &str) -> Vec<(usize, &str)> {
    let b = s.as_bytes(); let mut out = Vec::new(); let mut p = 0;
    while p < b.len() {
        while p < b.len() && whitespace(b[p]) { p += 1; }
        if p == b.len() { break; }
        let st = p; while p < b.len() && !whitespace(b[p]) { p += 1; }
        out.push((st, &s[st..p]));
    }
    out
}
fn tuple_version(t: (u64,u64,u64), pre: Vec<String>) -> Version {
    Version { major:t.0, minor:t.1, patch:t.2, pre, build:Vec::new() }
}
fn bump(t: (u64,u64,u64), field: usize, at: usize) -> Result<(u64,u64,u64), RangeError> {
    let mut a = [t.0,t.1,t.2];
    a[field] = a[field].checked_add(1).ok_or(RangeError::Syntax { at })?;
    for x in a.iter_mut().skip(field+1) { *x=0; }
    Ok((a[0],a[1],a[2]))
}

#[derive(Clone)]
struct Partial { nums: Vec<u64>, full: Option<Version>, k: usize }

fn partial_operand(s: &str, at: usize) -> Result<Partial, RangeError> {
    let b=s.as_bytes();
    let has_suffix=b.contains(&b'-') || b.contains(&b'+');
    let fields: Vec<&str> = s.split('.').collect();
    let no_wild = !fields.iter().any(|x| matches!(*x,"*"|"x"|"X"));
    if has_suffix || (fields.len() >= 3 && no_wild) {
        let v=parse_full_at(s,at)?;
        return Ok(Partial { k:3, nums:vec![v.major,v.minor,v.patch], full:Some(v) });
    }
    if fields.len()>3 { return Err(RangeError::Syntax{at}); }
    let mut nums=Vec::new();
    let mut wild=false;
    for f in fields {
        if f.is_empty() { return Err(RangeError::Syntax{at}); }
        if matches!(f,"*"|"x"|"X") { wild=true; continue; }
        if wild || !f.bytes().all(is_digit) || (f.len()>1 && f.starts_with('0')) {
            return Err(RangeError::Syntax{at});
        }
        let n=f.parse::<u64>().map_err(|_|RangeError::Syntax{at})?;
        nums.push(n);
    }
    Ok(Partial{k:nums.len(),nums,full:None})
}
fn padded(p:&Partial)->(u64,u64,u64) {
    let mut a=[0;3]; for (i,n) in p.nums.iter().enumerate().take(3){a[i]=*n;}
    (a[0],a[1],a[2])
}
fn add_constraint(out:&mut Vec<Constraint>, op:Cmp, v:Version, anchor:bool) {
    out.push(Constraint{op,v,anchor});
}
fn expand_bare(s:&str, at:usize, out:&mut Vec<Constraint>)->Result<(),RangeError>{
    let p=partial_operand(s,at)?;
    if let Some(v)=p.full { add_constraint(out,Cmp::Eq,v.clone(),!v.pre.is_empty()); return Ok(()); }
    let t=padded(&p);
    if p.k==0{return Ok(())}
    add_constraint(out,Cmp::Ge,tuple_version(t,Vec::new()),false);
    if p.k<3 {
        let upper=bump(t,p.k-1,at)?;
        add_constraint(out,Cmp::Lt,tuple_version(upper,vec!["0".into()]),false);
    }
    Ok(())
}
fn expand_caret_tilde(s:&str, at:usize, caret:bool, out:&mut Vec<Constraint>)->Result<(),RangeError>{
    let p=partial_operand(s,at)?;
    if p.k==0{return Err(RangeError::Syntax{at:at.saturating_sub(1)})}
    let t=padded(&p);
    let lower=p.full.clone().unwrap_or_else(||tuple_version(t,Vec::new()));
    let anch=!lower.pre.is_empty();
    add_constraint(out,Cmp::Ge,lower,anch);
    let k=p.k;
    let field=if caret {
        if t.0>0 || k==1 {0} else if t.1>0 || k==2 {1} else {2}
    } else if k==1 {0} else {1};
    let upper=bump(t,field,at)?;
    add_constraint(out,Cmp::Lt,tuple_version(upper,vec!["0".into()]),false);
    Ok(())
}
fn parse_hyphen(tokens:&[(usize,&str)], out:&mut Vec<Constraint>)->Result<(),RangeError>{
    let minus=tokens.iter().find(|x|x.1=="-").unwrap().0;
    if tokens.len()!=3 || tokens[1].1!="-" { return Err(RangeError::Syntax{at:minus}); }
    let (la,ls)=tokens[0]; let (ua,us)=tokens[2];
    let l=partial_operand(ls,la)?; let u=partial_operand(us,ua)?;
    if l.k==0{return Err(RangeError::Syntax{at:la})}
    if u.k==0{return Err(RangeError::Syntax{at:ua})}
    let lv=l.full.clone().unwrap_or_else(||tuple_version(padded(&l),Vec::new()));
    add_constraint(out,Cmp::Ge,lv.clone(),!lv.pre.is_empty());
    if let Some(uv)=u.full {
        add_constraint(out,Cmp::Le,uv.clone(),!uv.pre.is_empty());
    } else {
        let t=padded(&u);
        let upper=bump(t,if u.k==1 {0}else{1},ua)?;
        add_constraint(out,Cmp::Lt,tuple_version(upper,vec!["0".into()]),false);
    }
    Ok(())
}
fn parse_arm(tokens:&[(usize,&str)], out:&mut Vec<Constraint>)->Result<(),RangeError>{
    if tokens.iter().any(|x|x.1=="-"){return parse_hyphen(tokens,out)}
    for &(at,tok) in tokens {
        let b=tok.as_bytes();
        if tok.starts_with('^') || tok.starts_with('~') {
            if tok.len()==1{return Err(RangeError::Syntax{at})}
            expand_caret_tilde(&tok[1..],at+1,tok.starts_with('^'),out)?;
        } else if matches!(tok,"*"|"x"|"X") || b.first().map(|x|is_digit(*x)).unwrap_or(false) {
            expand_bare(tok,at,out)?;
        } else {
            let (op,n)=if tok.starts_with(">="){(Cmp::Ge,2)} else if tok.starts_with("<="){(Cmp::Le,2)}
                else if tok.starts_with('>'){(Cmp::Gt,1)} else if tok.starts_with('<'){(Cmp::Lt,1)}
                else if tok.starts_with('='){(Cmp::Eq,1)} else {return Err(RangeError::Syntax{at})};
            if tok.len()<=n || !tok.as_bytes()[n].is_ascii_digit(){return Err(RangeError::Syntax{at})}
            let v=parse_full_at(&tok[n..],at+n)?;
            add_constraint(out,op,v.clone(),!v.pre.is_empty());
        }
    }
    Ok(())
}
pub fn parse_range(s:&str)->Result<Range,RangeError>{
    let toks=tokenize(s);
    if toks.is_empty(){return Err(RangeError::Empty{at:0})}
    let mut arms=Vec::new(); let mut current=Vec::new(); let mut i=0;
    while i<toks.len(){
        if toks[i].1=="||" {
            if current.is_empty(){return Err(RangeError::Empty{at:toks[i].0})}
            let mut arm=Vec::new(); parse_arm(&current,&mut arm)?; arms.push(arm); current.clear();
            i+=1;
            if i==toks.len(){return Err(RangeError::Empty{at:s.len()})}
        } else { current.push(toks[i]); i+=1; }
    }
    if current.is_empty(){return Err(RangeError::Empty{at:s.len()})}
    let mut arm=Vec::new();parse_arm(&current,&mut arm)?;arms.push(arm);
    Ok(Range{arms})
}
fn arm_match(v:&Version,arm:&[Constraint],include:bool)->bool{
    if !arm.iter().all(|c|cmp_matches(v,c)){return false}
    include || v.pre.is_empty() || arm.iter().any(|c|c.anchor && !c.v.pre.is_empty() &&
        (c.v.major,c.v.minor,c.v.patch)==(v.major,v.minor,v.patch))
}
pub fn matches_with(v:&Version,r:&Range,include_prerelease:bool)->bool{
    r.arms.iter().any(|a|arm_match(v,a,include_prerelease))
}
pub fn matches(v:&Version,r:&Range)->bool{matches_with(v,r,false)}

#[derive(Clone,Debug,PartialEq,Eq)]
pub struct CandidateError{pub index:usize,pub error:VersionError}
fn satisfying(candidates:&[String],r:&Range,max:bool)->Result<Option<String>,CandidateError>{
    let mut parsed=Vec::new();
    for (i,s) in candidates.iter().enumerate(){
        let v=parse_version(s).map_err(|error|CandidateError{index:i,error})?;
        parsed.push(v);
    }
    let mut best:Option<usize>=None;
    for i in 0..parsed.len(){
        if !matches(&parsed[i],r){continue}
        if let Some(j)=best {
            let o=compare_versions(&parsed[i],&parsed[j]);
            if (max && o==Ordering::Greater)||(!max&&o==Ordering::Less){best=Some(i)}
        }else{best=Some(i)}
    }
    Ok(best.map(|i|candidates[i].clone()))
}
pub fn max_satisfying(candidates:&[String],r:&Range)->Result<Option<String>,CandidateError>{satisfying(candidates,r,true)}
pub fn min_satisfying(candidates:&[String],r:&Range)->Result<Option<String>,CandidateError>{satisfying(candidates,r,false)}

pub fn intersects(_a:&Range,_b:&Range,_include_prerelease:bool)->bool{false}
pub fn canonical(_r:&Range)->String{"EMPTY".to_string()}