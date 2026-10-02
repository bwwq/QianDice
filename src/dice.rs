use anyhow::{Result,bail,ensure};
use rand::Rng;
use serde::{Serialize,Deserialize};

#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct Roll {pub expression:String,pub total:i64,pub detail:Vec<String>}
pub fn roll(expression:&str,default_faces:u32)->Result<Roll>{evaluate(expression,default_faces,&mut rand::thread_rng())}
pub fn evaluate(expression:&str,default_faces:u32,rng:&mut impl Rng)->Result<Roll>{
    ensure!(expression.len()<=512,"骰式过长");
    let normalized=expression.trim().to_ascii_lowercase().replace(' ',"");
    let text=if normalized.is_empty(){format!("1d{default_faces}")}else{normalized};
    let mut p=Parser{s:text.as_bytes(),pos:0,rng,budget:1000,depth:0,detail:vec![],default_faces};
    let total=p.expr(0)?;ensure!(p.pos==p.s.len(),"无法识别骰式位置 {}",p.pos+1);
    Ok(Roll{expression:text,total,detail:p.detail})
}
struct Parser<'a,R>{s:&'a[u8],pos:usize,rng:&'a mut R,budget:u32,depth:u32,detail:Vec<String>,default_faces:u32}
impl<R:Rng> Parser<'_,R>{
    fn peek(&self)->u8{self.s.get(self.pos).copied().unwrap_or(0)}
    fn take(&mut self,c:u8)->bool{if self.peek()==c{self.pos+=1;true}else{false}}
    fn number(&mut self)->Result<i64>{let start=self.pos;while self.peek().is_ascii_digit(){self.pos+=1;}ensure!(start!=self.pos,"需要数字");Ok(std::str::from_utf8(&self.s[start..self.pos])?.parse()?)}
    fn expr(&mut self,min:u8)->Result<i64>{
        self.depth+=1;ensure!(self.depth<=32,"括号层数过多");
        let mut left=if self.take(b'-'){-self.expr(5)?}else if self.take(b'+'){self.expr(5)?}else if self.take(b'('){let n=self.expr(0)?;ensure!(self.take(b')'),"缺少右括号");n}else if self.peek()==b'd'{self.dice(1)?}else if self.peek()==b'b'||self.peek()==b'p'{let penalty=self.take(b'p');if !penalty{self.pos+=1;}let n=if self.peek().is_ascii_digit(){self.number()?}else{1};ensure!((0..=10).contains(&n),"奖惩骰数量应为0到10");let (v,detail)=percentile(n as u32,penalty,self.rng);self.detail.push(detail);v as i64}else{let n=self.number()?;if self.peek()==b'd'{self.dice(n)?}else{n}};
        loop {let op=self.peek();let binding=match op{b'+'|b'-'=>1,b'*'|b'/'=>3,_=>0};if binding==0||binding<min{break}self.pos+=1;let right=self.expr(binding+1)?;left=match op{b'+'=>left.checked_add(right),b'-'=>left.checked_sub(right),b'*'=>left.checked_mul(right),b'/'=>left.checked_div(right),_=>None}.ok_or_else(||anyhow::anyhow!("计算溢出或除数为零"))?;ensure!(left.unsigned_abs()<=1_000_000_000,"结果超出限制");}
        self.depth-=1;Ok(left)
    }
    fn dice(&mut self,n:i64)->Result<i64>{
        self.pos+=1;let faces=if self.peek().is_ascii_digit(){self.number()?}else{self.default_faces as i64};
        ensure!((1..=1000).contains(&n)&&(1..=1_000_000).contains(&faces),"骰数或面数超出限制");ensure!(n<=self.budget as i64,"一次最多投掷1000颗骰子");self.budget-=n as u32;
        let mut values:Vec<i64>=(0..n).map(|_|self.rng.gen_range(1..=faces)).collect();let original=values.clone();
        let mut keep=n as usize;
        if self.take(b'k'){let low=self.take(b'l');if !low{self.take(b'h');}let k=self.number()?;ensure!(k>0&&k<=n,"保留数量必须在1到骰数之间");keep=k as usize;values.sort_unstable();if !low{values.reverse();}}
        let total=values[..keep].iter().sum();self.detail.push(format!("{n}d{faces}={original:?}{}",if keep<n as usize{format!(" → 保留 {:?}",&values[..keep])}else{String::new()}));Ok(total)
    }
}
pub fn percentile(extra:u32,penalty:bool,rng:&mut impl Rng)->(u32,String){
    let unit=rng.gen_range(0..10);let values:Vec<u32>=(0..=extra).map(|_|{let n=rng.gen_range(0..10)*10+unit;if n==0{100}else{n}}).collect();
    let value=if penalty{*values.iter().max().unwrap()}else{*values.iter().min().unwrap()};
    (value,format!("{}{} {:?} → {value}",if penalty{"惩罚"}else{"奖励"},extra,values))
}
pub fn max_value(expression:&str)->Result<i64>{
    struct Maximum;impl rand::RngCore for Maximum{fn next_u32(&mut self)->u32{u32::MAX-1}fn next_u64(&mut self)->u64{u64::MAX-1}fn fill_bytes(&mut self,d:&mut[u8]){d.fill(254)}fn try_fill_bytes(&mut self,d:&mut[u8])->std::result::Result<(),rand::Error>{self.fill_bytes(d);Ok(())}}
    // SAN loss expressions only admit nonnegative NdM terms joined by +.
    let mut sum=0i64;
    for part in expression.to_lowercase().split('+') {let part=part.trim();let n=if let Some((n,m))=part.split_once('d'){let count=if n.is_empty(){1}else{n.parse::<i64>()?};let faces=m.parse::<i64>()?;ensure!(count>0&&count<=1000&&faces>0&&faces<=1_000_000,"损失骰式超出限制");count.checked_mul(faces).ok_or_else(||anyhow::anyhow!("损失溢出"))?}else{part.parse()?};if n<0{bail!("损失不能为负数")}sum=sum.checked_add(n).ok_or_else(||anyhow::anyhow!("损失溢出"))?;}
    Ok(sum)
}
#[cfg(test)]mod tests{
use super::*;use rand::{SeedableRng,rngs::StdRng};
#[test]fn deterministic_bounds(){let mut r=StdRng::seed_from_u64(7);for _ in 0..100{let v=evaluate("4d6k3+2",100,&mut r).unwrap().total;assert!((5..=20).contains(&v));}assert!(evaluate("1001d6",100,&mut r).is_err());assert!(evaluate("1/0",100,&mut r).is_err());assert_eq!(evaluate("2*(3+4)",100,&mut r).unwrap().total,14);}
#[test]fn bonus_never_worsens_same_sample(){for seed in 0..50{let a=percentile(2,false,&mut StdRng::seed_from_u64(seed)).0;let b=percentile(2,true,&mut StdRng::seed_from_u64(seed)).0;assert!(a<=b);assert!((1..=100).contains(&a));}}
}
