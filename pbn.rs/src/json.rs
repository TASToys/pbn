use std::borrow::Cow;
use std::char::from_u32;
use std::io::Read as IoRead;
use std::str::from_utf8;

#[derive(PartialEq,Eq,Debug)]
pub enum JsonToken
{
	StartArray,		//[
	EndArray,		//]
	StartObject,		//{
	EndObject,		//}
	DoubleColon,		//:
	Comma,			//,
	Boolean(bool),
	Null,
	Numeric(String),
	String(String),
	None,
}

fn is_ws_byte(x: u8) -> bool
{
	x == 9 || x== 10 || x == 13 || x == 32
}

fn json_parse_numeric(x: &str, end: bool) -> Option<(usize, String)>
{
	let mut y = String::new();
	let mut state = 0;
	for i in x.chars() {
		let _i = i as i32;
		state = match state {
			//State 0: Perhaps minus, or the first number.
			0 if i == '-' => 1,
			0 if i == '0' => 3,
			0 if _i >= 49 && _i <= 57 => 2,
			0 => return None,
			//State 1: First number.
			1 if i == '0' => 3,
			1 if _i >= 49 && _i <= 57 => 2,
			1 => return None,
			//State 2: Numeric part.
			2 if i == '.' => 4,
			2 if i == 'e' => 6,
			2 if i == 'E' => 6,
			2 if _i >= 48 && _i <= 57 => 2,
			2 => 99,
			//State 3: After zero.
			3 if i == '.' => 4,
			3 if i == 'e' => 6,
			3 if i == 'E' => 6,
			3 if _i >= 48 && _i <= 57 => return None,
			3 => 99,
			//State 4: Decimal part.
			4 if _i >= 48 && _i <= 57 => 5,
			4 if i == 'e' => 6,
			4 if i == 'E' => 6,
			4 => return None,
			//State 5: Decimal part, at least one digit.
			5 if _i >= 48 && _i <= 57 => 5,
			5 if i == 'e' => 6,
			5 if i == 'E' => 6,
			5 => 99,
			//State 6: After exponent sign
			6 if _i >= 48 && _i <= 57 => 8,
			6 if i == '+' => 7,
			6 if i == '-' => 7,
			6 => return None,
			//State 7: After exponent sign and numeric sign
			7 if _i >= 48 && _i <= 57 => 8,
			7 => return None,
			//State 8: Number in exponent.
			8 if _i >= 48 && _i <= 57 => 8,
			8 => 99,
			_ => return None
		};
		if state != 99 {
			y.push(i);
		} else {
			return Some((y.len(), y));
		}
	}
	match state {
		2|3|5|8 if end => return Some((y.len(), y)),
		_ => return None,
	}
}

fn hexparse(i: char) -> u32
{
	let i = i as u32;
	match i {
		48...57 => i - 48,
		65...70 => i - 55,
		97...102 => i - 87,
		_ => 0xFFFFFFFF,
	}
}

fn json_parse_string(x: &str) -> Option<(usize, String)>
{
	let mut y = String::new();
	let mut state = 0;
	let mut unicode: u32 = 0;
	let mut pending: u32 = 0;
	for (p, i) in x.chars().enumerate() {
		state = match state {
			//State 0: The first character is aways doublequote.
			0 if i == '\"' => 1,
			0 => return None,
			//State 1: Normal unescaped character.
			1 if i == '\\' => 2,
			1 if i == '\"' => return Some((p + 1, y)),
			1 => {y.push(i); 1},
			//State 2: Backslash escape.
			2 if i == '\"' || i == '\\' || i == '/' => {y.push(i); 1},
			2 if i == 'b' => {y.push(from_u32(8).unwrap()); 1},
			2 if i == 't' => {y.push(from_u32(9).unwrap()); 1},
			2 if i == 'n' => {y.push(from_u32(10).unwrap()); 1},
			2 if i == 'f' => {y.push(from_u32(12).unwrap()); 1},
			2 if i == 'r' => {y.push(from_u32(13).unwrap()); 1},
			2 if i == 'u' => {unicode = 0; 3},
			2 => return None,
			//State 3-6: Unicode escape.
			3 => {unicode = (unicode << 4) | hexparse(i); 4},
			4 => {unicode = (unicode << 4) | hexparse(i); 5},
			5 => {unicode = (unicode << 4) | hexparse(i); 6},
			6 => {
				unicode = (unicode << 4) | hexparse(i);
				if unicode > 0xFFFF {
					return None;
				} else if unicode < 0xD800 || unicode > 0xDFFF {
					y.push(from_u32(unicode).unwrap());
					1
				} else if unicode < 0xDC00 {
					pending = unicode - 0xD800;
					unicode = 0;
					7
				} else {
					return None;
				}
			},
			//State 7-12: Unicode escape after surrogate.
			7 if i == '\\' => 8,
			7 => return None,
			8 if i == 'u' => 9,
			8 => return None,
			9 => {unicode = (unicode << 4) | hexparse(i); 10},
			10 => {unicode = (unicode << 4) | hexparse(i); 11},
			11 => {unicode = (unicode << 4) | hexparse(i); 12},
			12 => {
				unicode = (unicode << 4) | hexparse(i);
				if unicode >= 0xDC00 && unicode <= 0xDFFF {
					unicode = 0x10000 + pending * 1024 + (unicode - 0xDC00);
					y.push(from_u32(unicode).unwrap());
					1
				} else {
					return None;
				}
			},
			_ => return None
		};
	}
	None
}

