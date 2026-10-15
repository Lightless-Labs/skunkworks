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

fn parse_core_field(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if *pos == start {
        let at = start;
        if at == bytes.len() || matches!(bytes[at], b'.' | b'-' | b'+') {
            return Err(VersionError::MissingCore { at });
        }
        return Err(VersionError::InvalidChar { at });
    }
    if *pos - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    s[start..*pos]
        .parse::<u64>()
        .map_err(|_| VersionError::Overflow { at: start })
}

fn parse_identifiers(
    s: &str,
    offset: usize,
    pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut start = 0;
    loop {
        let end = s[start..]
            .find('.')
            .map(|i| start + i)
            .unwrap_or(s.len());
        if end == start {
            return Err(VersionError::EmptyIdentifier { at: offset + start });
        }
        for i in start..end {
            if !bytes[i].is_ascii_alphanumeric() && bytes[i] != b'-' {
                return Err(VersionError::InvalidChar { at: offset + i });
            }
        }
        let ident = &s[start..end];
        if pre
            && ident.len() > 1
            && ident.as_bytes()[0] == b'0'
            && ident.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(VersionError::LeadingZero { at: offset + start });
        }
        result.push(ident.to_string());
        if end == s.len() {
            break;
        }
        start = end + 1;
        if start == s.len() {
            return Err(VersionError::EmptyIdentifier { at: offset + start });
        }
    }
    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let bytes = s.as_bytes();
    let mut pos = 0;
    let major = parse_core_field(s, &mut pos)?;
    if pos >= bytes.len() || bytes[pos] != b'.' {
        if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;
    let minor = parse_core_field(s, &mut pos)?;
    if pos >= bytes.len() || bytes[pos] != b'.' {
        if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;
    let patch = parse_core_field(s, &mut pos)?;
    if pos < bytes.len() && !matches!(bytes[pos], b'-' | b'+') {
        return Err(VersionError::InvalidChar { at: pos });
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if pos < bytes.len() && bytes[pos] == b'-' {
        let start = pos + 1;
        let end = s[start..]
            .find('+')
            .map(|i| start + i)
            .unwrap_or(s.len());
        pre = parse_identifiers(&s[start..end], start, true)?;
        pos = end;
    }
    if pos < bytes.len() && bytes[pos] == b'+' {
        let start = pos + 1;
        build = parse_identifiers(&s[start..], start, false)?;
        pos = s.len();
    }
    if pos != s.len() {
        return Err(VersionError::InvalidChar { at: pos });
    }
    Ok(Version { major, minor, patch, pre, build })
}

fn numeric_ident(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn cmp_ident(a: &str, b: &str) -> Ordering {
    let an = numeric_ident(a);
    let bn = numeric_ident(b);
    match (an, bn) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch)
        .cmp(&(b.major, b.minor, b.patch))
        .then_with(|| match (a.pre.is_empty(), b.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => {
                for (x, y) in a.pre.iter().zip(&b.pre) {
                    let o = cmp_ident(x, y);
                    if o != Ordering::Equal {
                        return o;
                    }
                }
                a.pre.len().cmp(&b.pre.len())
            }
        })
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
    anchor: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op { Eq, Lt, Le, Gt, Ge }
#[derive(Clone, Debug)]
struct Arm {
    comparators: Vec<Comparator>,
}
#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

fn ws(b: u8) -> bool { matches!(b, b' ' | b'\t' | b'\r' | b'\n') }

fn full_at(s: &str, at: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at, error })
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let b = s.as_bytes();
    let mut tokens: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && ws(b[i]) { i += 1; }
        if i == b.len() { break; }
        let start = i;
        while i < b.len() && !ws(b[i]) { i += 1; }
        tokens.push((start, i));
    }
    if tokens.is_empty() { return Err(RangeError::Empty { at: 0 }); }

    let mut arms = Vec::new();
    let mut arm_tokens = Vec::new();
    let mut idx = 0;
    while idx < tokens.len() {
        let (st, en) = tokens[idx];
        if &s[st..en] == "||" {
            if arm_tokens.is_empty() {
                return Err(RangeError::Empty { at: st });
            }
            arms.push(parse_arm(s, &arm_tokens)?);
            arm_tokens.clear();
            idx += 1;
            if idx == tokens.len() {
                return Err(RangeError::Empty { at: s.len() });
            }
        } else {
            arm_tokens.push((st, en));
            idx += 1;
        }
    }
    if arm_tokens.is_empty() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(s, &arm_tokens)?);
    Ok(Range { arms })
}

