pub const MAX_DIM: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Row,
    Column,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entry {
    pub row: usize,
    pub col: usize,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sparse {
    rows: usize,
    cols: usize,
    entries: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Csr {
    rows: usize,
    cols: usize,
    row_ptr: Vec<usize>,
    col_idx: Vec<usize>,
    values: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextError {
    Newline, Header, Decimal, Dimension, Count, Record, Hex,
    RowBounds, ColumnBounds, NonFinite, Zero, Order,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    Shape { axis: Axis, size: usize },
    Bounds { entry: usize, axis: Axis, index: usize, limit: usize },
    NonFinite { entry: usize },
    SumOverflow { entry: usize },
    RowPtrLength { expected: usize, actual: usize },
    ColumnValueLength { columns: usize, values: usize },
    RowPtrStart { found: usize },
    RowPtrEnd { found: usize, expected: usize },
    RowPtrOrder { row: usize },
    ColumnBounds { slot: usize, column: usize, limit: usize },
    ColumnOrder { slot: usize },
    NonFiniteValue { slot: usize },
    ExplicitZero { slot: usize },
    VectorLength { expected: usize, actual: usize },
    NonFiniteVector { index: usize },
    ProductOverflow { row: usize, column: usize },
    AccumulationOverflow { row: usize, column: usize },
    DimensionMismatch { left_cols: usize, right_rows: usize },
    MultiplyOverflow { row: usize, inner: usize, column: usize },
    MultiplySumOverflow { row: usize, inner: usize, column: usize },
    Tolerance,
    Text { line: usize, kind: TextError },
}

fn shape(rows: usize, cols: usize) -> Result<(), Error> {
    if rows > MAX_DIM { Err(Error::Shape { axis: Axis::Row, size: rows }) }
    else if cols > MAX_DIM { Err(Error::Shape { axis: Axis::Column, size: cols }) }
    else { Ok(()) }
}

impl Sparse {
    pub fn from_coo(rows: usize, cols: usize, entries: &[Entry]) -> Result<Self, Error> {
        shape(rows, cols)?;
        for (i, e) in entries.iter().enumerate() {
            if e.row >= rows { return Err(Error::Bounds { entry:i, axis:Axis::Row, index:e.row, limit:rows }); }
            if e.col >= cols { return Err(Error::Bounds { entry:i, axis:Axis::Column, index:e.col, limit:cols }); }
            if !e.value.is_finite() { return Err(Error::NonFinite { entry:i }); }
        }
        let mut sums = std::collections::BTreeMap::<(usize,usize), f64>::new();
        for (i,e) in entries.iter().enumerate() {
            let sum = sums.entry((e.row,e.col)).or_insert(0.0);
            *sum += e.value;
            if !sum.is_finite() { return Err(Error::SumOverflow { entry:i }); }
        }
        let entries = sums.into_iter().filter_map(|((row,col),value)| {
            if value == 0.0 { None } else { Some(Entry { row,col,value }) }
        }).collect();
        Ok(Self { rows, cols, entries })
    }
    pub fn shape(&self)->(usize,usize){(self.rows,self.cols)}
    pub fn entries(&self)->&[Entry]{&self.entries}
    pub fn to_csr(&self)->Csr {
        let mut rp=vec![0;self.rows+1];
        let mut ci=Vec::with_capacity(self.entries.len());
        let mut vs=Vec::with_capacity(self.entries.len());
        let mut pos=0;
        for r in 0..self.rows {
            while pos<self.entries.len() && self.entries[pos].row==r {
                ci.push(self.entries[pos].col); vs.push(self.entries[pos].value); pos+=1;
            }
            rp[r+1]=pos;
        }
        Csr{rows:self.rows,cols:self.cols,row_ptr:rp,col_idx:ci,values:vs}
    }
}

impl Csr {
    pub fn from_parts(rows:usize,cols:usize,row_ptr:Vec<usize>,col_idx:Vec<usize>,values:Vec<f64>)->Result<Self,Error>{
        shape(rows,cols)?;
        if row_ptr.len()!=rows+1{return Err(Error::RowPtrLength{expected:rows+1,actual:row_ptr.len()});}
        if col_idx.len()!=values.len(){return Err(Error::ColumnValueLength{columns:col_idx.len(),values:values.len()});}
        if row_ptr[0]!=0{return Err(Error::RowPtrStart{found:row_ptr[0]});}
        if row_ptr[rows]!=values.len(){return Err(Error::RowPtrEnd{found:row_ptr[rows],expected:values.len()});}
        for r in 0..rows {if row_ptr[r]>row_ptr[r+1]{return Err(Error::RowPtrOrder{row:r});}}
        for r in 0..rows {
            let mut prev=None;
            for s in row_ptr[r]..row_ptr[r+1] {
                let c=col_idx[s];
                if c>=cols{return Err(Error::ColumnBounds{slot:s,column:c,limit:cols});}
                if prev.is_some_and(|p|c<=p){return Err(Error::ColumnOrder{slot:s});}
                if !values[s].is_finite(){return Err(Error::NonFiniteValue{slot:s});}
                if values[s]==0.0{return Err(Error::ExplicitZero{slot:s});}
                prev=Some(c);
            }
        }
        Ok(Self{rows,cols,row_ptr,col_idx,values})
    }
    pub fn shape(&self)->(usize,usize){(self.rows,self.cols)}
    pub fn row_ptr(&self)->&[usize]{&self.row_ptr}
    pub fn col_indices(&self)->&[usize]{&self.col_idx}
    pub fn values(&self)->&[f64]{&self.values}
    pub fn to_coo(&self)->Sparse{
        let mut es=Vec::with_capacity(self.values.len());
        for r in 0..self.rows {for s in self.row_ptr[r]..self.row_ptr[r+1]{es.push(Entry{row:r,col:self.col_idx[s],value:self.values[s]});}}
        Sparse{rows:self.rows,cols:self.cols,entries:es}
    }
    pub fn mul_vector(&self,x:&[f64])->Result<Vec<f64>,Error>{
        if x.len()!=self.cols{return Err(Error::VectorLength{expected:self.cols,actual:x.len()});}
        for(i,v)in x.iter().enumerate(){if !v.is_finite(){return Err(Error::NonFiniteVector{index:i});}}
        let mut out=Vec::with_capacity(self.rows);
        for r in 0..self.rows {
            let mut a=0.0;
            for s in self.row_ptr[r]..self.row_ptr[r+1] {
                let c=self.col_idx[s]; let p=self.values[s]*x[c];
                if !p.is_finite(){return Err(Error::ProductOverflow{row:r,column:c});}
                a+=p;
                if !a.is_finite(){return Err(Error::AccumulationOverflow{row:r,column:c});}
            }
            out.push(if a==0.0{0.0}else{a});
        }
        Ok(out)
    }
    pub fn transpose(&self)->Csr{
        let mut rp=vec![0;self.cols+1];
        for &c in &self.col_idx{rp[c+1]+=1;}
        for r in 0..self.cols{rp[r+1]+=rp[r];}
        let mut next=rp[..self.cols].to_vec();
        let mut ci=vec![0;self.values.len()]; let mut vs=vec![0.0;self.values.len()];
        for r in 0..self.rows {for s in self.row_ptr[r]..self.row_ptr[r+1] {
            let c=self.col_idx[s];let p=next[c];ci[p]=r;vs[p]=self.values[s];next[c]+=1;
        }}
        Csr{rows:self.cols,cols:self.rows,row_ptr:rp,col_idx:ci,values:vs}
    }
    pub fn multiply(&self,rhs:&Csr)->Result<Csr,Error>{
        if self.cols!=rhs.rows{return Err(Error::DimensionMismatch{left_cols:self.cols,right_rows:rhs.rows});}
        let mut rp=vec![0];let mut ci=Vec::new();let mut vs=Vec::new();
        for r in 0..self.rows {
            let mut sums=vec![0.0;rhs.cols];
            let mut touched=vec![false;rhs.cols];
            for ls in self.row_ptr[r]..self.row_ptr[r+1] {
                let k=self.col_idx[ls];
                for rs in rhs.row_ptr[k]..rhs.row_ptr[k+1] {
                    let c=rhs.col_idx[rs];let p=self.values[ls]*rhs.values[rs];
                    if !p.is_finite(){return Err(Error::MultiplyOverflow{row:r,inner:k,column:c});}
                    sums[c]+=p;touched[c]=true;
                    if !sums[c].is_finite(){return Err(Error::MultiplySumOverflow{row:r,inner:k,column:c});}
                }
            }
            for c in 0..rhs.cols {if touched[c]&&sums[c]!=0.0{ci.push(c);vs.push(sums[c]);}}
            rp.push(vs.len());
        }
        Ok(Csr{rows:self.rows,cols:rhs.cols,row_ptr:rp,col_idx:ci,values:vs})
    }
    pub fn prune(&self,tolerance:f64)->Result<Csr,Error>{
        if !tolerance.is_finite()||tolerance<0.0{return Err(Error::Tolerance);}
        let mut rp=vec![0];let mut ci=Vec::new();let mut vs=Vec::new();
        for r in 0..self.rows {for s in self.row_ptr[r]..self.row_ptr[r+1]{
            if self.values[s].abs()>tolerance{ci.push(self.col_idx[s]);vs.push(self.values[s]);}
        }rp.push(vs.len());}
        Ok(Csr{rows:self.rows,cols:self.cols,row_ptr:rp,col_idx:ci,values:vs})
    }
    pub fn write_text(&self)->String{
        use std::fmt::Write;
        let mut s=String::new();writeln!(&mut s,"SM1 {} {} {}",self.rows,self.cols,self.values.len()).unwrap();
        for r in 0..self.rows {for i in self.row_ptr[r]..self.row_ptr[r+1]{writeln!(&mut s,"{} {} {:016x}",r,self.col_idx[i],self.values[i].to_bits()).unwrap();}}
        s
    }
    pub fn parse_text(text:&str)->Result<Csr,Error>{
        fn err(line:usize,kind:TextError)->Error{Error::Text{line,kind}}
        fn dec(s:&str)->Option<usize>{
            if s.is_empty()||!s.bytes().all(|b|b.is_ascii_digit())||(s.len()>1&&s.starts_with('0')){None}else{s.parse().ok()}
        }
        if !text.ends_with('\n'){return Err(err(1+text.bytes().filter(|b|*b==b'\n').count(),TextError::Newline));}
        let lines:Vec<&str>=text[..text.len()-1].split('\n').collect();
        let h:Vec<&str>=lines[0].split(' ').collect();
        if h.len()!=4||h.iter().any(|x|x.is_empty())||h[0]!="SM1"{return Err(err(1,TextError::Header));}
        let rows=dec(h[1]).ok_or_else(||err(1,TextError::Decimal))?;
        let cols=dec(h[2]).ok_or_else(||err(1,TextError::Decimal))?;
        let n=dec(h[3]).ok_or_else(||err(1,TextError::Decimal))?;
        if rows>MAX_DIM||cols>MAX_DIM{return Err(err(1,TextError::Dimension));}
        if n!=lines.len()-1{return Err(err(1,TextError::Count));}
        let mut rp=vec![0;rows+1];let mut ci=Vec::with_capacity(n);let mut vs=Vec::with_capacity(n);
        let mut prior=None;let mut cr=0;
        for (i,line) in lines.iter().enumerate().skip(1){
            let ln=i+1;let f:Vec<&str>=line.split(' ').collect();
            if f.len()!=3||f.iter().any(|x|x.is_empty()){return Err(err(ln,TextError::Record));}
            let r=dec(f[0]).ok_or_else(||err(ln,TextError::Decimal))?;
            let c=dec(f[1]).ok_or_else(||err(ln,TextError::Decimal))?;
            if f[2].len()!=16||!f[2].bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)){return Err(err(ln,TextError::Hex));}
            let v=f64::from_bits(u64::from_str_radix(f[2],16).unwrap());
            if r>=rows{return Err(err(ln,TextError::RowBounds));}
            if c>=cols{return Err(err(ln,TextError::ColumnBounds));}
            if !v.is_finite(){return Err(err(ln,TextError::NonFinite));}
            if v==0.0{return Err(err(ln,TextError::Zero));}
            if prior.is_some_and(|p|(r,c)<=p){return Err(err(ln,TextError::Order));}
            while cr<r{rp[cr+1]=ci.len();cr+=1;}
            ci.push(c);vs.push(v);prior=Some((r,c));
        }
        while cr<rows{rp[cr+1]=ci.len();cr+=1;}
        Ok(Csr{rows,cols,row_ptr:rp,col_idx:ci,values:vs})
    }
}