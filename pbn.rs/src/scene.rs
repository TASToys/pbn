use super::get_scene_key;
use super::error::Error;
use md5::compute;
use postgres::types::{Type, IsNull, FromSql, ToSql};
use rocket::request::FromParam;
use rocket::http::RawStr;
use std::error::Error as ErrorTrait;

fn random_f(n: u32, seed: &[u8]) -> u32
{
	let mut buf = [0; 55];
	(&mut buf[..seed.len()]).copy_from_slice(&seed[..]);
	buf[seed.len()+0] = (n >> 16) as u8;
	buf[seed.len()+1] = (n >> 8) as u8;
	buf[seed.len()+2] = (n >> 0) as u8;
	let res = compute(&buf[..seed.len()+3]);
	let res = res.as_ref();
	(res[0] as u32 & 127) * 256 + (res[1] as u32) 
}

fn permute(n: i32) -> [u8;6]
{
	let seed: Vec<u8> = get_scene_key();
	let n = n as u32;
	let l = n >> 15;
	let r = n & 0x7FFF;
	let r = r ^ random_f(0x00000 | l, &seed);
	let l = l ^ random_f(0x08000 | r, &seed);
	let r = r ^ random_f(0x10000 | l, &seed);
	let l = l ^ random_f(0x18000 | r, &seed);
	let n = (l << 15) | r;
	let mut ret = [0;6];
	static LETTERS: &'static [u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
	for i in 0..6 {
		ret[i] = LETTERS[((n >> (5 * i)) & 31) as usize];
	}
	ret
}

fn unpermute(s: &[u8]) -> i32
{
	let seed: Vec<u8> = get_scene_key();
	let mut n = 0;
	for i in 0..6 {
		let c = s[i];
		n = n | ((c - match s[i] {
			65...90 => 65,
			97...122 => 97,
			50...55 => 24,
			_ => c,
		}) as u32) << 5 * i;
	}
	let l = n >> 15;
	let r = n & 0x7FFF;
	let l = l ^ random_f(0x18000 | r, &seed);
	let r = r ^ random_f(0x10000 | l, &seed);
	let l = l ^ random_f(0x08000 | r, &seed);
	let r = r ^ random_f(0x00000 | l, &seed);
	((l << 15) | r) as i32
}

#[derive(Copy,Clone,Debug,PartialEq,Eq)]
pub struct Scene(i32);

impl Scene
{
	pub fn new(num: i32) -> Scene
	{
		Scene(num)
	}
	pub fn scramble(&self) -> [u8;6]
	{
		permute(self.0)
	}
	pub fn as_inner(&self) -> i32
	{
		self.0
	}
}

impl<'a> FromParam<'a> for Scene
{
	type Error = Error;
	fn from_param(param: &'a RawStr) -> Result<Self, Self::Error>
	{
		let param = param.as_str().as_bytes();
		for i in param.iter() {
			let i = *i;
			if !((i >= 50 && i <= 55) || (i >= 65 && i <= 90) || (i >= 97 && i <= 122)) {
				return Err(Error::SceneNotFound);
			}
		}
		if param.len() != 6 { return Err(Error::SceneNotFound); }
		Ok(Scene(unpermute(param)))
	}
}

impl FromSql for Scene
{
	fn from_sql(ty: &Type, raw: &[u8]) -> Result<Self, Box<ErrorTrait + 'static + Sync + Send>>
	{
		Ok(Scene(i32::from_sql(ty, raw)?))
	}
	fn accepts(ty: &Type) -> bool { <i32 as FromSql>::accepts(ty) }
}

impl ToSql for Scene
{
	fn to_sql(&self, ty: &Type, out: &mut Vec<u8>) -> Result<IsNull, Box<ErrorTrait + 'static + Sync + Send>>
	{
		self.0.to_sql(ty, out)
	}
	fn to_sql_checked(&self, ty: &Type, out: &mut Vec<u8>) ->
		Result<IsNull, Box<ErrorTrait + 'static + Sync + Send>>
	{
		self.0.to_sql_checked(ty, out)
	}
	fn accepts(ty: &Type) -> bool { <i32 as ToSql>::accepts(ty) }
}