fn parse_full_operand(s: &str, start: usize) -> Result<Version, RangeError> {
    full_at(s, start)
}

fn parse_partial(s: &str, start: usize) -> Result<(Vec<Option<u64>>, Option<Version>, bool), RangeError> {
    let has_mark = s.contains('-') || s.contains('+');
    let fields: Vec<&str> = s.split('.').collect();
    let has_wild = fields.iter().any(|f| matches!(*f, "*" | "x" | "X"));
    if has_mark || (fields.len() >= 3 && !has_wild) {
        return Ok((Vec::new(), Some(parse_full_operand(s, start)?), true));
    }
    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at: start });
    }
    let mut vals = Vec::new();
    let mut wildcard = false;
    for f in fields {
        if f.is_empty() {
            return Err(RangeError::Syntax { at: start });
        }
        if matches!(f, "*" | "x" | "X") {
            wildcard = true;
            vals.push(None);
        } else {
            if wildcard || !f.bytes().all(|b| b.is_ascii_digit()) || (f.len() > 1 && f.starts_with('0')) {
                return Err(RangeError::Syntax { at: start });
            }
            let n = f.parse::<u64>().map_err(|_| RangeError::Syntax { at: start })?;
            vals.push(Some(n));
        }
    }
    Ok((vals, None, false))
}

fn version_tuple(v: &Version) -> (u64,u64,u64) { (v.major,v.minor,v.patch) }

fn make_version(t: (u64,u64,u64), pre: Vec<String>, build: Vec<String>) -> Version {
    Version { major:t.0, minor:t.1, patch:t.2, pre, build }
}
fn v0(t:(u64,u64,u64)) -> Version { make_version(t, vec![], vec![]) }
fn vminus0(t:(u64,u64,u64)) -> Version { make_version(t, vec!["0".into()], vec![]) }

fn padded(vals:&[Option<u64>]) -> (u64,u64,u64) {
    let mut a=[0u64;3];
    for (i,v) in vals.iter().enumerate() { if let Some(n)=v { a[i]=*n; } }
    (a[0],a[1],a[2])
}
fn k(vals:&[Option<u64>])->usize { vals.iter().filter(|v| v.is_some()).count() }
fn next_bound(t:(u64,u64,u64), field:usize, at:usize)->Result<Version,RangeError>{
    let (mut a,mut b,mut c)=t;
    let target=match field {0=>&mut a,1=>&mut b,_=>&mut c};
    *target=target.checked_add(1).ok_or(RangeError::Syntax{at})?;
    if field==0 {b=0;c=0;} else if field==1 {c=0;}
    Ok(vminus0((a,b,c)))
}
fn add(c:&mut Vec<Comparator>, op:Op, v:Version, anchor:bool) {
    c.push(Comparator{op,version:v,anchor});
}

fn parse_bare(tok:&str, start:usize, c:&mut Vec<Comparator>) -> Result<(),RangeError> {
    let (vals, full, _) = parse_partial(tok,start)?;
    if let Some(v)=full {
        let anchor=!v.pre.is_empty();
        add(c,Op::Eq,v,anchor);
        return Ok(());
    }
    let count=k(&vals);
    if count==0 { return Ok(()); }
    let t=padded(&vals);
    add(c,Op::Ge,v0(t),false);
    if count==1 { add(c,Op::Lt,next_bound(t,0,start)?,false); }
    else if count==2 { add(c,Op::Lt,next_bound(t,1,start)?,false); }
    Ok(())
}

fn parse_caret_tilde(tok:&str,start:usize,c:&mut Vec<Comparator>,caret:bool)->Result<(),RangeError>{
    let operand=&tok[1..];
    let os=start+1;
    let (vals,full,_)=parse_partial(operand,os)?;
    let count=if let Some(ref v)=full {3} else {k(&vals)};
    if count==0 {return Err(RangeError::Syntax{at:start});}
    let lower=if let Some(v)=full {v} else {v0(padded(&vals))};
    let t=version_tuple(&lower);
    let field=if caret {
        if t.0>0 || count==1 {0} else if t.1>0 || count==2 {1} else {2}
    } else if count==1 {0} else {1};
    add(c,Op::Ge,lower.clone(),!lower.pre.is_empty());
    add(c,Op::Lt,next_bound(t,field,os)?,false);
    Ok(())
}

