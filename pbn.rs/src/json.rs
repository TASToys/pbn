use std::borrow::Cow;
use std::char::from_u32;
use std::fmt::{Display, Formatter, Error as FmtError};
use std::io::Read as IoRead;
use std::str::from_utf8;

#[derive(Clone,Debug)]
pub enum JsonTokenError
{
	//Bad token initial character.
	BadTokenInitial(char),
	//Bad number format
	BadNumberFormat(String),
	//Bad string format
	BadStringFormat(String),
	//Token too long.
	TokenTooLong,
	//Expected token null.
	ExpectedNull,
	//Expected token false.
	ExpectedFalse,
	//Expected token true.
	ExpectedTrue,
}

impl Display for JsonTokenError
{
	fn fmt(&self, fmt: &mut Formatter) -> Result<(), FmtError>
	{
		match self.clone() {
			JsonTokenError::BadTokenInitial(x) => fmt.write_fmt(format_args!("Bad token initial \
				character '{}', expected one of '[]{{}}:,tfn-0123456789\"'", x)),
			JsonTokenError::BadNumberFormat(x) => fmt.write_fmt(format_args!("Bad number format: {}",
				x)),
			JsonTokenError::BadStringFormat(x) => fmt.write_fmt(format_args!("Bad string format: {}",
				x)),
			JsonTokenError::TokenTooLong => fmt.write_str("Token too long"),
			JsonTokenError::ExpectedNull => fmt.write_str("Token starting with 'n' is not 'null'"),
			JsonTokenError::ExpectedFalse => fmt.write_str("Token starting with 'f' is not 'false'"),
			JsonTokenError::ExpectedTrue => fmt.write_str("Token starting with 't' is not 'true'"),
		}
	}
}


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

