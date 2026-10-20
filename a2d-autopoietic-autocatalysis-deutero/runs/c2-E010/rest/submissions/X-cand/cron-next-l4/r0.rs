use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Minute,
    Hour,
    DayOfMonth,
    Month,
    DayOfWeek,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    FieldCount { found: usize },
    InvalidCharacter { field: Field, offset: usize },
    EmptyItem { field: Field, offset: usize },
    InvalidSyntax { field: Field, offset: usize },
    LeadingZero { field: Field, offset: usize },
    NumberOverflow { field: Field, offset: usize },
    OutOfRange { field: Field, offset: usize },
    ReversedRange { field: Field, offset: usize },
    BadStep { field: Field, offset: usize },
    UnknownSpecial,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    sets: [Vec<u8>; 5],
    unrestricted_dom: bool,
    unrestricted_dow: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CivilDateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatePart {
    Year,
    Month,
    Day,
    Hour,
    Minute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateError {
    InvalidDateTime { component: DatePart },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowError {
    InvalidStart { component: DatePart },
    InvalidEnd { component: DatePart },
    ReversedWindow,
    ZeroCap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FireWindow {
    pub fires: Vec<CivilDateTime>,
    pub truncated: bool,
}

const FIELDS: [Field; 5] = [Field::Minute, Field::Hour, Field::DayOfMonth, Field::Month, Field::DayOfWeek];

fn bounds(f: Field) -> (u32, u32) {
    match f { Field::Minute => (0,59), Field::Hour => (0,23), Field::DayOfMonth => (1,31), Field::Month => (1,12), Field::DayOfWeek => (0,6) }
}
fn idx(f: Field) -> usize {
    match f { Field::Minute=>0, Field::Hour=>1, Field::DayOfMonth=>2, Field::Month=>3, Field::DayOfWeek=>4 }
}
fn parse_num(s: &str, f: Field, off: usize) -> Result<u32, ParseError> {
    if s.len()>1 && s.as_bytes()[0]==b'0' { return Err(ParseError::LeadingZero{field:f,offset:off}); }
    let mut n=0u32;
    for b in s.bytes() {
        n=n.checked_mul(10).and_then(|x|x.checked_add((b-b'0') as u32)).ok_or(ParseError::NumberOverflow{field:f,offset:off})?;
    }
    Ok(n)
}
fn name(s: &str, f: Field) -> Option<u32> {
    let names: &[&str] = match f {
        Field::Month => &["JAN","FEB","MAR","APR","MAY","JUN","JUL","AUG","SEP","OCT","NOV","DEC"],
        Field::DayOfWeek => &["SUN","MON","TUE","WED","THU","FRI","SAT"],
        _ => return None
    };
    names.iter().position(|x| s.eq_ignore_ascii_case(x)).map(|i| if f==Field::Month {i as u32+1} else {i as u32})
}
fn base_value(s: &str, f: Field, off: usize) -> Result<(u32,bool), ParseError> {
    if !s.is_empty() && s.bytes().all(|b|b.is_ascii_digit()) { return Ok((parse_num(s,f,off)?,true)); }
    if let Some(v)=name(s,f) { return Ok((v,false)); }
    Err(ParseError::InvalidSyntax{field:f,offset:off})
}
fn field_parse(text:&str, f:Field)->Result<(Vec<u8>,bool),ParseError>{
    for (i,b) in text.bytes().enumerate() {
        if !(b.is_ascii_digit()||b.is_ascii_alphabetic()||matches!(b,b'*'|b'-'|b'/'|b',')) {
            return Err(ParseError::InvalidCharacter{field:f,offset:i});
        }
    }
    let (lo,hi)=bounds(f);
    let mut bits=vec![false;(hi+1) as usize];
    let mut offset=0usize;
    for item in text.split(',') {
        if item.is_empty(){return Err(ParseError::EmptyItem{field:f,offset});}
        let slashes:Vec<usize>=item.bytes().enumerate().filter_map(|(i,b)|if b==b'/'{Some(i)}else{None}).collect();
        if slashes.len()>1{return Err(ParseError::InvalidSyntax{field:f,offset});}
        let (base,step_s,step_off)=if let Some(&p)=slashes.first(){(&item[..p],Some(&item[p+1..]),offset+p+1)}else{(item,None,0)};
        let hyphens:Vec<usize>=base.bytes().enumerate().filter_map(|(i,b)|if b==b'-'{Some(i)}else{None}).collect();
        if base=="*" {
        } else if hyphens.len()==1 {
            let p=hyphens[0];
            if p==0||p+1==base.len()||&base[..p]=="*"||&base[p+1..]=="*" {return Err(ParseError::InvalidSyntax{field:f,offset});}
            base_value(&base[..p],f,offset)?;
            base_value(&base[p+1..],f,offset+p+1)?;
        } else if hyphens.is_empty() {
            base_value(base,f,offset)?;
        } else { return Err(ParseError::InvalidSyntax{field:f,offset}); }
        if let Some(st)=step_s {
            if st.is_empty() || !st.bytes().all(|b|b.is_ascii_digit()){return Err(ParseError::InvalidSyntax{field:f,offset});}
        }
        let (a,b,an,bn,star)=if base=="*" {(lo,hi,false,false,true)} else if let Some(&p)=hyphens.first() {
            let (a,an)=base_value(&base[..p],f,offset)?;
            let (b,bn)=base_value(&base[p+1..],f,offset+p+1)?;
            (a,b,an,bn,false)
        } else {let(a,an)=base_value(base,f,offset)?;(a,a,an,an,false)};
        if an && !(lo..=hi).contains(&a){return Err(ParseError::OutOfRange{field:f,offset});}
        if bn && hyphens.len()==1 && !(lo..=hi).contains(&b){return Err(ParseError::OutOfRange{field:f,offset:offset+hyphens[0]+1});}
        if hyphens.len()==1&&a>b{return Err(ParseError::ReversedRange{field:f,offset:offset+hyphens[0]+1});}
        let step=if let Some(st)=step_s {
            let v=parse_num(st,f,step_off)?;
            if v==0||v>hi-lo+1{return Err(ParseError::BadStep{field:f,offset:step_off});} v
        } else {1};
        let (start,end)=if step_s.is_some()&&!star&&hyphens.is_empty(){(a,hi)}else{(a,b)};
        let mut v=start;
        while v<=end {bits[v as usize]=true; match v.checked_add(step){Some(n)=>v=n,None=>break}}
        offset += item.len()+1;
    }
    Ok(((lo..=hi).filter(|v|bits[*v as usize]).map(|v|v as u8).collect(),text=="*"))
}
pub fn parse_cron(input:&str)->Result<Cron,ParseError>{
    if input.as_bytes().first()==Some(&b'@'){
        let s=match input {"@hourly"=>"0 * * * *","@daily"=>"0 0 * * *","@weekly"=>"0 0 * * 0","@monthly"=>"0 0 1 * *","@yearly"=>"0 0 1 1 *",_=>return Err(ParseError::UnknownSpecial)};
        return parse_fields(s);
    }
    parse_fields(input)
}
fn parse_fields(s:&str)->Result<Cron,ParseError>{
    let v:Vec<&str>=s.split(' ').collect();
    if v.len()!=5{return Err(ParseError::FieldCount{found:v.len()});}
    let mut sets:[Vec<u8>;5]=std::array::from_fn(|_|Vec::new());
    let mut flags=[false;5];
    for i in 0..5 {let(x,u)=field_parse(v[i],FIELDS[i])?;sets[i]=x;flags[i]=u;}
    Ok(Cron{sets,unrestricted_dom:flags[2],unrestricted_dow:flags[4]})
}
fn contains(v:&[u8],x:u8)->bool{v.binary_search(&x).is_ok()}
fn leap(y:u16)->bool{y%4==0&&(y%100!=0||y%400==0)}
fn ml(y:u16,m:u8)->u8{match m{2=>if leap(y){29}else{28},4|6|9|11=>30,_=>31}}
fn wd(y:u16,m:u8,d:u8)->u8{
    let mut n=0u32; for yy in 2000..y{n+=if leap(yy){366}else{365};}
    for mm in 1..m{n+=ml(y,mm) as u32;} n+=d as u32-1; ((n+6)%7) as u8
}
fn validate(t:CivilDateTime)->Result<(),DateError>{
    if !(2000..=2399).contains(&t.year){return Err(DateError::InvalidDateTime{component:DatePart::Year});}
    if !(1..=12).contains(&t.month){return Err(DateError::InvalidDateTime{component:DatePart::Month});}
    if t.day==0||t.day>ml(t.year,t.month){return Err(DateError::InvalidDateTime{component:DatePart::Day});}
    if t.hour>23{return Err(DateError::InvalidDateTime{component:DatePart::Hour});}
    if t.minute>59{return Err(DateError::InvalidDateTime{component:DatePart::Minute});}
    Ok(())
}
fn day_ok(c:&Cron,y:u16,m:u8,d:u8)->bool{
    let a=contains(&c.sets[2],d);let b=contains(&c.sets[4],wd(y,m,d));
    match(c.unrestricted_dom,c.unrestricted_dow){(true,true)=>true,(true,false)=>b,(false,true)=>a,(false,false)=>if d==ml(y,m){a&&b}else{a||b}}
}
fn nd((y,m,d):(u16,u8,u8))->Option<(u16,u8,u8)>{if d<ml(y,m){Some((y,m,d+1))}else if m<12{Some((y,m+1,1))}else if y<2399{Some((y+1,1,1))}else{None}}
fn pd((y,m,d):(u16,u8,u8))->Option<(u16,u8,u8)>{if d>1{Some((y,m,d-1))}else if m>1{Some((y,m-1,ml(y,m-1)))}else if y>2000{Some((y-1,12,31))}else{None}}
impl Cron{
    pub fn allowed(&self,f:Field)->Vec<u8>{self.sets[idx(f)].clone()}
    pub fn matches(&self,t:CivilDateTime)->Result<bool,DateError>{validate(t)?;Ok(contains(&self.sets[0],t.minute)&&contains(&self.sets[1],t.hour)&&contains(&self.sets[3],t.month)&&day_ok(self,t.year,t.month,t.day))}
    pub fn next_fire(&self,after:CivilDateTime)->Result<Option<CivilDateTime>,DateError>{
        validate(after)?;let mut date=(after.year,after.month,after.day);let mut first=true;
        loop{
            if contains(&self.sets[3],date.1)&&day_ok(self,date.0,date.1,date.2){
                for &h in &self.sets[1]{if first&&h<after.hour{continue;}
                    for &m in &self.sets[0]{if first&&h==after.hour&&m<=after.minute{continue;}
                        return Ok(Some(CivilDateTime{year:date.0,month:date.1,day:date.2,hour:h,minute:m}));
                    }
                }
            }
            match nd(date){Some(x)=>{date=x;first=false},None=>return Ok(None)}
        }
    }
    pub fn previous_fire(&self,before:CivilDateTime)->Result<Option<CivilDateTime>,DateError>{
        validate(before)?;let mut date=(before.year,before.month,before.day);let mut first=true;
        loop{
            if contains(&self.sets[3],date.1)&&day_ok(self,date.0,date.1,date.2){
                for &h in self.sets[1].iter().rev(){if first&&h>before.hour{continue;}
                    for &m in self.sets[0].iter().rev(){if first&&h==before.hour&&m>=before.minute{continue;}
                        return Ok(Some(CivilDateTime{year:date.0,month:date.1,day:date.2,hour:h,minute:m}));
                    }
                }
            }
            match pd(date){Some(x)=>{date=x;first=false},None=>return Ok(None)}
        }
    }
    pub fn fires_between(&self,start:CivilDateTime,end:CivilDateTime,cap:usize)->Result<FireWindow,WindowError>{
        validate(start).map_err(|e|match e{DateError::InvalidDateTime{component}=>WindowError::InvalidStart{component}})?;
        validate(end).map_err(|e|match e{DateError::InvalidDateTime{component}=>WindowError::InvalidEnd{component}})?;
        if start>end{return Err(WindowError::ReversedWindow);}if cap==0{return Err(WindowError::ZeroCap);}
        if start==end{return Ok(FireWindow{fires:Vec::new(),truncated:false});}
        let mut fires=Vec::new();let mut cursor=start;
        loop{
            let candidate=if self.matches_valid(cursor){Some(cursor)}else{self.next_fire(cursor).ok().flatten()};
            match candidate{Some(t) if t<end=>{if fires.len()==cap{return Ok(FireWindow{fires,truncated:true});}fires.push(t);if t.minute<59{cursor=CivilDateTime{minute:t.minute+1,..t};}else if t.hour<23{cursor=CivilDateTime{hour:t.hour+1,minute:0,..t};}else if let Some((y,m,d))=nd((t.year,t.month,t.day)){cursor=CivilDateTime{year:y,month:m,day:d,hour:0,minute:0};}else{break;}},_=>break}
        }
        Ok(FireWindow{fires,truncated:false})
    }
    fn matches_valid(&self,t:CivilDateTime)->bool{contains(&self.sets[0],t.minute)&&contains(&self.sets[1],t.hour)&&contains(&self.sets[3],t.month)&&day_ok(self,t.year,t.month,t.day)}
    pub fn canonical(&self)->String{
        let mut fields=Vec::new();
        for i in 0..5{let f=FIELDS[i];let(lo,hi)=bounds(f);let full=self.sets[i].len()==(hi-lo+1) as usize;
            fields.push(if (i==2&&self.unrestricted_dom)||(i==4&&self.unrestricted_dow){"*".to_string()}else if full&&i!=2&&i!=4{"*".to_string()}else if full{"*/1".to_string()}else{self.sets[i].iter().map(|x|x.to_string()).collect::<Vec<_>>().join(",")});
        } fields.join(" ")
    }
}