fn parse_comparator(tok:&str,start:usize,c:&mut Vec<Comparator>)->Result<(),RangeError>{
    let bytes=tok.as_bytes();
    let (op,n)=if tok.starts_with(">="){(Op::Ge,2)}else if tok.starts_with("<="){(Op::Le,2)}
        else if tok.starts_with(">"){(Op::Gt,1)}else if tok.starts_with("<"){(Op::Lt,1)}
        else {(Op::Eq,1)};
    let operand=&tok[n..];
    let os=start+n;
    if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
        return Err(RangeError::Syntax{at:start});
    }
    let v=parse_full_operand(operand,os)?;
    let anchor=!v.pre.is_empty();
    let _=bytes;
    add(c,op,v,anchor);
    Ok(())
}

fn parse_arm(s:&str,toks:&[(usize,usize)])->Result<Arm,RangeError>{
    let standalone:Vec<usize>=toks.iter().enumerate().filter_map(|(i,(a,b))| if &s[*a..*b]=="-" {Some(i)} else {None}).collect();
    if !standalone.is_empty() {
        let m=standalone[0];
        if toks.len()!=3 || m!=1 {
            return Err(RangeError::Syntax{at:toks[m].0});
        }
        let (ls,le)=toks[0]; let (us,ue)=toks[2];
        let (lv,lf,_)=parse_partial(&s[ls..le],ls)?;
        let (uv,uf,_)=parse_partial(&s[us..ue],us)?;
        let lk=if lf.is_some(){3}else{k(&lv)};
        let uk=if uf.is_some(){3}else{k(&uv)};
        if lk==0 {return Err(RangeError::Syntax{at:ls});}
        if uk==0 {return Err(RangeError::Syntax{at:us});}
        let mut c=Vec::new();
        let low=lf.unwrap_or_else(||v0(padded(&lv)));
        add(&mut c,Op::Ge,low.clone(),!low.pre.is_empty());
        let upper=if let Some(v)=uf {
            add(&mut c,Op::Le,v.clone(),!v.pre.is_empty());
            None
        } else {
            let t=padded(&uv);
            let field=if uk==1 {0}else{1};
            Some(next_bound(t,field,us)?)
        };
        if let Some(v)=upper { add(&mut c,Op::Lt,v,false); }
        return Ok(Arm{comparators:c});
    }

    let mut c=Vec::new();
    for &(st,en) in toks {
        let tok=&s[st..en];
        if tok.starts_with('^') { parse_caret_tilde(tok,st,&mut c,true)?; }
        else if tok.starts_with('~') { parse_caret_tilde(tok,st,&mut c,false)?; }
        else if tok.starts_with('<') || tok.starts_with('>') || tok.starts_with('=') {
            parse_comparator(tok,st,&mut c)?;
        } else if tok.starts_with(',') || tok.starts_with('(') || tok.starts_with(')') || tok=="&" {
            return Err(RangeError::Syntax{at:st});
        } else if tok.as_bytes()[0].is_ascii_digit() || matches!(tok,"*"|"x"|"X") {
            parse_bare(tok,st,&mut c)?;
        } else {
            return Err(RangeError::Syntax{at:st});
        }
    }
    Ok(Arm{comparators:c})
}