fn json_parse_numeric(x: &str, end: bool) -> Result<(usize, String), String>
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
			0 => return Err(format!("Got '{}' for sign or first number", i)),
			//State 1: First number.
			1 if i == '0' => 3,
			1 if _i >= 49 && _i <= 57 => 2,
			1 => return Err(format!("Got '{}' for first number", i)),
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
			3 if _i >= 48 && _i <= 57 => return Err(format!("Got '{}' for after zero", i)),
			3 => 99,
			//State 4: Decimal part.
			4 if _i >= 48 && _i <= 57 => 5,
			4 => return Err(format!("Got '{}' for after decimal dot", i)),
			//State 5: Decimal part, at least one digit.
			5 if _i >= 48 && _i <= 57 => 5,
			5 if i == 'e' => 6,
			5 if i == 'E' => 6,
			5 => 99,
			//State 6: After exponent sign
			6 if _i >= 48 && _i <= 57 => 8,
			6 if i == '+' => 7,
			6 if i == '-' => 7,
			6 => return Err(format!("Got '{}' for after exponent", i)),
			//State 7: After exponent sign and numeric sign
			7 if _i >= 48 && _i <= 57 => 8,
			7 => return Err(format!("Got '{}' for after exponent sign", i)),
			//State 8: Number in exponent.
			8 if _i >= 48 && _i <= 57 => 8,
			8 => 99,
			x => return Err(format!("Where am I (state={})???", x)),
		};
		if state != 99 {
			y.push(i);
		} else {
			return Ok((y.len(), y));
		}
	}
	match state {
		2|3|5|8 if end => Ok((y.len(), y)),
		_ if !end => Err(format!("Numeric token too long")),
		0 => Err(format!("Got EoF expecting sign or first number")),
		1 => Err(format!("Got EoF expecting first number")),
		2 => Err(format!("Got EoF expecting numeric part")),
		3 => Err(format!("Got EoF expecting after zero")),
		4 => Err(format!("Got EoF expecting after decimal dot")),
		5 => Err(format!("Got EoF expecting decimal part")),
		6 => Err(format!("Got EoF expecting after exponent")),
		7 => Err(format!("Got EoF expecting after sign")),
		8 => Err(format!("Got EoF expecting after number in exponent")),
		x => Err(format!("Where am I (state={})???", x))
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

fn json_parse_string(x: &str, end: bool) -> Result<(usize, String), String>
{
	let mut y = String::new();
	let mut state = 0;
	let mut unicode: u32 = 0;
	let mut pending: u32 = 0;
	for (p, i) in x.chars().enumerate() {
		state = match state {
			//State 0: The first character is aways doublequote.
			0 if i == '\"' => 1,
			0 => return Err(format!("Get '{}' for initial double quote", i)),
			//State 1: Normal unescaped character.
			1 if i == '\\' => 2,
			1 if i == '\"' => return Ok((p + 1, y)),
			1 => {y.push(i); 1},
			//State 2: Backslash escape.
			2 if i == '\"' || i == '\\' || i == '/' => {y.push(i); 1},
			2 if i == 'b' => {y.push(from_u32(8).unwrap()); 1},
			2 if i == 't' => {y.push(from_u32(9).unwrap()); 1},
			2 if i == 'n' => {y.push(from_u32(10).unwrap()); 1},
			2 if i == 'f' => {y.push(from_u32(12).unwrap()); 1},
			2 if i == 'r' => {y.push(from_u32(13).unwrap()); 1},
			2 if i == 'u' => {unicode = 0; 3},
			2 => return Err(format!("Get '{}' for escaped character, expected one of '\"\\/bfnrtu'", i)),
			//State 3-6: Unicode escape.
			3 => {unicode = (unicode << 4) | hexparse(i); 4},
			4 => {unicode = (unicode << 4) | hexparse(i); 5},
			5 => {unicode = (unicode << 4) | hexparse(i); 6},
			6 => {
				unicode = (unicode << 4) | hexparse(i);
				if unicode > 0xFFFF {
					return Err(format!("Bad hex characters in unicode escape"));
				} else if unicode < 0xD800 || unicode > 0xDFFF {
					//This should never fail.
					y.push(from_u32(unicode).unwrap());
					1
				} else if unicode < 0xDC00 {
					pending = unicode - 0xD800;
					unicode = 0;
					7
				} else {
					return Err(format!("Unpaired low surrogate {:04x} in unicode escape",
						unicode));
				}
			},
			//State 7-12: Unicode escape after surrogate.
			7 if i == '\\' => 8,
			7 => return Err(format!("Bad unicode escape after high surrogate, expected '\\', got {}",
				i)),
			8 if i == 'u' => 9,
			8 => return Err(format!("Bad unicode escape after high surrogate, expected 'u', got {}", i)),
			9 => {unicode = (unicode << 4) | hexparse(i); 10},
			10 => {unicode = (unicode << 4) | hexparse(i); 11},
			11 => {unicode = (unicode << 4) | hexparse(i); 12},
			12 => {
				unicode = (unicode << 4) | hexparse(i);
				if unicode > 0xFFFF {
					return Err(format!("Bad hex characters in unicode escape after high \
						surrogate"));
				} else if unicode >= 0xDC00 && unicode <= 0xDFFF {
					unicode = 0x10000 + pending * 1024 + (unicode - 0xDC00);
					//This should produce at most 10FFFF.
					y.push(from_u32(unicode).unwrap());
					1
				} else {
					return Err(format!("High surrogate followed by not low surrogate {:04x} in \
						unicode escape", unicode));
				}
			},
			x => return Err(format!("Where am I (state={})???", x)),
		};
	}
	Err(if end { format!("EoF while reading string") } else { format!("String token too long") })
}

impl JsonToken
{
	fn next(partial: &str, end: bool) -> Result<(usize, JsonToken), JsonTokenError>
	{
		let mut idx = 0;
		//Skip any whitespace.
		while idx < partial.len() && is_ws_byte(partial.as_bytes()[idx]) { idx += 1; }
		if idx == partial.len() { return if end { Ok((idx, JsonToken::None)) } else { Err(
			JsonTokenError::TokenTooLong) }; }
		match partial.as_bytes()[idx] {
			b'[' => return Ok((idx + 1, JsonToken::StartArray)),
			b']' => return Ok((idx + 1, JsonToken::EndArray)),
			b'{' => return Ok((idx + 1, JsonToken::StartObject)),
			b'}' => return Ok((idx + 1, JsonToken::EndObject)),
			b':' => return Ok((idx + 1, JsonToken::DoubleColon)),
			b',' => return Ok((idx + 1, JsonToken::Comma)),
			b't' => {
				if partial[idx..].starts_with("true") {
					Ok((idx + 4, JsonToken::Boolean(true)))
				} else {
					Err(JsonTokenError::ExpectedTrue)
				}
			},
			b'f' => {
				if partial[idx..].starts_with("false") {
					Ok((idx + 5, JsonToken::Boolean(false)))
				} else {
					Err(JsonTokenError::ExpectedFalse)
				}
			},
			b'n' => {
				if partial[idx..].starts_with("null") {
					Ok((idx + 4, JsonToken::Null))
				} else {
					Err(JsonTokenError::ExpectedNull)
				}
			},
			48...57|45 => {
				//Numeric.
				match json_parse_numeric(&partial[idx..], end) {
					Ok((nlen, n)) => Ok((idx + nlen, JsonToken::Numeric(n))),
					Err(x) => Err(JsonTokenError::BadNumberFormat(x))
				}
			},
			34 => {
				//String.
				match json_parse_string(&partial[idx..], end) {
					Ok((slen, n)) => Ok((idx + slen, JsonToken::String(n))),
					Err(x) => Err(JsonTokenError::BadStringFormat(x))
				}
			},
			//The smallest number from_u32 fails on is 0xD800, and u8 can't store that, so the from_u32
			//always succeeds.
			x => Err(JsonTokenError::BadTokenInitial(from_u32(x as u32).unwrap()))
		}
	}
}

#[derive(Clone,Debug)]
pub enum BasicJsonError
{
	//I/O error from system.
	IoError(String),
	//Bad UTF-8.
	BadUtf8,
	//Bad JSON token.
	BadJsonToken(JsonTokenError),
	//Expected comma or end of array.
	ExpectedCommaOrEndOfArray(String),
	//Expected string object key.
	ExpectedStringObjectKey(String),
	//Expected double colon.
	ExpectedDoubleColon(String),
	//Expected comma or end of object.
	ExpectedCommaOrEndOfObject(String),
	//Expected start of object.
	ExpectedStartOfObject(String),
	//Expected start of array.
	ExpectedStartOfArray(String),
	//Expected string.
	ExpectedString(String),
	//Expected end of object.
	ExpectedEndOfObject(String),
	//Expected end of JSON.
	ExpectedEnd(String),
}

impl Display for BasicJsonError
{
	fn fmt(&self, fmt: &mut Formatter) -> Result<(), FmtError>
	{
		match self.clone() {
			BasicJsonError::IoError(x) => fmt.write_fmt(format_args!("I/O Error: {}", x)),
			BasicJsonError::BadUtf8 => fmt.write_str("Bad UTF-8"),
			BasicJsonError::BadJsonToken(x) => fmt.write_fmt(format_args!("Bad JSON token: {}", x)),
			BasicJsonError::ExpectedCommaOrEndOfArray(x) => fmt.write_fmt(format_args!("Expected comma \
				or end of array, got {}", x)),
			BasicJsonError::ExpectedStringObjectKey(x) => fmt.write_fmt(format_args!("Expected string \
				as object key, got {}", x)),
			BasicJsonError::ExpectedDoubleColon(x) => fmt.write_fmt(format_args!("Expected double \
				colon, got {}", x)),
			BasicJsonError::ExpectedCommaOrEndOfObject(x) => fmt.write_fmt(format_args!("Expected \
				comma or end of object, got {}", x)),
			BasicJsonError::ExpectedStartOfArray(x) => fmt.write_fmt(format_args!("Expected start of \
				array, got {}", x)),
			BasicJsonError::ExpectedStartOfObject(x) => fmt.write_fmt(format_args!("Expected start of \
				object, got {}", x)),
			BasicJsonError::ExpectedString(x) => fmt.write_fmt(format_args!("Expected string, got {}",
				x)),
			BasicJsonError::ExpectedEndOfObject(x) => fmt.write_fmt(format_args!("Expected end of \
				object, got {}", x)),
			BasicJsonError::ExpectedEnd(x) => fmt.write_fmt(format_args!("Expected end of JSON, got {}",
				x)),
		}
	}
}

pub struct JsonStream<R:IoRead>
{
	reader: R,
	buffer: String,
	utf8_overflow: u32,
	eof: bool,
}

impl<R:IoRead> JsonStream<R>
{
	pub fn new(reader: R) -> JsonStream<R>
	{
		JsonStream{reader: reader, buffer: String::new(), utf8_overflow: 0, eof: false}
	}
	fn refill(&mut self) -> Result<(), BasicJsonError>
	{
		while !self.eof && self.buffer.len() < 4096 {
			let mut buf = [0;8192];
			let n = self.reader.read(&mut buf).map_err(|x|BasicJsonError::IoError(format!("{}", x)))?;
			if n == 0 {
				//Stream must not end with incomplete UTF8 character.
				if self.utf8_overflow != 0 { return Err(BasicJsonError::BadUtf8); }
				self.eof = true;
				break;
			}
			//Be careful: There may be pending partial UTF-8 charcter.
			let mut ipos = 0;
			while ipos < n && self.utf8_overflow > 0 {
				self.utf8_overflow = match self.utf8_overflow {
					//Codes 0xC0...0xF7, 0xE000...0xF7FF and 0xF00000...0xF7FFFF are missing
					//bytes.
					x@0xC0...0xF7 => 256 * x + (buf[ipos] as u32),
					x@0xE000...0xF7FF => 256 * x + (buf[ipos] as u32),
					x@0xF00000...0xF7FFFF => 256 * x + (buf[ipos] as u32),
					//Codes 0xC000...0xDFFF, 0xE00000...0xEFFFFF and 0xF0000000...0xF7FFFFFF
					//are decodeable.
					x@0xC000...0xDFFF => {
						self.buffer.push_str(from_utf8(&[(x >> 8) as u8, x as u8]).map_err(
							|_|BasicJsonError::BadUtf8)?);
						0
					},
					x@0xE00000...0xEFFFFF => {
						self.buffer.push_str(from_utf8(&[(x >> 16) as u8, (x >> 8) as u8,
							x as u8]).map_err(|_|BasicJsonError::BadUtf8)?);
						0
					},
					x@0xF0000000...0xF7FFFFFF => {
						self.buffer.push_str(from_utf8(&[(x >> 24) as u8, (x >> 16) as u8,
							(x >> 8) as u8, x as u8]).map_err(|_|
							BasicJsonError::BadUtf8)?);
						0
					},
					//All others are bad (utf8_overflow=0 can not happen).
					_ => return Err(BasicJsonError::BadUtf8)
				};
				ipos += 1;
			}
			if ipos == n { continue; }	//Processed all.
			let valid_up_to = match from_utf8(&buf[ipos..n]).map_err(|x|x.valid_up_to()) {
				Ok(x) => {
					self.buffer.push_str(x);
					n
				},
				Err(n) => {
					//It was valid up to this point.
					self.buffer.push_str(from_utf8(&buf[..n]).unwrap());
					n
				}
			};
			//There can be at most 3 pending bytes, since 4 should have combined into a character.
			//Process the trailing incomplete UTF-8 part.
			if n - valid_up_to > 3 { return Err(BasicJsonError::BadUtf8); }
			for i in valid_up_to..n {
				self.utf8_overflow = 256 * self.utf8_overflow + (buf[i] as u32);
			}
		}
		Ok(())
	}
	pub fn next<E:Clone,ErrFn>(&mut self, error: &ErrFn) -> Result<JsonToken, E> where ErrFn: Fn(BasicJsonError)
		-> E
	{
		self.refill().map_err(error)?;
		let (n, t) = JsonToken::next(&self.buffer, self.eof).map_err(|x|error(
			BasicJsonError::BadJsonToken(x)))?;
		self.buffer = (&self.buffer[n..]).to_owned();
		Ok(t)
	}
	pub fn peek(&mut self) -> Result<JsonToken, BasicJsonError>
	{
		self.refill()?;
		JsonToken::next(&self.buffer, self.eof).map(|(_,x)|x).map_err(|x|BasicJsonError::BadJsonToken(x))
	}
	pub fn do_array<F,E:Clone,ErrFn>(&mut self, mut cb: F, error: &ErrFn) -> Result<(), E> where F: FnMut(
		&mut JsonStream<R>) -> Result<(), E>, ErrFn: Fn(BasicJsonError) -> E
	{
		//Empty array case.
		if self.peek().map_err(error)? == JsonToken::EndArray { return Ok(()); }
		
		loop {
			cb(self)?;
			let ntoken = self.next(error)?;
			if ntoken == JsonToken::Comma {
				//Skip,
			} else if ntoken == JsonToken::EndArray {
				//End of array.
				break;
			} else {
				return Err(error(BasicJsonError::ExpectedCommaOrEndOfArray(format!("{:?}",
					ntoken))));
			}
		}
		Ok(())
	}
	pub fn do_object<F,E:Clone,ErrFn>(&mut self, mut cb: F, error: &ErrFn) -> Result<(), E> where F: FnMut(
		&mut JsonStream<R>, String) -> Result<(), E>, ErrFn: Fn(BasicJsonError) -> E
	{
		use self::BasicJsonError::*;
		//Empty array case.
		if self.peek().map_err(error.clone())? == JsonToken::EndObject { return Ok(()); }
		
		loop {
			let keyname = match self.next(error)? {
				JsonToken::String(key) => key,
				x => return Err(error(ExpectedStringObjectKey(format!("{:?}", x))))
			};
			match self.next(error)? {
				JsonToken::DoubleColon => (),
				x => return Err(error(ExpectedDoubleColon(format!("{:?}", x))))
			};
			cb(self, keyname)?;
			match self.next(error)? {
				JsonToken::Comma => (),		//Skip.
				JsonToken::EndObject => break,	//End of array.
				x => return Err(error(ExpectedCommaOrEndOfObject(format!("{:?}", x))))
			};
		}
		Ok(())
	}
	pub fn expect_object(&mut self) -> Result<(), BasicJsonError>
	{
		match self.peek()? {
			JsonToken::StartObject => self.next(&|x|x).map(|_|()),
			x => Err(BasicJsonError::ExpectedStartOfObject(format!("{:?}", x)))
		}
	}
	pub fn expect_object_end(&mut self) -> Result<(), BasicJsonError>
	{
		match self.peek()? {
			JsonToken::EndObject => self.next(&|x|x).map(|_|()),
			x => Err(BasicJsonError::ExpectedEndOfObject(format!("{:?}", x)))
		}
	}
	pub fn expect_array(&mut self) -> Result<(), BasicJsonError>
	{
		match self.peek()? {
			JsonToken::StartArray => self.next(&|x|x).map(|_|()),
			x => Err(BasicJsonError::ExpectedStartOfArray(format!("{:?}", x)))
		}
	}
	pub fn expect_string(&mut self) -> Result<String, BasicJsonError>
	{
		match self.peek()? {
			JsonToken::String(x) => { self.next(&|x|x)?; Ok(x)},
			x => Err(BasicJsonError::ExpectedString(format!("{:?}", x)))
		}
	}
	pub fn expect_doublecolon(&mut self) -> Result<(), BasicJsonError>
	{
		match self.peek()? {
			JsonToken::DoubleColon => self.next(&|x|x).map(|_|()),
			x => Err(BasicJsonError::ExpectedDoubleColon(format!("{:?}", x)))
		}
	}
	pub fn expect_end_of_json(&mut self) -> Result<(), BasicJsonError>
	{
		match self.peek()? {
			JsonToken::None => self.next(&|x|x).map(|_|()),
			x => Err(BasicJsonError::ExpectedEnd(format!("{:?}", x)))
		}
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