impl JsonToken
{
	fn next(partial: &str, end: bool) -> Option<(usize, JsonToken)>
	{
		let mut idx = 0;
		//Skip any whitespace.
		while idx < partial.len() && is_ws_byte(partial.as_bytes()[idx]) { idx += 1; }
		if idx == partial.len() { return if end { Some((idx, JsonToken::None)) } else { None }; }
		match partial.as_bytes()[idx] {
			b'[' => return Some((idx + 1, JsonToken::StartArray)),
			b']' => return Some((idx + 1, JsonToken::EndArray)),
			b'{' => return Some((idx + 1, JsonToken::StartObject)),
			b'}' => return Some((idx + 1, JsonToken::EndObject)),
			b':' => return Some((idx + 1, JsonToken::DoubleColon)),
			b',' => return Some((idx + 1, JsonToken::Comma)),
			b't' => {
				if partial[idx..].starts_with("true") {
					return Some((idx + 4, JsonToken::Boolean(true)))
				} else {
					return None;
				}
			},
			b'f' => {
				if partial[idx..].starts_with("false") {
					return Some((idx + 5, JsonToken::Boolean(false)))
				} else {
					return None;
				}
			},
			b'n' => {
				if partial[idx..].starts_with("null") {
					return Some((idx + 4, JsonToken::Null))
				} else {
					return None;
				}
			},
			48...57|45 => {
				//Numeric.
				if let Some((nlen, n)) = json_parse_numeric(&partial[idx..], end) {
					return Some((idx + nlen, JsonToken::Numeric(n)))
				} else {
					return None;
				}
			},
			34 => {
				//String.
				if let Some((slen, n)) = json_parse_string(&partial[idx..]) {
					return Some((idx + slen, JsonToken::String(n)))
				} else {
					return None;
				}
			},
			_ => return None
		}
	}
}

pub struct JsonStream<R:IoRead>
{
	reader: R,
	buffer: String,
	eof: bool,
}

impl<R:IoRead> JsonStream<R>
{
	pub fn new(reader: R) -> JsonStream<R>
	{
		JsonStream{reader: reader, buffer: String::new(), eof: false}
	}
	fn refill(&mut self) -> Result<(), ()>
	{
		while !self.eof && self.buffer.len() < 4096 {
			let mut buf = [0;8192];
			let n = self.reader.read(&mut buf).map_err(|_|())?;
			if n == 0 { self.eof = true; break; }
			self.buffer.push_str(from_utf8(&buf[..n]).map_err(|_|())?);
		}
		Ok(())
	}
	pub fn next<E:Clone>(&mut self, error: E) -> Result<JsonToken, E>
	{
		self.refill().map_err(|_|error.clone())?;
		if let Some((n, t)) = JsonToken::next(&self.buffer, self.eof) {
			self.buffer = (&self.buffer[n..]).to_owned();
			Ok(t)
		} else {
			Err(error)
		}
	}
	pub fn peek(&mut self) -> Result<JsonToken, ()>
	{
		self.refill()?;
		JsonToken::next(&self.buffer, self.eof).map(|(_,x)|x).ok_or(())
	}
	pub fn do_array<F,E:Clone>(&mut self, mut cb: F, readerr: E) -> Result<(), E> where F: FnMut(
		&mut JsonStream<R>) -> Result<(), E>
	{
		//Empty array case.
		if self.peek().map_err(|_|readerr.clone())? == JsonToken::EndArray { return Ok(()); }
		
		loop {
			cb(self)?;
			let ntoken = self.next(readerr.clone())?;
			if ntoken == JsonToken::Comma {
				//Skip,
			} else if ntoken == JsonToken::EndArray {
				//End of array.
				break;
			} else {
				return Err(readerr.clone())
			}
		}
		Ok(())
	}
	pub fn do_object<F,E:Clone>(&mut self, mut cb: F, readerr: E) -> Result<(), E> where F: FnMut(
		&mut JsonStream<R>, String) -> Result<(), E>
	{
		//Empty array case.
		if self.peek().map_err(|_|readerr.clone())? == JsonToken::EndObject { return Ok(()); }
		
		loop {
			let keyname = if let JsonToken::String(key) = self.next(readerr.clone())? { key } else {
				return Err(readerr.clone()); };
			if self.next(readerr.clone())? != JsonToken::DoubleColon { return Err(readerr.
				clone()); }
			cb(self, keyname)?;
			let ntoken = self.next(readerr.clone())?;
			if ntoken == JsonToken::Comma {
				//Skip,
			} else if ntoken == JsonToken::EndObject {
				//End of array.
				break;
			} else {
				return Err(readerr.clone())
			}
		}
		Ok(())
	}
}

pub fn escape_json_string<'a>(x: &'a str) -> Cow<'a, str>
{
	if x.find(|c|{let c = c as u32; c < 32 || c == 34 || c == 92  /*controls, doublequote or backslash.*/}).
		is_none() { return Cow::Borrowed(x); }
	//Needs escaping.
	let mut out = String::new();
	for c in x.chars() {
		let _c = c as u32;
		match _c {
			0...31 => out.push_str(&format!("\\u{:04x}", _c)),
			34 => out.push_str("\\\""),
			92 => out.push_str("\\\\"),
			_ => out.push(c)
		};
	}
	Cow::Owned(out)
}