fn admission(v:&Version, arm:&Arm)->bool {
    arm.comparators.iter().any(|c| c.anchor && version_tuple(&c.version)==version_tuple(v))
}
fn check_arm(v:&Version, arm:&Arm, include:bool)->bool {
    if !v.pre.is_empty() && !include && !admission(v,arm) {return false;}
    arm.comparators.iter().all(|c|{
        let o=compare_versions(v,&c.version);
        match c.op {Op::Eq=>o==Ordering::Equal,Op::Lt=>o==Ordering::Less,Op::Le=>o!=Ordering::Greater,Op::Gt=>o==Ordering::Greater,Op::Ge=>o!=Ordering::Less}
    })
}
pub fn matches(v:&Version,r:&Range)->bool { matches_with(v,r,false) }
pub fn matches_with(v:&Version,r:&Range,include_prerelease:bool)->bool {
    r.arms.iter().any(|a|check_arm(v,a,include_prerelease))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError { pub index: usize, pub error: VersionError }

pub fn max_satisfying(candidates:&[String],r:&Range)->Result<Option<String>,CandidateError>{
    extrema(candidates,r,true)
}
pub fn min_satisfying(candidates:&[String],r:&Range)->Result<Option<String>,CandidateError>{
    extrema(candidates,r,false)
}
fn extrema(candidates:&[String],r:&Range,max:bool)->Result<Option<String>,CandidateError>{
    let parsed:Result<Vec<Version>,CandidateError>=candidates.iter().enumerate().map(|(i,s)|parse_version(s).map_err(|error|CandidateError{index:i,error})).collect();
    let parsed=parsed?;
    let mut best:Option<usize>=None;
    for (i,v) in parsed.iter().enumerate(){
        if !matches(v,r){continue;}
        if let Some(j)=best {
            let o=compare_versions(v,&parsed[j]);
            if (max&&o==Ordering::Greater)||(!max&&o==Ordering::Less){best=Some(i);}
        } else {best=Some(i);}
    }
    Ok(best.map(|i|candidates[i].clone()))
}

fn tuple_cmp(a:(u64,u64,u64),b:(u64,u64,u64))->Ordering {a.cmp(&b)}
fn tuple_successor(t:(u64,u64,u64))->Option<(u64,u64,u64)>{
    if t.2<u64::MAX {Some((t.0,t.1,t.2+1))}
    else if t.1<u64::MAX {Some((t.0,t.1+1,0))}
    else if t.0<u64::MAX {Some((t.0+1,0,0))}
    else {None}
}

#[derive(Clone)]
struct Bounds { lower:Option<(Version,bool)>, upper:Option<(Version,bool)>, anchors:Vec<(u64,u64,u64)>, impossible:bool }
fn bounds_arm(a:&Arm)->Bounds{
    let mut b=Bounds{lower:None,upper:None,anchors:Vec::new(),impossible:false};
    for c in &a.comparators {
        if c.anchor { let t=version_tuple(&c.version); if !b.anchors.contains(&t){b.anchors.push(t);} }
        match c.op {
            Op::Eq=>{
                tighten_lower(&mut b.lower,c.version.clone(),true);
                tighten_upper(&mut b.upper,c.version.clone(),true);
            }
            Op::Ge=>tighten_lower(&mut b.lower,c.version.clone(),true),
            Op::Gt=>tighten_lower(&mut b.lower,c.version.clone(),false),
            Op::Le=>tighten_upper(&mut b.upper,c.version.clone(),true),
            Op::Lt=>tighten_upper(&mut b.upper,c.version.clone(),false),
        }
    }
    if let (Some((l,li)),Some((u,ui)))=(&b.lower,&b.upper) {
        let o=compare_versions(l,u);
        if o==Ordering::Greater || (o==Ordering::Equal && (!li||!ui)){b.impossible=true;}
    }
    b
}
fn tighten_lower(slot:&mut Option<(Version,bool)>,v:Version,inclusive:bool){
    match slot {None=>*slot=Some((v,inclusive)),Some((old,inc))=>{
        let o=compare_versions(&v,old);
        if o==Ordering::Greater || (o==Ordering::Equal && !inclusive && *inc){*slot=Some((v,inclusive));}
    }}
}
fn tighten_upper(slot:&mut Option<(Version,bool)>,v:Version,inclusive:bool){
    match slot {None=>*slot=Some((v,inclusive)),Some((old,inc))=>{
        let o=compare_versions(&v,old);
        if o==Ordering::Less || (o==Ordering::Equal && !inclusive && *inc){*slot=Some((v,inclusive));}
    }}
}

fn candidate_witness(b:&Bounds, include:bool)->bool{
    if b.impossible{return false;}
    if interval_has_stable(b){return true;}
    if include {
        return interval_has_any_pre(b);
    }
    b.anchors.iter().any(|t|{
        let v=vminus0(*t);
        in_bounds(&v,b)
    })
}
fn in_bounds(v:&Version,b:&Bounds)->bool{
    if let Some((l,inc))=&b.lower {let o=compare_versions(v,l);if o==Ordering::Less||(o==Ordering::Equal&&!*inc){return false;}}
    if let Some((u,inc))=&b.upper {let o=compare_versions(v,u);if o==Ordering::Greater||(o==Ordering::Equal&&!*inc){return false;}}
    true
}
fn interval_has_stable(b:&Bounds)->bool{
    let mut t=b.lower.as_ref().map(|x|version_tuple(&x.0)).unwrap_or((0,0,0));
    if let Some((l,inc))=&b.lower {
        if !inc && l.pre.is_empty() {
            if let Some(n)=tuple_successor(t){t=n;} else{return false;}
        }
    }
    let v=v0(t);
    if !in_bounds(&v,b) {
        if let Some((u,inc))=&b.upper {
            if !inc && !u.pre.is_empty() {
                let tu=version_tuple(u);
                return tuple_cmp(t,tu)!=Ordering::Greater;
            }
        }
        return false;
    }
    true
}
fn interval_has_any_pre(b:&Bounds)->bool{
    if b.impossible{return false;}
    // Prerelease candidate at an admissible core is represented elsewhere; for include=true,
    // any nonempty interval containing a tuple has a least prerelease at that tuple.
    let t=b.lower.as_ref().map(|x|version_tuple(&x.0)).unwrap_or((0,0,0));
    let v=vminus0(t);
    if in_bounds(&v,b){return true;}
    if let Some(n)=tuple_successor(t){in_bounds(&vminus0(n),b)}else{false}
}
fn arm_intersects(a:&Arm,b:&Arm,include:bool)->bool{
    let mut all=a.comparators.clone(); all.extend(b.comparators.clone());
    let x=Arm{comparators:all};
    candidate_witness(&bounds_arm(&x),include)
}
pub fn intersects(a:&Range,b:&Range,include_prerelease:bool)->bool{
    for x in &a.arms {for y in &b.arms {if arm_intersects(x,y,include_prerelease){return true;}}}
    false
}

fn canonical_version(v:&Version)->String{
    let mut s=format!("{}.{}.{}",v.major,v.minor,v.patch);
    if !v.pre.is_empty(){s.push('-');s.push_str(&v.pre.join("."));}
    s
}
fn canonical_arm(a:&Arm)->Option<String>{
    let b=bounds_arm(a);
    if !candidate_witness(&b,false){return None;}
    let mut anchors=b.anchors.clone();
    anchors.sort();
    let an=if anchors.is_empty() {"-".to_string()} else {anchors.iter().map(|t|format!("{}.{}.{}",t.0,t.1,t.2)).collect::<Vec<_>>().join(",")};
    let lo=b.lower.as_ref().map(|(v,i)|format!("{}{}",if *i{">="}else{">"},canonical_version(v))).unwrap_or("*".into());
    let hi=b.upper.as_ref().map(|(v,i)|format!("{}{}",if *i{"<="}else{"<"},canonical_version(v))).unwrap_or("*".into());
    Some(format!("[{};{};pre={}]",lo,hi,an))
}
pub fn canonical(r:&Range)->String{
    let mut out:Vec<String>=r.arms.iter().filter_map(canonical_arm).collect();
    out.sort_by(|a,b|a.as_bytes().cmp(b.as_bytes()));
    out.dedup();
    if out.is_empty(){"EMPTY".into()}else{out.join(" || ")}
}

#[derive(Clone, Debug)]
pub enum SetExpr { Range(String), Union(Vec<SetExpr>), Intersection(Vec<SetExpr>) }
#[derive(Clone, Debug)]
pub struct Constraint { pub label:String, pub expr:SetExpr }
#[derive(Clone, Debug)]
pub struct Package { pub name:String, pub candidates:Vec<String>, pub constraints:Vec<Constraint> }
#[derive(Clone, Copy, Debug)]
pub enum Preference { Highest, Lowest, StableFirst }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection { pub package:String, pub version:String }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectError {
    DuplicatePackage { name:String },
    DuplicateLabel { package:String, label:String },
    InvalidCandidate { package:String, index:usize, error:VersionError },
    InvalidRange { package:String, label:String, path:Vec<usize>, error:RangeError },
    NoSolution { package:String, labels:Vec<String> },
}

fn validate_expr(expr:&SetExpr,pkg:&str,label:&str,path:&mut Vec<usize>,out:&mut Vec<(Vec<usize>,Range)>)->Result<(),SelectError>{
    match expr {
        SetExpr::Range(s)=>{
            let r=parse_range(s).map_err(|error|SelectError::InvalidRange{package:pkg.to_string(),label:label.to_string(),path:path.clone(),error})?;
            out.push((path.clone(),r));
        }
        SetExpr::Union(v)|SetExpr::Intersection(v)=>{
            for (i,e) in v.iter().enumerate(){path.push(i);validate_expr(e,pkg,label,path,out)?;path.pop();}
        }
    }
    Ok(())
}
fn eval_expr(expr:&SetExpr,v:&Version,include:bool)->bool{
    match expr {
        SetExpr::Range(s)=>parse_range(s).map(|r|matches_with(v,&r,include)).unwrap_or(false),
        SetExpr::Union(xs)=>xs.iter().any(|x|eval_expr(x,v,include)),
        SetExpr::Intersection(xs)=>xs.iter().all(|x|eval_expr(x,v,include)),
    }
}

pub fn select_packages(packages:&[Package],preference:Preference,include_prerelease:bool)->Result<Vec<Selection>,SelectError>{
    let mut names=std::collections::HashSet::new();
    for p in packages {
        if !names.insert(p.name.clone()){return Err(SelectError::DuplicatePackage{name:p.name.clone()});}
    }
    for p in packages {
        let mut labels=std::collections::HashSet::new();
        for c in &p.constraints {
            if !labels.insert(c.label.clone()){return Err(SelectError::DuplicateLabel{package:p.name.clone(),label:c.label.clone()});}
        }
    }
    let mut parsed:Vec<Vec<Version>>=Vec::new();
    for p in packages {
        let mut vs=Vec::new();
        for (i,s) in p.candidates.iter().enumerate(){
            vs.push(parse_version(s).map_err(|error|SelectError::InvalidCandidate{package:p.name.clone(),index:i,error})?);
        }
        parsed.push(vs);
    }
    let mut ranges:Vec<Vec<Vec<(Vec<usize>,Range)>>>=Vec::new();
    for p in packages {
        let mut pr=Vec::new();
        for c in &p.constraints {
            let mut leaves=Vec::new();
            validate_expr(&c.expr,&p.name,&c.label,&mut Vec::new(),&mut leaves)?;
            pr.push(leaves);
        }
        ranges.push(pr);
    }

    let mut selections=Vec::new();
    for (pi,p) in packages.iter().enumerate(){
        let qualifies:Vec<usize>=(0..p.candidates.len()).filter(|&i|{
            p.constraints.iter().all(|c|eval_expr(&c.expr,&parsed[pi][i],include_prerelease))
        }).collect();
        if qualifies.is_empty(){
            let labels=if p.candidates.is_empty(){Vec::new()}else{diagnose(p,&parsed[pi],include_prerelease)};
            return Err(SelectError::NoSolution{package:p.name.clone(),labels});
        }
        let mut best=qualifies[0];
        for &i in qualifies.iter().skip(1){
            let vi=&parsed[pi][i];let vb=&parsed[pi][best];
            let better=match preference{
                Preference::Highest=>compare_versions(vi,vb)==Ordering::Greater,
                Preference::Lowest=>compare_versions(vi,vb)==Ordering::Less,
                Preference::StableFirst=>{
                    let si=vi.pre.is_empty();let sb=vb.pre.is_empty();
                    if si!=sb {si} else {compare_versions(vi,vb)==Ordering::Greater}
                }
            };
            if better{best=i;}
        }
        selections.push(Selection{package:p.name.clone(),version:p.candidates[best].clone()});
    }
    Ok(selections)
}

fn diagnose(p:&Package,vs:&[Version],include:bool)->Vec<String>{
    let n=p.constraints.len();
    let mut best:Option<Vec<String>>=None;
    if n>=usize::BITS as usize{return Vec::new();}
    for mask in 1usize..(1usize<<n){
        let count=mask.count_ones() as usize;
        if best.as_ref().map(|x|x.len()<count).unwrap_or(false){continue;}
        let any=vs.iter().enumerate().any(|(i,v)|{
            (0..n).filter(|j|mask&(1<<j)!=0).all(|j|eval_expr(&p.constraints[j].expr,v,include))
        });
        if !any{
            let mut labels=(0..n).filter(|j|mask&(1<<j)!=0).map(|j|p.constraints[j].label.clone()).collect::<Vec<_>>();
            labels.sort_by(|a,b|a.as_bytes().cmp(b.as_bytes()));
            if best.as_ref().map(|x|labels.len()<x.len() || (labels.len()==x.len() && labels.as_slice()<x.as_slice())).unwrap_or(true){best=Some(labels);}
        }
    }
    best.unwrap_or_default()
}

fn main() {}