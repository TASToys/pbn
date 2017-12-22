use std::borrow::Cow;
use std::char::from_u32;
use std::fmt::{Display, Formatter, Error as FmtError};
use std::io::Read as IoRead;
use std::str::from_utf8;
use std::str::FromStr;

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
	NumericInteger(i64),	//Numbers that fit in 64-bit signed type.
	String(String),
	None,
}

fn is_ws_byte(x: u8) -> bool
{
	x == 9 || x== 10 || x == 13 || x == 32
}

const CLASSIFY_NUMERIC: u32 = 1;
const CLASSIFY_ZERO: u32 = 2;
const CLASSIFY_NONZERO: u32 = 4;
const CLASSIFY_MINUS: u32 = 8;
const CLASSIFY_MINUSPLUS: u32 = 16;
const CLASSIFY_DECIMAL: u32 = 32;
const CLASSIFY_EXPONENT: u32 = 64;

fn nclassify(x: char) -> u32
{
	match x as u32 {
		43 => CLASSIFY_MINUSPLUS,
		45 => CLASSIFY_MINUS | CLASSIFY_MINUSPLUS,
		46 => CLASSIFY_DECIMAL,
		48 => CLASSIFY_NUMERIC | CLASSIFY_ZERO,
		49...57 => CLASSIFY_NUMERIC | CLASSIFY_NONZERO,
		69 => CLASSIFY_EXPONENT,
		101 => CLASSIFY_EXPONENT,
		_ => 0,
	}
}

trait StringOrBorrow
{
	fn borrow<'a>(&'a self) -> &'a str;
	fn get(self) -> String;
}

impl StringOrBorrow for String
{
	fn borrow<'a>(&'a self) -> &'a str { &self }
	fn get(self) -> String { self }
}

impl<'b> StringOrBorrow for &'b str
{
	fn borrow<'a>(&'a self) -> &'a str { self }
	fn get(self) -> String { self.to_owned() }
}

enum StringOrInteger
{
	String(String),
	Integer(i64),
}

impl StringOrInteger
{
	fn from2<S:StringOrBorrow>(x: S) -> StringOrInteger
	{
		match i64::from_str(x.borrow()) {
			Ok(x) => StringOrInteger::Integer(x),
			Err(_) => StringOrInteger::String(x.get())
		}
	}
}

