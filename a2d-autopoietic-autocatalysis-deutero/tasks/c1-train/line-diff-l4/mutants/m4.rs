
#[derive(Debug, Clone, PartialEq, Eq)]
enum Edit {
    Keep { old_index: usize, new_index: usize, line: String },
    Delete { old_index: usize, line: String },
    Insert { new_index: usize, line: String },
}
fn script(old: &[String], new: &[String]) -> Vec<Edit> {
    let (n,m)=(old.len(),new.len());
    let mut dp=vec![vec![0usize;m+1];n+1];
    for i in (0..n).rev() { for j in (0..m).rev() {
        dp[i][j]=if old[i]==new[j] {1+dp[i+1][j+1]} else {dp[i+1][j].max(dp[i][j+1])};
    }}
    let (mut i,mut j)=(0,0); let mut out=Vec::new();
    while i<n || j<m {
        if i<n && j<m && old[i]==new[j] {
            out.push(Edit::Keep{old_index:i,new_index:j,line:old[i].clone()}); i+=1;j+=1;
        } else if i<n && (j==m || dp[i+1][j]>dp[i][j+1] || (dp[i+1][j]==dp[i][j+1] && (i+j)%2==0)) {
            out.push(Edit::Delete{old_index:i,line:old[i].clone()}); i+=1;
        } else { out.push(Edit::Insert{new_index:j,line:new[j].clone()}); j+=1; }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeSide { Base, Left, Right }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution { Mark, PreferLeft, PreferRight }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeError {
    CarriageReturn { side: MergeSide, byte: usize }, MissingFinalLf { side: MergeSide },
    TooLarge { base_len: usize, left_len: usize, right_len: usize },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict { pub ordinal:usize, pub base_start:usize, pub base_len:usize, pub output_start:usize, pub output_len:usize }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeResult { pub text:String, pub conflicts:usize, pub regions:Vec<Conflict> }
struct Change { start:usize, end:usize, replacement:Vec<String>, side:usize, order:usize }
fn changes(base:&[String],other:&[String],side:usize)->Vec<Change> {
    let mut pos=0;let mut out:Vec<Change>=Vec::new();let mut current:Option<Change>=None;
    for e in script(base,other) {
        match e {
            Edit::Keep{..}=>{if let Some(c)=current.take(){out.push(c);}pos+=1;},
            Edit::Delete{..}=>{let c=current.get_or_insert_with(||Change{start:pos,end:pos,replacement:Vec::new(),side,order:out.len()});pos+=1;c.end=pos;},
            Edit::Insert{line,..}=>{let c=current.get_or_insert_with(||Change{start:pos,end:pos,replacement:Vec::new(),side,order:out.len()});c.replacement.push(line);},
        }
    }
    if let Some(c)=current {out.push(c);}out
}
fn interacts(a:&Change,b:&Change)->bool {
    match (a.start==a.end,b.start==b.end) {
        (true,true)=>a.start==b.start,
        (false,false)=>a.start.max(b.start)<a.end.min(b.end),
        (true,false)=>b.start<a.start && a.start<b.end,
        (false,true)=>a.start<b.start && b.start<a.end,
    }
}
fn root(parent:&[usize],mut i:usize)->usize {while parent[i]!=i {i=parent[i];}i}
fn projection(base:&[String],all:&[Change],indices:&[usize],start:usize,end:usize,side:usize)->Vec<String> {
    let mut relevant:Vec<&Change>=indices.iter().map(|&i|&all[i]).filter(|c|c.side==side).collect();
    relevant.sort_by_key(|c|c.order);
    let mut out=Vec::new();let mut cursor=start;
    for c in relevant {out.extend_from_slice(&base[cursor..c.start]);out.extend(c.replacement.iter().cloned());cursor=c.end;}
    out.extend_from_slice(&base[cursor..end]);out
}
pub fn merge_three_way(base:&str,left:&str,right:&str,resolution:Resolution)->Result<MergeResult,MergeError> {
    for (side,text) in [(MergeSide::Base,base),(MergeSide::Left,left),(MergeSide::Right,right)] {
        if let Some(byte)=text.find('\r'){return Err(MergeError::CarriageReturn{side,byte});}
    }
    for (side,text) in [(MergeSide::Base,base),(MergeSide::Left,left),(MergeSide::Right,right)] {
        if !text.is_empty() && !text.ends_with('\n'){return Err(MergeError::MissingFinalLf{side});}
    }
    let b:Vec<String>=base.split_terminator('\n').map(str::to_owned).collect();
    let l:Vec<String>=left.split_terminator('\n').map(str::to_owned).collect();
    let r:Vec<String>=right.split_terminator('\n').map(str::to_owned).collect();
    if b.len()>128 || l.len()>128 || r.len()>128 {return Err(MergeError::TooLarge{base_len:b.len(),left_len:l.len(),right_len:r.len()});}
    let mut all=changes(&b,&l,0);all.extend(changes(&b,&r,1));
    let mut parent:Vec<usize>=(0..all.len()).collect();
    for i in 0..all.len(){for j in i+1..all.len(){if interacts(&all[i],&all[j]){let a=root(&parent,i);let z=root(&parent,j);parent[z]=a;}}}
    let mut components:Vec<Vec<usize>>=Vec::new();
    for i in 0..all.len() {
        let ri=root(&parent,i);
        if let Some(group)=components.iter_mut().find(|g|root(&parent,g[0])==ri){group.push(i);}else{components.push(vec![i]);}
    }
    components.sort_by_key(|g| {let s=g.iter().map(|&i|all[i].start).min().unwrap();let e=g.iter().map(|&i|all[i].end).max().unwrap();(s,e!=s)});
    let mut lines:Vec<String>=Vec::new();let mut regions=Vec::new();let mut cursor=0;
    for group in components {
        let start=group.iter().map(|&i|all[i].start).min().unwrap();let end=group.iter().map(|&i|all[i].end).max().unwrap();
        lines.extend_from_slice(&b[cursor..start]);
        let bp=&b[start..end];let lp=projection(&b,&all,&group,start,end,0);let rp=projection(&b,&all,&group,start,end,1);
        if lp==rp {lines.extend(lp);} else if lp==bp {lines.extend(rp);}else if rp==bp {lines.extend(lp);}else {
            match resolution {
                Resolution::PreferLeft=>lines.extend(lp),Resolution::PreferRight=>lines.extend(rp),
                Resolution::Mark=>{
                    let ordinal=regions.len()+1;let output_start=lines.iter().map(|s|s.len()+1).sum();let output_len=4+lp.len()+bp.len()+rp.len();
                    regions.push(Conflict{ordinal,base_start:start,base_len:end-start,output_start,output_len});
                    lines.push(format!("<<<<<<< LEFT {ordinal}"));lines.extend(lp);
                    let s=if end==start {start}else{start+1};
                    lines.push(format!("||||||| BASE {s},{}",end-start));lines.extend_from_slice(bp);
                    lines.push("=======".into());lines.extend(rp);lines.push(format!(">>>>>>> RIGHT {ordinal}"));
                }
            }
        }
        cursor=end;
    }
    lines.extend_from_slice(&b[cursor..]);
    let mut text=String::new();for line in lines {text.push_str(&line);text.push('\n');}
    Ok(MergeResult{text,conflicts:regions.len(),regions})
}
