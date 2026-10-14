// Deliberate mutant: stable-first ignores stability
use std::cmp::Ordering;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u64, pub minor: u64, pub patch: u64,
    pub pre: Vec<String>, pub build: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VersionError {
    Empty, MissingCore { at: usize }, LeadingZero { at: usize },
    Overflow { at: usize }, EmptyIdentifier { at: usize }, InvalidChar { at: usize },
}
pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() { return Err(VersionError::Empty); }
    let b=s.as_bytes(); let mut p=0; let mut nums=[0;3];
    for i in 0..3 {
        let start=p;
        while p<b.len() && b[p].is_ascii_digit() { p+=1; }
        if start==p {
            return Err(if p==b.len() || matches!(b[p], b'.'|b'-'|b'+') {
                VersionError::MissingCore{at:p}
            } else { VersionError::InvalidChar{at:p} });
        }
        if p-start>1 && b[start]==b'0' { return Err(VersionError::LeadingZero{at:start}); }
        nums[i]=s[start..p].parse().map_err(|_| VersionError::Overflow{at:start})?;
        if i<2 {
            if p==b.len() || matches!(b[p], b'-'|b'+') { return Err(VersionError::MissingCore{at:p}); }
            if b[p]!=b'.' { return Err(VersionError::InvalidChar{at:p}); }
            p+=1;
        }
    }
    let mut pre=Vec::new(); let mut build=Vec::new();
    if p<b.len() && b[p]==b'-' {
        p+=1; pre=identifiers(s,&mut p,true)?;
    }
    if p<b.len() && b[p]==b'+' {
        p+=1; build=identifiers(s,&mut p,false)?;
    }
    if p<b.len() { return Err(VersionError::InvalidChar{at:p}); }
    Ok(Version{major:nums[0],minor:nums[1],patch:nums[2],pre,build})
}
fn identifiers(s:&str,p:&mut usize,pre:bool)->Result<Vec<String>,VersionError> {
    let b=s.as_bytes(); let mut out=Vec::new();
    loop {
        let start=*p;
        while *p<b.len() && b[*p]!=b'.' && !(pre && b[*p]==b'+') {
            if !(b[*p].is_ascii_alphanumeric() || b[*p]==b'-') { return Err(VersionError::InvalidChar{at:*p}); }
            *p+=1;
        }
        if *p==start { return Err(VersionError::EmptyIdentifier{at:start}); }
        let id=&s[start..*p];
        if pre && id.len()>1 && id.as_bytes()[0]==b'0' && id.bytes().all(|c|c.is_ascii_digit()) {
            return Err(VersionError::LeadingZero{at:start});
        }
        out.push(id.to_string());
        if *p==b.len() || (pre && b[*p]==b'+') { break; }
        *p+=1;
    }
    Ok(out)
}
pub fn compare_versions(a:&Version,b:&Version)->Ordering {
    let core=(a.major,a.minor,a.patch).cmp(&(b.major,b.minor,b.patch));
    if core!=Ordering::Equal { return core; }
    if a.pre.is_empty() || b.pre.is_empty() {
        return match (a.pre.is_empty(),b.pre.is_empty()) {
            (true,false)=>Ordering::Greater,(false,true)=>Ordering::Less,_=>Ordering::Equal,
        };
    }
    for (x,y) in a.pre.iter().zip(&b.pre) {
        let xn=x.bytes().all(|c|c.is_ascii_digit()); let yn=y.bytes().all(|c|c.is_ascii_digit());
        let c=match (xn,yn) {
            (true,true)=>x.len().cmp(&y.len()).then_with(||x.cmp(y)),
            (true,false)=>Ordering::Less,(false,true)=>Ordering::Greater,
            (false,false)=>x.cmp(y),
        };
        if c!=Ordering::Equal {return c;}
    }
    a.pre.len().cmp(&b.pre.len())
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError { Empty { at: usize }, Syntax { at: usize }, Version { at: usize, error: VersionError } }
#[derive(Clone, Debug)]
pub struct Range { arms: Vec<Arm> }
#[derive(Clone, Debug)]
struct Arm { atoms: Vec<Atom>, anchors: BTreeSet<(u64,u64,u64)> }
#[derive(Clone, Debug)]
struct Atom { op: u8, v: Version }
fn tuple(v:&Version)->(u64,u64,u64) { (v.major,v.minor,v.patch) }
fn plain(t:(u64,u64,u64))->Version { Version{major:t.0,minor:t.1,patch:t.2,pre:vec![],build:vec![]} }
fn floor(t:(u64,u64,u64))->Version { let mut v=plain(t); v.pre.push("0".into()); v }
fn version_at(s:&str,at:usize)->Result<Version,RangeError> { parse_version(s).map_err(|error|RangeError::Version{at,error}) }
fn add(a:&mut Arm,op:u8,v:Version,anchor:bool) {
    if anchor && !v.pre.is_empty() {a.anchors.insert(tuple(&v));}
    a.atoms.push(Atom{op,v});
}
fn tokens(s:&str)->Vec<(&str,usize)> {
    let b=s.as_bytes();let mut p=0;let mut out=vec![];
    while p<b.len() {
        if matches!(b[p], b' '|b'\t'|b'\r'|b'\n') {p+=1;continue;}
        let st=p;
        while p<b.len() && !matches!(b[p], b' '|b'\t'|b'\r'|b'\n') {p+=1;}
        out.push((&s[st..p],st));
    }
    out
}
fn atom_matches(v:&Version,a:&Atom)->bool {
    let c=compare_versions(v,&a.v);
    match a.op {0=>c==Ordering::Equal,1=>c==Ordering::Less,2=>c!=Ordering::Greater,3=>c==Ordering::Greater,_=>c!=Ordering::Less}
}
fn arm_matches(v:&Version,a:&Arm,include:bool)->bool {
    (include || v.pre.is_empty() || a.anchors.contains(&tuple(v))) && a.atoms.iter().all(|c|atom_matches(v,c))
}
pub fn parse_range(s:&str)->Result<Range,RangeError> {
    let ts=tokens(s);if ts.is_empty(){return Err(RangeError::Empty{at:0});}
    let mut arms=vec![];let mut start=0;
    for i in 0..=ts.len() {
        if i==ts.len() || ts[i].0=="||" {
            if start==i{return Err(RangeError::Empty{at:if i==ts.len(){s.len()}else{ts[i].1}});}
            arms.push(parse_arm(&ts[start..i])?);start=i+1;
        }
    }
    Ok(Range{arms})
}
fn partial(s:&str,at:usize)->Result<(Version,usize),RangeError> {
    if s.contains('-') || s.contains('+') || (s.split('.').count()>=3 && !s.split('.').any(|p|matches!(p,"*"|"x"|"X"))) {return Ok((version_at(s,at)?,3));}
    let ps:Vec<_>=s.split('.').collect();
    if ps.len()>3 {return Err(RangeError::Syntax{at});}
    let mut nums=[0;3];let mut count=0;let mut wild=false;
    for (i,p) in ps.iter().enumerate() {
        if matches!(*p,"*"|"x"|"X") {wild=true;continue;}
        if wild || p.is_empty() || !p.bytes().all(|c|c.is_ascii_digit()) || (p.len()>1 && p.starts_with('0')) {
            return Err(RangeError::Syntax{at});
        }
        nums[i]=p.parse().map_err(|_|RangeError::Syntax{at})?;count+=1;
    }
    if count==3 {return Ok((version_at(s,at)?,3));}
    Ok((plain((nums[0],nums[1],nums[2])),count))
}
fn bump(t:(u64,u64,u64),field:usize,at:usize)->Result<Version,RangeError> {
    let t=match field {
        0=>(t.0.checked_add(1).ok_or(RangeError::Syntax{at})?,0,0),
        1=>(t.0,t.1.checked_add(1).ok_or(RangeError::Syntax{at})?,0),
        _=>(t.0,t.1,t.2.checked_add(1).ok_or(RangeError::Syntax{at})?),
    };Ok(floor(t))
}
fn parse_arm(ts:&[(&str,usize)])->Result<Arm,RangeError> {
    let mut a=Arm{atoms:vec![],anchors:BTreeSet::new()};
    if ts.iter().any(|t|t.0=="-") {
        if ts.len()!=3 || ts[1].0!="-" {return Err(RangeError::Syntax{at:ts.iter().find(|t|t.0=="-").unwrap().1});}
        let (lo,lc)=partial(ts[0].0,ts[0].1)?;let (hi,hc)=partial(ts[2].0,ts[2].1)?;
        if lc==0 || hc==0 {return Err(RangeError::Syntax{at:if lc==0{ts[0].1}else{ts[2].1}});}
        add(&mut a,4,lo,true);
        if hc==3 {add(&mut a,2,hi,true);}else{add(&mut a,1,bump(tuple(&hi),hc-1,ts[2].1)?,false);}
        return Ok(a);
    }
    for &(s,at) in ts {
        if s.starts_with('<') || s.starts_with('>') || s.starts_with('=') {
            let (op,n)=if s.starts_with(">="){(4,2)}else if s.starts_with("<="){(2,2)}else if s.starts_with('>'){(3,1)}else if s.starts_with('<'){(1,1)}else{(0,1)};
            if s[n..].is_empty() || !s.as_bytes()[n].is_ascii_digit(){return Err(RangeError::Syntax{at});}
            add(&mut a,op,version_at(&s[n..],at+n)?,true);continue;
        }
        if !s.as_bytes()[0].is_ascii_digit() && !matches!(s.as_bytes()[0], b'\x2a'|b'x'|b'X'|b'^'|b'~') {return Err(RangeError::Syntax{at});}
        let (kind,n)=if s.starts_with('^'){(1,1)}else if s.starts_with('~'){(2,1)}else{(0,0)};
        let (v,count)=partial(&s[n..],at+n)?;
        if count==0 {if kind!=0{return Err(RangeError::Syntax{at});}continue;}
        if kind==0 && count==3 {add(&mut a,0,v,true);continue;}
        let field=if kind==2 {if count==1{0}else{1}}
            else if kind==1 {if v.major>0 || count==1{0}else if v.minor>0 || count==2{1}else{2}}
            else {count-1};
        let upper=bump(tuple(&v),field,at+n)?;
        add(&mut a,4,v,true);add(&mut a,1,upper,false);
    }
    Ok(a)
}
pub fn matches(v:&Version,r:&Range)->bool {r.arms.iter().any(|a|arm_matches(v,a,false))}
pub fn max_satisfying(candidates:&[String],r:&Range)->Result<Option<String>,CandidateError> {extreme(candidates,r,true)}
pub fn min_satisfying(candidates:&[String],r:&Range)->Result<Option<String>,CandidateError> {extreme(candidates,r,false)}
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct CandidateError {pub index:usize,pub error:VersionError}
fn extreme(cs:&[String],r:&Range,max:bool)->Result<Option<String>,CandidateError> {
    let vs:Vec<_>=cs.iter().enumerate().map(|(index,s)|parse_version(s).map_err(|error|CandidateError{index,error})).collect::<Result<_,_>>()?;
    let mut best:Option<usize>=None;
    for (i,v) in vs.iter().enumerate() {
        if matches(v,r) && best.is_none_or(|j| if max {compare_versions(v,&vs[j])==Ordering::Greater}else{compare_versions(v,&vs[j])==Ordering::Less}) {best=Some(i);}
    }
    Ok(best.map(|i|cs[i].clone()))
}
pub fn matches_with(v:&Version,r:&Range,include_prerelease:bool)->bool {
    r.arms.iter().any(|a|arm_matches(v,a,include_prerelease))
}
#[derive(Clone)]
struct Bounds {lo:Option<(Version,bool)>,hi:Option<(Version,bool)>}
fn bounds(a:&Arm)->Bounds {
    let mut b=Bounds{lo:None,hi:None};
    for atom in &a.atoms {
        if matches!(atom.op,0|3|4) { tighten(&mut b.lo,atom.v.clone(),atom.op!=3,true); }
        if matches!(atom.op,0|1|2) { tighten(&mut b.hi,atom.v.clone(),atom.op!=1,false); }
    } b
}
fn tighten(slot:&mut Option<(Version,bool)>,mut v:Version,closed:bool,lower:bool) {
    v.build.clear();
    match slot {
        None=>*slot=Some((v,closed)),
        Some((old,c))=>{let cmp=compare_versions(&v,old);if (lower && cmp==Ordering::Greater)||(!lower && cmp==Ordering::Less){*slot=Some((v,closed));}else if cmp==Ordering::Equal{*c &= closed;}}
    }
}
fn join_bounds(a:&Bounds,b:&Bounds)->Bounds {
    let mut c=a.clone();
    if let Some((v,i))=&b.lo {tighten(&mut c.lo,v.clone(),*i,true);}
    if let Some((v,i))=&b.hi {tighten(&mut c.hi,v.clone(),*i,false);}
    c
}
fn next_tuple(t:(u64,u64,u64))->Option<(u64,u64,u64)> {
    if t.2<u64::MAX {Some((t.0,t.1,t.2+1))}else if t.1<u64::MAX {Some((t.0,t.1+1,0))}else if t.0<u64::MAX {Some((t.0+1,0,0))}else{None}
}
fn after(v:&Version)->Option<Version> {
    if !v.pre.is_empty() {let mut n=v.clone();n.build.clear();n.pre.push("0".into());Some(n)}else{next_tuple(tuple(v)).map(floor)}
}
fn within(v:&Version,b:&Bounds)->bool {
    b.lo.as_ref().is_none_or(|(l,c)|{let x=compare_versions(v,l);x==Ordering::Greater || (*c && x==Ordering::Equal)}) &&
    b.hi.as_ref().is_none_or(|(h,c)|{let x=compare_versions(v,h);x==Ordering::Less || (*c && x==Ordering::Equal)})
}
fn exists(b:&Bounds,anchors:&BTreeSet<(u64,u64,u64)>,include:bool)->bool {
    if include {
        let candidate=match &b.lo {None=>Some(floor((0,0,0))),Some((v,true))=>Some(v.clone()),Some((v,false))=>after(v)};
        return candidate.is_some_and(|v|within(&v,b));
    }
    let stable=match &b.lo {
        None=>Some(plain((0,0,0))),Some((v,c))=>if !v.pre.is_empty() || *c {Some(plain(tuple(v)))}else{next_tuple(tuple(v)).map(plain)},
    };
    if stable.is_some_and(|v|within(&v,b)){return true;}
    for &t in anchors {
        let mut v=floor(t);
        if let Some((lo,c))=&b.lo {
            if tuple(lo)>t || (tuple(lo)==t && lo.pre.is_empty()) {continue;}
            if tuple(lo)==t {v=if *c{lo.clone()}else{after(lo).unwrap()};}
        }
        if !v.pre.is_empty() && within(&v,b){return true;}
    }false
}
pub fn intersects(a:&Range,b:&Range,include_prerelease:bool)->bool {
    a.arms.iter().any(|x|b.arms.iter().any(|y| {
        let anchors=x.anchors.intersection(&y.anchors).copied().collect();
        exists(&join_bounds(&bounds(x),&bounds(y)),&anchors,include_prerelease)
    }))
}
fn show(v:&Version)->String {
    let mut s=format!("{}.{}.{}",v.major,v.minor,v.patch);
    if !v.pre.is_empty(){s.push('-');s.push_str(&v.pre.join("."));}s
}
pub fn canonical(r:&Range)->String {
    let mut arms=BTreeSet::new();
    for a in &r.arms {
        let b=bounds(a);
        if !exists(&b,&a.anchors,false){continue;}
        let lower=b.lo.as_ref().map_or("*".to_string(),|(v,c)|format!("{}{}",if *c{">="}else{">"},show(v)));
        let upper=b.hi.as_ref().map_or("*".to_string(),|(v,c)|format!("{}{}",if *c{"<="}else{"<"},show(v)));
        let anchors=a.anchors.iter().map(|t|format!("{}.{}.{}",t.0,t.1,t.2)).collect::<Vec<_>>().join(",");
        arms.insert(format!("[{};{};pre={}]",lower,upper,if anchors.is_empty(){"-"}else{&anchors}));
    }
    if arms.is_empty(){"EMPTY".into()}else{arms.into_iter().collect::<Vec<_>>().join(" || ")}
}
#[derive(Clone,Debug)]
pub enum SetExpr { Range(String), Union(Vec<SetExpr>), Intersection(Vec<SetExpr>) }
#[derive(Clone,Debug)]
pub struct Constraint {pub label:String,pub expr:SetExpr}
#[derive(Clone,Debug)]
pub struct Package {pub name:String,pub candidates:Vec<String>,pub constraints:Vec<Constraint>}
#[derive(Clone,Copy,Debug)]
pub enum Preference {Highest,Lowest,StableFirst}
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct Selection {pub package:String,pub version:String}
#[derive(Clone,Debug,PartialEq,Eq)]
pub enum SelectError {
    DuplicatePackage{name:String},DuplicateLabel{package:String,label:String},
    InvalidCandidate{package:String,index:usize,error:VersionError},
    InvalidRange{package:String,label:String,path:Vec<usize>,error:RangeError},
    NoSolution{package:String,labels:Vec<String>},
}
#[derive(Clone)]
enum Parsed { Leaf(Range), Union(Vec<Parsed>), Intersection(Vec<Parsed>) }
fn parse_expr(e:&SetExpr,path:&mut Vec<usize>)->Result<Parsed,(Vec<usize>,RangeError)> {
    match e {
        SetExpr::Range(s)=>parse_range(s).map(Parsed::Leaf).map_err(|e|(path.clone(),e)),
        SetExpr::Union(es)|SetExpr::Intersection(es)=>{
            let mut children=vec![];
            for (i,x) in es.iter().enumerate(){path.push(i);let p=parse_expr(x,path)?;path.pop();children.push(p);}
            Ok(if matches!(e,SetExpr::Union(_)){Parsed::Union(children)}else{Parsed::Intersection(children)})
        }
    }
}
fn eval(e:&Parsed,v:&Version,inc:bool)->bool {
    match e {Parsed::Leaf(r)=>matches_with(v,r,inc),Parsed::Union(es)=>es.iter().any(|e|eval(e,v,inc)),Parsed::Intersection(es)=>es.iter().all(|e|eval(e,v,inc))}
}
fn conflict(vs:&[Version],cs:&[(String,Parsed)],inc:bool)->Vec<String> {
    if vs.is_empty(){return vec![];}
    let mut order:Vec<_>=(0..cs.len()).collect();order.sort_by(|&i,&j|cs[i].0.cmp(&cs[j].0));
    fn search(pos:usize,left:usize,order:&[usize],chosen:&mut Vec<usize>,vs:&[Version],cs:&[(String,Parsed)],inc:bool)->Option<Vec<String>> {
        if left==0 {return if vs.iter().any(|v|chosen.iter().all(|&i|eval(&cs[i].1,v,inc))){None}else{Some(chosen.iter().map(|&i|cs[i].0.clone()).collect())};}
        if order.len()-pos<left{return None;}
        for j in pos..=order.len()-left {
            chosen.push(order[j]);let found=search(j+1,left-1,order,chosen,vs,cs,inc);chosen.pop();if found.is_some(){return found;}
        }None
    }
    for k in 1..=cs.len(){if let Some(s)=search(0,k,&order,&mut vec![],vs,cs,inc){return s;}}
    vec![]
}
pub fn select_packages(packages:&[Package],preference:Preference,include_prerelease:bool)->Result<Vec<Selection>,SelectError> {
    let mut seen=BTreeSet::new();
    for p in packages {if !seen.insert(p.name.clone()){return Err(SelectError::DuplicatePackage{name:p.name.clone()});}}
    for p in packages {
        let mut labels=BTreeSet::new();for c in &p.constraints {if !labels.insert(c.label.clone()){return Err(SelectError::DuplicateLabel{package:p.name.clone(),label:c.label.clone()});}}
    }
    let mut versions=vec![];
    for p in packages {
        let vs=p.candidates.iter().enumerate().map(|(index,s)|parse_version(s).map_err(|error|SelectError::InvalidCandidate{package:p.name.clone(),index,error})).collect::<Result<Vec<_>,_>>()?;versions.push(vs);
    }
    let mut constraints=vec![];
    for p in packages {
        let mut cs=vec![];
        for c in &p.constraints {
            let e=parse_expr(&c.expr,&mut vec![]).map_err(|(path,error)|SelectError::InvalidRange{package:p.name.clone(),label:c.label.clone(),path,error})?;
            cs.push((c.label.clone(),e));
        }constraints.push(cs);
    }
    let mut out=vec![];
    for (pi,p) in packages.iter().enumerate() {
        let vs=&versions[pi];let cs=&constraints[pi];let mut best:Option<usize>=None;
        for (i,v) in vs.iter().enumerate() {
            if !cs.iter().all(|(_,e)|eval(e,v,include_prerelease)){continue;}
            let better=best.is_none_or(|j|{
                let old=&vs[j];
                
                let c=compare_versions(v,old);if matches!(preference,Preference::Lowest){c==Ordering::Less}else{c==Ordering::Greater}
            });
            if better {best=Some(i);}
        }
        match best {
            Some(i)=>out.push(Selection{package:p.name.clone(),version:p.candidates[i].clone()}),
            None=>return Err(SelectError::NoSolution{package:p.name.clone(),labels:conflict(vs,cs,include_prerelease)}),
        }
    }Ok(out)
}