fn json_parse_numeric(x: &str, end: bool) -> Result<(usize, StringOrInteger), String>
{
	const STATE_INIT: u32 = 0;	//Initial.
	const STATE_FNUM: u32 = 1;	//First number.
	const STATE_CNUM: u32 = 2;	//Main number continuation
	const STATE_ZERO: u32 = 3;	//Main number is zero.
	const STATE_FDEC: u32 = 4;	//First in decimal part.
	const STATE_CDEC: u32 = 5;	//Continue decimal part.
	const STATE_EXPN: u32 = 6;	//After exponent separator.
	const STATE_EXPS: u32 = 7;	//After exponent numerical sign.
	const STATE_EXPC: u32 = 8;	//After exponent numerical continuation.
	const STATE_FINI: u32 = 99;	//Finished.
	const STATE_FAIL: u32 = 98;	//Failed.
	//Fastpath parse. The number can end in whitespace, comma, and close bracket and curly paren.
	if let Some(end) = x.find(|x|x == '\t' || x == '\r' || x == '\n' || x == ' ' || x == ',' || x == ']'
		|| x == '}') {
		let x = &x[..end];
		let mut state = STATE_INIT;
		for i in x.chars() {
			let c = nclassify(i);
			state = match state {
				STATE_INIT if c & CLASSIFY_MINUS != 0 => STATE_FNUM,
				STATE_INIT if c & CLASSIFY_ZERO != 0 => STATE_ZERO,
				STATE_INIT if c & CLASSIFY_NONZERO != 0 => STATE_CNUM,
				STATE_FNUM if c & CLASSIFY_ZERO != 0 => STATE_ZERO,
				STATE_FNUM if c & CLASSIFY_NONZERO != 0 => STATE_CNUM,
				STATE_CNUM if c & CLASSIFY_DECIMAL != 0 => STATE_FDEC,
				STATE_CNUM if c & CLASSIFY_EXPONENT != 0 => STATE_EXPN,
				STATE_CNUM if c & CLASSIFY_NUMERIC != 0 => STATE_CNUM,
				STATE_ZERO if c & CLASSIFY_DECIMAL != 0 => STATE_FDEC,
				STATE_ZERO if c & CLASSIFY_EXPONENT != 0 => STATE_EXPN,
				STATE_FDEC if c & CLASSIFY_NUMERIC != 0 => STATE_CDEC,
				STATE_CDEC if c & CLASSIFY_EXPONENT != 0 => STATE_EXPN,
				STATE_CDEC if c & CLASSIFY_NUMERIC != 0 => STATE_CDEC,
				STATE_EXPN if c & CLASSIFY_MINUSPLUS != 0 => STATE_EXPS,
				STATE_EXPN if c & CLASSIFY_NUMERIC != 0 => STATE_EXPC,
				STATE_EXPS if c & CLASSIFY_NUMERIC != 0 => STATE_EXPC,
				STATE_EXPC if c & CLASSIFY_NUMERIC != 0 => STATE_EXPC,
				_ => STATE_FAIL
			};
		}
		//Check suitable last state.
		match state {
			STATE_CNUM|STATE_ZERO|STATE_CDEC|STATE_EXPC => {
				return Ok((x.len(), StringOrInteger::from2(x)));
			}
			_ => (),	//Failed.
		}
	}
	//Fastpath failed, do slowpath.
	let mut y = String::new();
	let mut state = 0;
	for i in x.chars() {
		let _i = i as i32;
		state = match state {
			STATE_INIT if i == '-' => STATE_FNUM,
			STATE_INIT if i == '0' => STATE_ZERO,
			STATE_INIT if _i >= 49 && _i <= 57 => STATE_CNUM,
			STATE_INIT => return Err(format!("Got '{}' for sign or first number", i)),
			STATE_FNUM if i == '0' => STATE_ZERO,
			STATE_FNUM if _i >= 49 && _i <= 57 => STATE_CNUM,
			STATE_FNUM => return Err(format!("Got '{}' for first number", i)),
			STATE_CNUM if i == '.' => STATE_FDEC,
			STATE_CNUM if i == 'e' => STATE_EXPN,
			STATE_CNUM if i == 'E' => STATE_EXPN,
			STATE_CNUM if _i >= 48 && _i <= 57 => STATE_CNUM,
			STATE_CNUM => STATE_FINI,
			STATE_ZERO if i == '.' => STATE_FDEC,
			STATE_ZERO if i == 'e' => STATE_EXPN,
			STATE_ZERO if i == 'E' => STATE_EXPN,
			STATE_ZERO if _i >= 48 && _i <= 57 => return Err(format!("Got '{}' for after zero", i)),
			STATE_ZERO => STATE_FINI,
			STATE_FDEC if _i >= 48 && _i <= 57 => STATE_CDEC,
			STATE_FDEC => return Err(format!("Got '{}' for after decimal dot", i)),
			STATE_CDEC if _i >= 48 && _i <= 57 => STATE_CDEC,
			STATE_CDEC if i == 'e' => STATE_EXPN,
			STATE_CDEC if i == 'E' => STATE_EXPN,
			STATE_CDEC => STATE_FINI,
			STATE_EXPN if _i >= 48 && _i <= 57 => STATE_EXPC,
			STATE_EXPN if i == '+' => STATE_EXPS,
			STATE_EXPN if i == '-' => STATE_EXPS,
			STATE_EXPN => return Err(format!("Got '{}' for after exponent", i)),
			STATE_EXPS if _i >= 48 && _i <= 57 => STATE_EXPC,
			STATE_EXPS => return Err(format!("Got '{}' for after exponent sign", i)),
			//State 8: Number in exponent.
			STATE_EXPC if _i >= 48 && _i <= 57 => 8,
			STATE_EXPC => STATE_FINI,
			x => return Err(format!("Where am I (state={})???", x)),
		};
		if state != STATE_FINI {
			y.push(i);
		} else {
			return Ok((x.len(), StringOrInteger::from2(x)));
		}
	}
	match state {
		STATE_CNUM|STATE_ZERO|STATE_CDEC|STATE_EXPC if end => Ok((x.len(), StringOrInteger::from2(x))),
		_ if !end => Err(format!("Numeric token too long")),
		STATE_INIT => Err(format!("Got EoF expecting sign or first number")),
		STATE_FNUM => Err(format!("Got EoF expecting first number")),
		STATE_CNUM => Err(format!("Got EoF expecting numeric part")),
		STATE_ZERO => Err(format!("Got EoF expecting after zero")),
		STATE_FDEC => Err(format!("Got EoF expecting after decimal dot")),
		STATE_CDEC => Err(format!("Got EoF expecting decimal part")),
		STATE_EXPN => Err(format!("Got EoF expecting after exponent")),
		STATE_EXPS => Err(format!("Got EoF expecting after sign")),
		STATE_EXPC => Err(format!("Got EoF expecting after number in exponent")),
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
	//Fastpath parse. This succeeds if first character is doublequote, and out of doublequote and backslash,
	//doublequote comes first.
	if x.len() > 2 && x.as_bytes()[0] == 34 {
		let x = &x[1..];
		if let Some(end) = x.find(|x|x == '\"' || x == '\\') {
			if x.as_bytes()[end] == 34 {
				//Fastpath succeeded.
				return Ok((end+2, (&x[..end]).to_owned()));
			}
		}
	}
	//Fastpath failed, use slowpath.
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
					//This should produce at least 10000 and at most 10FFFF.
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
/*
const ETAB: [u64;18] = [
	1000000000000000000,
	100000000000000000,
	10000000000000000,
	1000000000000000,
	100000000000000,
	10000000000000,
	1000000000000,
	100000000000,
	10000000000,
	1000000000,
	100000000,
	10000000,
	1000000,
	100000,
	10000,
	1000,
	100,
	10,
];
*/
impl JsonToken
{
/*
	pub fn write<W:Write>(&self, output: &mut W) -> Result<(), IoError>
	{
		match self {
			&JsonToken::StartArray => output.write_all(b"["),
			&JsonToken::EndArray => output.write_all(b"]"),
			&JsonToken::StartObject => output.write_all(b"{"),
			&JsonToken::EndObject => output.write_all(b"}"),
			&JsonToken::DoubleColon => output.write_all(b":"),
			&JsonToken::Comma => output.write_all(b","),
			&JsonToken::Boolean(false) => output.write_all(b"false"),
			&JsonToken::Boolean(true) => output.write_all(b"true"),
			&JsonToken::Null => output.write_all(b"null"),
			&JsonToken::Numeric(ref x) => output.write_all(x.as_bytes()),
			&JsonToken::NumericInteger(x) => {
				//Numeric integer always fits in 20 bytes.
				let mut buf = [0;20];
				let mut idx = 0;
				if x < 0 { buf[idx] = b'-'; idx += 1; }
				let mut x = if x < 0 { -x as u64 } else { x as u64 };
				for i in 0..18 {
					if x > ETAB[i] {
						//Calculating quotent and remainder together is fast on amd64.
						let q = x / ETAB[i];
						x = x % ETAB[i];
						buf[idx] = 48 + q as u8;
						idx += 1;
					}
				}
				buf[idx] = 48 + x as u8;
				idx += 1;
				output.write_all(&buf[..idx]),
			}
			&JsonToken::String(ref x) => {
				output.write_all(b"\"")?,
				output.write_all(escape_json_string(x).deref().as_bytes())?,
				output.write_all(b"\""),
			},
			&JsonToken::None => Ok(())
		}
	}
*/
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
					Ok((nlen, StringOrInteger::String(n))) => Ok((idx + nlen,
						JsonToken::Numeric(n))),
					Ok((nlen, StringOrInteger::Integer(n))) => Ok((idx + nlen,
						JsonToken::NumericInteger(n))),
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
	start: usize,
	eof: bool,
}

impl<R:IoRead> JsonStream<R>
{
	pub fn new(reader: R) -> JsonStream<R>
	{
		JsonStream{reader: reader, buffer: String::new(), utf8_overflow: 0, start: 0, eof: false}
	}
	fn refill(&mut self) -> Result<(), BasicJsonError>
	{
		while !self.eof && self.buffer.len() - self.start < 4096 {
			if self.start > 0 {
				self.buffer = (&self.buffer[self.start..]).to_owned();
				self.start = 0;
			}
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
		let (n, t) = JsonToken::next(&self.buffer[self.start..], self.eof).map_err(|x|error(
			BasicJsonError::BadJsonToken(x)))?;
		self.start += n;
		Ok(t)
	}
	pub fn peek(&mut self) -> Result<JsonToken, BasicJsonError>
	{
		self.refill()?;
		JsonToken::next(&self.buffer[self.start..], self.eof).map(|(_,x)|x).map_err(|x|
			BasicJsonError::BadJsonToken(x))
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

