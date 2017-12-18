#![feature(plugin, decl_macro, custom_derive)]
#![plugin(rocket_codegen)]

extern crate rocket;
extern crate postgres;
extern crate md5;
extern crate rand;
use postgres::{Connection, TlsMode};
use rocket::request::{FromRequest, FromForm, Form, FormItems, Request};
use rocket::outcome::Outcome;
use rocket::response::{Responder, Response};
use rocket::http::{Header, Status};
use rocket::Data;
use std::borrow::Cow;
use std::fmt::Write as FmtWrite;
use std::io::Cursor;
use std::str::{FromStr, from_utf8};
use std::time::{SystemTime, UNIX_EPOCH};
use rand::os::OsRng;
use std::io::Read as IoRead;
use std::char::from_u32;
use rand::Rng;

#[derive(Debug)]
enum Error
{
	SceneNotFound,
	InvalidOrigin,
	InvalidDimensions,
	BadFormField(String),
	BadGrant,
	BadEventStream,
}

impl<'r> Responder<'r> for Error
{
	fn respond_to(self, _request: &Request) -> Result<Response<'r>, Status>
	{
		match self {
			Error::SceneNotFound => {
				let mut response = Response::new();
				let body = "Scene not found\n".to_owned().into_bytes();
				response.set_status(Status::new(404, "Not found"));
				response.set_header(Header::new("Content-Type", "text/plain"));
				response.set_sized_body(Cursor::new(body));
				Ok(response)
			},
			Error::InvalidOrigin => {
				let mut response = Response::new();
				let body = "Invalid origin\n".to_owned().into_bytes();
				response.set_status(Status::new(403, "Forbidden"));
				response.set_header(Header::new("Content-Type", "text/plain"));
				response.set_sized_body(Cursor::new(body));
				Ok(response)
			},
			Error::InvalidDimensions => {
				let mut response = Response::new();
				let body = "Invalid dimensions\n".to_owned().into_bytes();
				response.set_status(Status::new(418, "Invalid dimensions"));
				response.set_header(Header::new("Content-Type", "text/plain"));
				response.set_sized_body(Cursor::new(body));
				Ok(response)
			},
			Error::BadFormField(f) => {
				let mut response = Response::new();
				let body = format!("Bad form field {}\n", f).into_bytes();
				response.set_status(Status::new(418, "Bad form field"));
				response.set_header(Header::new("Content-Type", "text/plain"));
				response.set_sized_body(Cursor::new(body));
				Ok(response)
			},
			Error::BadGrant => {
				let mut response = Response::new();
				let body = "Bad grant\n".to_owned().into_bytes();
				response.set_status(Status::new(418, "Bad grant"));
				response.set_header(Header::new("Content-Type", "text/plain"));
				response.set_sized_body(Cursor::new(body));
				Ok(response)
			},
			Error::BadEventStream => {
				let mut response = Response::new();
				let body = "Bad event stream\n".to_owned().into_bytes();
				response.set_status(Status::new(418, "Bad event stream"));
				response.set_header(Header::new("Content-Type", "text/plain"));
				response.set_sized_body(Cursor::new(body));
				Ok(response)
			},
		}
	}
}

fn db_connect() -> Connection
{
	Connection::connect("pq://pbn@%2fvar%2frun%2fpostgresql%2f/pbndb", TlsMode::None).unwrap()
}

//use std::thread::sleep;
//use std::time::Duration;
//use std::str::from_utf8;

struct AuthenticationInfo
{
	origin: Option<String>,
	overridden: bool,
	key: Option<String>,
}

impl AuthenticationInfo
{
	fn get_origin(&self, conn: &mut Connection, privileged: bool) -> Result<i32, ()>
	{
		//Cleanup expired suborigins.
		let tnow = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
		conn.execute("DELETE FROM applications WHERE expires < $1 AND temporary=true", &[&tnow]).
			unwrap();
		let origin: String = self.origin.as_ref().ok_or(())?.to_owned();
		//Check valid scheme.
		if !origin.starts_with("https://") && !origin.starts_with("acct:") { return Err(()); }
		//If overridden is set, force privileged.
		let privileged = privileged | self.overridden;
		//If privileged is set, the apikey has to be set. This can login to any origin with.
		//login set. Otherwise login is only allowed to those with login set and not temporary.
		let matches: i64 = if privileged {
			let apikey: String = self.key.as_ref().ok_or(())?.to_owned();
			conn.query("SELECT COUNT(*) FROM applications WHERE origin=$1 AND apikey=$2 AND \
				login=true", &[&origin, &apikey]).unwrap().iter().next().unwrap().get(0)
		} else {
			conn.query("SELECT COUNT(*) FROM applications WHERE origin=$1 AND temporary=false AND \
				login=true", &[&origin]).unwrap().iter().next().unwrap().get(0)
		};
		if matches == 0 { return Err(()); }
		let realorigin = (if let Some(pos) = origin.rfind('#') {
			&origin[..pos]
		} else {
			&origin[..]
		}).to_owned();
		Ok(conn.query("SELECT appid FROM applications WHERE origin=$1 AND temporary=false",
			&[&realorigin]).unwrap().iter().next().ok_or(())?.get(0))
	}
	fn check_write(&self, conn: &mut Connection, scene: i32) -> Result<i32, ()>
	{
		let appid = self.get_origin(conn, true)?;
		let has_access: i64 = conn.query("SELECT COUNT(sceneid) FROM application_scene WHERE \
			appid=$1 AND sceneid=$2", &[&appid,&scene]).unwrap().iter().next().unwrap().get(0);
		if has_access > 0 { Ok(appid) } else { Err(()) }
	}
}

impl<'a, 'r> FromRequest<'a, 'r> for AuthenticationInfo
{
	type Error = ();
	fn from_request(request: &'a Request<'r>) -> Outcome<AuthenticationInfo, (Status, ()), ()> {
		let h = request.headers();
		let origin = h.get_one("origin").map(|x|x.to_owned());
		let origin = origin.or_else(||h.get_one("api-origin").map(|x|x.to_owned()));
		let key = h.get_one("api-key").map(|x|x.to_owned());
		let overridden = h.contains("api-origin");
		Outcome::Success(AuthenticationInfo{origin, overridden, key})
	}
}

//Returns sub-origin and apikey.
fn create_local_token(conn: &mut Connection, username: &str, expiry: u64) -> (String, String)
{
	static BASE64URL: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
	let origin = format!("acct:{}", username);
	let mut random = [0;18];
	let mut apikey = [0;24];
	OsRng::new().unwrap().fill_bytes(&mut random);
	for i in 0..6 {
		let v = (random[3*i+0] as usize) * 65536 + (random[3*i+1] as usize) * 256 + (random[3*i+2] as usize);
		apikey[4*i+0] = BASE64URL[(v >> 18) & 0x3F];
		apikey[4*i+1] = BASE64URL[(v >> 12) & 0x3F];
		apikey[4*i+2] = BASE64URL[(v >> 6) & 0x3F];
		apikey[4*i+3] = BASE64URL[v & 0x3F];
	}
	let dt = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
	let dt = dt.as_secs() * 1000000000 + (dt.subsec_nanos() as u64);
	let suborigin = format!("{}#{}", origin, dt);
	let expiry = expiry as i64;
	let apikey = from_utf8(&apikey).unwrap().to_owned();
	conn.execute("INSERT INTO applications (origin,apikey,expires,temporary,login) VALUES \
		($1,'',0,false,false) ON CONFLICT DO NOTHING", &[&origin]).unwrap();
	conn.execute("INSERT INTO applications (origin,apikey,expires,temporary,login) VALUES \
		($1,$2,$3,true,true)", &[&suborigin, &apikey, &expiry]).unwrap();
	(suborigin, apikey)
}

const MAXI32: u32 = 0x7FFFFFFF;

struct EventInfo
{
	ts: i64,
	username: String,
	color: i32,
	x: i32,
	y: i32,
}


#[derive(Debug)]
struct SendFileAsWithCors(&'static str, Vec<u8>);

impl<'r> Responder<'r> for SendFileAsWithCors
{
	fn respond_to(self, request: &Request) -> Result<Response<'r>, Status>
	{
		let h = request.headers();
		let origin = h.get_one("origin").map(|x|x.to_owned());

		let mut response = Response::new();
		response.set_status(Status::new(200, "OK"));
		response.set_header(Header::new("Content-Type", self.0));
		if let Some(origin) = origin { if origin.starts_with("https://") {
			response.set_header(Header::new("Content-Type", self.0));
			response.set_header(Header::new("Access-Control-Allow-Origin", origin));
			response.set_header(Header::new("Access-Control-Allow-Methods", "GET, PUT, POST, OPTIONS"));
			response.set_header(Header::new("Access-Control-Allow-Headers", "api-origin, api-key, since, until, Content-Type"));
		}}
		response.set_sized_body(Cursor::new(self.1));
		Ok(response)
	}
}

/************************* NAME PERMUTATION ************************************************************************/
static SEED: &'static [u8] = b"9vk2VmEsHICVXQNMYHAOF7Fe6lzR7eMq";

fn random_f(n: u32) -> u32
{
	let mut buf = [0; 55];
	(&mut buf[..SEED.len()]).copy_from_slice(&SEED[..]);
	buf[SEED.len()+0] = (n >> 16) as u8;
	buf[SEED.len()+1] = (n >> 8) as u8;
	buf[SEED.len()+2] = (n >> 0) as u8;
	let res = md5::compute(&buf[..SEED.len()+3]);
	let res = res.as_ref();
	(res[0] as u32 & 127) * 256 + (res[1] as u32) 
}

fn permute(n: i32) -> [u8;6]
{
	let n = n as u32;
	let l = n >> 15;
	let r = n & 0x7FFF;
	let r = r ^ random_f(0x00000 | l);
	let l = l ^ random_f(0x08000 | r);
	let r = r ^ random_f(0x10000 | l);
	let l = l ^ random_f(0x18000 | r);
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
	let mut n = 0;
	for i in 0..6 {
		let c = s[i];
		n = n | ((c - match s[i] {
			65...90 => 65,
			50...55 => 24,
			_ => c,
		}) as u32) << 5 * i;
	}
	let l = n >> 15;
	let r = n & 0x7FFF;
	let l = l ^ random_f(0x18000 | r);
	let r = r ^ random_f(0x10000 | l);
	let l = l ^ random_f(0x08000 | r);
	let r = r ^ random_f(0x00000 | l);
	((l << 15) | r) as i32
}

/************************* GET SCENES ******************************************************************************/
#[get("/scenes")]
fn scenes_get(auth: AuthenticationInfo) -> Result<SendFileAsWithCors, Error>
{
	let mut conn = db_connect();
	let appid = auth.get_origin(&mut conn, false).map_err(|_|Error::InvalidOrigin)?;
	let mut retval: Vec<(String, String)> = Vec::new();
	for row in conn.query("SELECT application_scene.sceneid AS sceneid, scenes.name AS name FROM \
		application_scene, scenes WHERE appid=$1 AND scenes.sceneid=application_scene.sceneid", &[&appid]).
		unwrap().iter() {
		let x = from_utf8(&permute(row.get(0))).unwrap().to_owned();
		let y: String = row.get(1);
		retval.push((x, y));
	}
	let mut out = String::new();
	out.push_str(r#"{"#);
	let mut first = true;
	for i in retval.iter() {
		if !first { out.push(','); }
		write!(out, r#""{}":"{}""#, i.0, escape_json_string(&i.1)).unwrap();
		first = false;
	}
	out.push_str("}\n");
	//Return with headers.
	Ok(SendFileAsWithCors("application/json", out.into_bytes()))
}


/************************* OPTIONS SCENES **************************************************************************/
#[options("/scenes")]
fn scenes_options() -> Result<SendFileAsWithCors, Error>
{
	Ok(SendFileAsWithCors("text/plain", Vec::new()))
}

/************************* POST SCENES *****************************************************************************/
#[derive(FromForm)]
struct SceneInfo
{
	name: String,
	width: u32,
	height: u32,
}

#[post("/scenes", data = "<upload>")]
fn scenes_post(auth: AuthenticationInfo, upload: Form<SceneInfo>) -> Result<SendFileAsWithCors, Error>
{
	let mut conn = db_connect();
	let appid = auth.get_origin(&mut conn, true).map_err(|_|Error::InvalidOrigin)?;

	let upload = upload.into_inner();
	let name = upload.name;
	let (w, h) = if upload.width > 0 && upload.height > 0 && upload.width <= MAXI32 && upload.height <= MAXI32 {
		(upload.width as i32, upload.height as i32)
	} else {
		return Err(Error::InvalidDimensions);
	};
	let scene: i32 = conn.query("INSERT INTO scenes (name,width,height) VALUES ($1,$2,$3) RETURNING sceneid",
		&[&name, &w, &h]).unwrap().iter().next().unwrap().get(0);
	conn.execute("INSERT INTO application_scene (appid,sceneid) VALUES ($1,$2)", &[&appid, &scene]).unwrap();
	let out = format!(r#"{{"scene":{}}}"#, from_utf8(&permute(scene)).unwrap());
	Ok(SendFileAsWithCors("application/json", out.into_bytes()))
}


/************************* PUT SCENE *******************************************************************************/
#[derive(PartialEq,Eq,Debug)]
enum JsonToken
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

struct JsonStream<R:IoRead>
{
	reader: R,
	buffer: String,
	eof: bool,
}

impl<R:IoRead> JsonStream<R>
{
	fn next(&mut self) -> Result<JsonToken, ()>
	{
		while !self.eof && self.buffer.len() < 4096 {
			let mut buf = [0;8192];
			let n = self.reader.read(&mut buf).map_err(|_|())?;
			if n == 0 { self.eof = true; break; }
			self.buffer.push_str(from_utf8(&buf[..n]).map_err(|_|())?);
		}
		if let Some((n, t)) = JsonToken::next(&self.buffer, self.eof) {
			self.buffer = (&self.buffer[n..]).to_owned();
			Ok(t)
		} else {
			Err(())
		}
	}
}

//Assumes last token was StartObject.
fn parse_one_event<R:IoRead>(stream: &mut JsonStream<R>) -> Result<EventInfo, ()>
{
	let mut ts = None;
	let mut username = None;
	let mut color = None;
	let mut x = None;
	let mut y = None;
	let mut keyname;
	loop {
		//EndObject would be supposed to be checked on the first iteration. However, such thing
		//would be an error anyway.
		if let JsonToken::String(key) = stream.next()? {
			keyname = key;
		} else {
			return Err(());
		}
		if stream.next()? != JsonToken::DoubleColon { return Err(()); }
		let ntoken = stream.next()?;
		let value = if let JsonToken::String(value) = ntoken {
			value
		} else if let JsonToken::Numeric(value) = ntoken {
			value
		} else {
			return Err(());
		};
		if keyname == "ts" { ts = Some(value); }
		else if keyname == "u" { username = Some(value); }
		else if keyname == "c" { color = Some(value); }
		else if keyname == "x" { x = Some(value); }
		else if keyname == "y" { y = Some(value); }
		else { return Err(()); }

		let ntoken = stream.next()?;
		if ntoken == JsonToken::EndObject {
			match (ts, username, color, x, y) {
				(Some(ts), Some(username), Some(color), Some(x), Some(y)) =>
					return Ok(EventInfo{
						ts: i64::from_str(&ts).map_err(|_|())?,
						username: username,
						color: checkpos(i32::from_str(&color), "c").map_err(|_|())?,
						x: checkpos(i32::from_str(&x), "x").map_err(|_|())?,
						y: checkpos(i32::from_str(&y), "y").map_err(|_|())?,
					}),
				_=> return Err(())
			};
		} else if ntoken == JsonToken::Comma {
			//Skip.
		} else {
			return Err(());
		}
	}
}

fn parse_event_stream<R:IoRead>(stream: &mut R) -> Result<Vec<EventInfo>, ()>
{
	let mut out = Vec::new();
	let mut stream = JsonStream{reader:stream, buffer:String::new(), eof:false};
	if stream.next()? != JsonToken::StartObject { return Err(()); }
	if stream.next()? != JsonToken::String("data".to_owned()) { return Err(()); }
	if stream.next()? != JsonToken::DoubleColon { return Err(()); }
	if stream.next()? != JsonToken::StartArray { return Err(()); }
	let mut maybe_end = true;
	loop {
		let ntoken = stream.next()?;
		if ntoken == JsonToken::StartObject {
			out.push(parse_one_event(&mut stream)?);
			maybe_end = true;
		} else if ntoken == JsonToken::EndArray && maybe_end {
			break;
		} else if ntoken == JsonToken::Comma {
			maybe_end = false;
		} else {
			return Err(());
		}
	}
	if stream.next()? != JsonToken::EndObject { return Err(()); }
	if stream.next()? != JsonToken::None { return Err(()); }
	Ok(out)
}

#[put("/scenes/<scene>", data = "<upload>")]
fn scene_put(scene: String, auth: AuthenticationInfo, upload: Data) -> Result<SendFileAsWithCors, Error>
{
	let scene = unpermute(scene.as_bytes());
	let mut conn = db_connect();

	auth.check_write(&mut conn, scene).map_err(|_|Error::InvalidOrigin)?;

	let mut upload = upload.open();
	let events: Vec<EventInfo> = match parse_event_stream(&mut upload).map_err(|_|Error::BadEventStream) {
		Ok(x) => x,
		Err(x) => {
			//Read the event stream to the end to avoid Rocket barfing.
			let mut buf = [0;4096];
			while upload.read(&mut buf).unwrap() > 0 {}
			return Err(x);
		}
	};
	conn.execute("BEGIN TRANSACTION", &[]).unwrap();
	for i in events.iter() {
		conn.execute("INSERT INTO scene_data (sceneid,timestamp,username,color,x,y) VALUES ($1,$2,$3,$4,$5,$6) \
			ON CONFLICT DO NOTHING", &[&scene, &i.ts, &i.username, &i.color, &i.x, &i.y]).unwrap();
	}
	conn.execute("COMMIT", &[]).unwrap();
	//Ok.
	Ok(SendFileAsWithCors("text/plain", Vec::new()))
}

/************************* OPTIONS SCENE ***************************************************************************/
#[options("/scenes/<scene>")]
fn scene_options(scene: String) -> Result<SendFileAsWithCors, Error>
{
	let _ = scene;	//Shut up.
	Ok(SendFileAsWithCors("text/plain", Vec::new()))
}


/************************* POST SCENE ******************************************************************************/
fn checkpos<E>(x: Result<i32, E>, name: &str) -> Result<i32, Error>
{
	if let Ok(x) = x {
		if x >= 0 { return Ok(x); }
	}
	Err(Error::BadFormField(name.to_owned()))
}

enum ScenePostForm
{
	Event(EventInfo),
	Grant(String),
	Ungrant(String),
}

impl<'r> FromForm<'r> for ScenePostForm
{
	type Error = Error;
	fn from_form(it: &mut FormItems<'r>, strict: bool) -> Result<Self, Error>
	{
		let mut grant = None;
		let mut ungrant = None;
		let mut ts = None;
		let mut username = None;
		let mut color = None;
		let mut x = None;
		let mut y = None;
		for (key, value) in it {
			let val = value.url_decode().map_err(|_|Error::BadFormField(key.as_str().to_owned()))?;
			match key.as_str() {
				"a" => grant = Some(val),
				"d" => ungrant = Some(val),
				"ts" => ts = Some(val),
				"u" => username = Some(val),
				"c" => color = Some(val),
				"x" => x = Some(val),
				"y" => y = Some(val),
				name if strict => return Err(Error::BadFormField(name.to_owned())),
				_ => {}
			};
		}
		match (grant, ungrant, ts, username, color, x, y) {
			(Some(grant), None, None, None, None, None, None) =>
				Ok(ScenePostForm::Grant(grant)),
			(None, Some(ungrant), None, None, None, None, None) =>
				Ok(ScenePostForm::Ungrant(ungrant)),
			(None, None, Some(ts), Some(username), Some(color), Some(x), Some(y)) =>
				Ok(ScenePostForm::Event(EventInfo{
					ts: i64::from_str(&ts).map_err(|_|Error::BadFormField("ts".to_owned()))?,
					username: username,
					color: checkpos(i32::from_str(&color), "c")?,
					x: checkpos(i32::from_str(&x), "x")?,
					y: checkpos(i32::from_str(&y), "y")?,
				})),
			_ => return Err(Error::BadFormField("invalid combination".to_string()))
		}
	}
}

#[post("/scenes/<scene>", data = "<upload>")]
fn scene_post(scene: String, auth: AuthenticationInfo, upload: Form<ScenePostForm>) -> Result<SendFileAsWithCors, Error>
{
	let scene = unpermute(scene.as_bytes());
	let mut conn = db_connect();

	auth.check_write(&mut conn, scene).map_err(|_|Error::InvalidOrigin)?;

	match upload.into_inner() {
		ScenePostForm::Grant(grant) => {
			let appid: i32 = conn.query("SELECT appid FROM applications WHERE origin=$1 AND temporary=false",
				&[&grant]).unwrap().iter().next().ok_or(Error::BadGrant)?.get(0);
			conn.execute("INSERT INTO application_scene (appid,sceneid) VALUES ($1,$2)", &[&appid,
				&scene]).unwrap();
		},
		ScenePostForm::Ungrant(ungrant) => {
			let appid: i32 = conn.query("SELECT appid FROM applications WHERE origin=$1 AND temporary=false",
				&[&ungrant]).unwrap().iter().next().ok_or(Error::BadGrant)?.get(0);
			conn.execute("DELETE FROM application_scene WHERE appid=$1 AND sceneid=$2", &[&appid,
				&scene]).unwrap();
		},
		ScenePostForm::Event(ev) => {
			conn.execute("INSERT INTO scene_data (sceneid,timestamp,username,color,x,y) VALUES ($1,$2,\
				$3,$4,$5,$6) ON CONFLICT DO NOTHING", &[&scene, &ev.ts, &ev.username, &ev.color,
				&ev.x, &ev.y]).unwrap();
		}
	}
	//Ok.
	Ok(SendFileAsWithCors("text/plain", Vec::new()))
}

/************************* DELETE SCENE ****************************************************************************/
#[delete("/scenes/<scene>")]
fn scene_delete(scene: String, auth: AuthenticationInfo) -> Result<SendFileAsWithCors, Error>
{
	let scene = unpermute(scene.as_bytes());
	let mut conn = db_connect();

	auth.check_write(&mut conn, scene).map_err(|_|Error::InvalidOrigin)?;

	if conn.execute("DELETE FROM scenes WHERE sceneid=$1", &[&scene]).unwrap() == 0 {
		return Err(Error::SceneNotFound);
	}
	//Ok.
	Ok(SendFileAsWithCors("text/plain", Vec::new()))
}


/************************* GET SCENE *******************************************************************************/
struct GetBounds
{
	start: Option<i64>,
	end: Option<i64>
}

impl<'a, 'r> FromRequest<'a, 'r> for GetBounds
{
	type Error = ();
	fn from_request(request: &'a Request<'r>) -> Outcome<GetBounds, (Status, ()), ()> {
		let h = request.headers();
		let start = h.get_one("since").and_then(|x|i64::from_str(x).ok());
		let end = h.get_one("until").and_then(|x|i64::from_str(x).ok());
		Outcome::Success(GetBounds{start, end})
	}
}

fn escape_json_string<'a>(x: &'a str) -> Cow<'a, str>
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

fn format_row(target: &mut String, row: &EventInfo)
{
	let eusername = escape_json_string(&row.username);
	write!(target, r#"{{"ts":{},"u":"{}","c":{},"x":{},"y":{}}}"#, row.ts, eusername, row.color,
		row.x, row.y).unwrap();
}

#[get("/scenes/<scene>")]
fn scene_get(scene: String, range: GetBounds) -> Result<SendFileAsWithCors, Error>
{
	let scene = unpermute(scene.as_bytes());
	let conn = db_connect();
	let (w, h) = if let Some(row) = conn.query("SELECT width, height FROM scenes WHERE sceneid=$1", &[&scene]).
		unwrap().iter().next() {
		let w: i32 = row.get(0);
		let h: i32 = row.get(1);
		(w, h)
	} else {
		return Err(Error::SceneNotFound);
	};
	let tstart = range.start.unwrap_or(i64::min_value());
	let tend = range.end.unwrap_or(i64::max_value());
	let mut retval = Vec::new();
	for row in conn.query("SELECT timestamp,username,color,x,y FROM scene_data WHERE sceneid=$1 AND timestamp>=$2 \
		AND timestamp <= $3 ORDER BY timestamp, recordid", &[&scene, &tstart, &tend]).unwrap().iter() {
		retval.push(EventInfo {
			ts: row.get(0),
			username: row.get(1),
			color: row.get(2),
			x: row.get(3),
			y: row.get(4),
		});
	}
	let mut out = String::new();
	out.push_str(r#"{"data":["#);
	let mut first = true;
	for i in retval.iter() {
		if !first { out.push(','); }
		format_row(&mut out, i);
		first = false;
	}
	write!(out, r#"],"width":{},"height":{}}}"#, w, h).unwrap();
	out.push('\n');
	//Return with headers.
	Ok(SendFileAsWithCors("application/json", out.into_bytes()))
}


/************************* LSMV EXPORT *****************************************************************************/
fn write_byte(out: &mut Vec<u8>, b: u8)
{
	out.push(b);
}

fn write_u32(out: &mut Vec<u8>, b: u32)
{
	let x = [(b >> 24) as u8, (b >> 16) as u8, (b >> 8) as u8, b as u8];
	out.extend_from_slice(&x);
}

fn write_integer(out: &mut Vec<u8>, mut b: u64)
{
	while b > 127 {
		out.push(128 | (b & 127) as u8);
		b >>= 7;
	}
	out.push(b as u8);
}

fn write_heading(out: &mut Vec<u8>, htype: u32, size: u64)
{
	write_u32(out, 0xADDB2D86);
	write_u32(out, htype);
	write_integer(out, size);
}

fn write_member(out: &mut Vec<u8>, htype: u32, content: &[u8])
{
	write_heading(out, htype, content.len() as u64);
	out.extend_from_slice(content);
}

fn write_string(out: &mut Vec<u8>, content: &str)
{
	write_integer(out, content.len() as u64);
	out.extend_from_slice(content.as_bytes());
}

fn write_frame(out: &mut Vec<u8>, x: u16, y: u16, color: u32, sync: bool, spin: bool)
{
	let flags = if sync { 1 } else { 0 }  | if spin { 2 } else { 0 };
	let x = [flags, x as u8, (x >> 8) as u8, y as u8, (y >> 8) as u8, (color >> 16) as u8, 0, (color >> 8) as u8,
		0, color as u8, 0];
	out.extend_from_slice(&x);
}

struct MovieEvent
{
	timestamp: i64,
	x: u16,
	y: u16,
	color: u32,
}

fn write_lsmv_file(sceneid: &str, width: u16, height: u16, movie: &[MovieEvent]) -> Vec<u8>
{
	let mut out = Vec::with_capacity(1024 + 11 * movie.len());
	//Magic.
	out.extend_from_slice(&[0x6C, 0x73, 0x6D, 0x76, 0x1A]);
	//Systemtype.
	write_string(&mut out, "pbn");
	//Settings.
	write_byte(&mut out, 1); write_string(&mut out, "width"); write_string(&mut out, &format!("{}", width));
	write_byte(&mut out, 1); write_string(&mut out, "height"); write_string(&mut out, &format!("{}", height));
	write_byte(&mut out, 0);
	//Moivetime.
	let mut out2 = Vec::with_capacity(64);
	write_integer(&mut out2, 1000000000); write_integer(&mut out2, 0);
	write_member(&mut out, 0x18C3A975, &out2);
	//COREVERSION.
	write_member(&mut out, 0xE4344C7E, b"pbn");
	//ROMHASH.
	write_member(&mut out, 0x0428ACFC, b"\x00e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
	//ROMHINT.
	write_member(&mut out, 0x6F715830, b"\x00pbn");
	//RDATA.
	write_member(&mut out, 0xA3A07F71, b"\x1f\x00");
	//PROJECTID.
	write_member(&mut out, 0x359BFBAB, format!("scene{}", sceneid).as_bytes());
	//Moviedata.
	let mut iframecnt = 0;
	let mut lastframe = -1;
	let timebase = movie.get(0).map(|x|x.timestamp).unwrap_or(0);
	for i in movie.iter() {
		let evtime = i.timestamp - timebase;
		let framenum = 3 * evtime / 50;			//3 frames in 50ms.
		if framenum == lastframe {
			iframecnt += 1;				//New subframe.
		} else {
			iframecnt +=  framenum - lastframe;	//Padding + New frame.
			lastframe = framenum;
		}
	}
	write_heading(&mut out, 0xF3DCA44B, 11 * iframecnt as u64);
	let mut spin = true;
	lastframe = -1;
	for i in movie.iter() {
		let evtime = i.timestamp - timebase;
		let framenum = 3 * evtime / 50;			//3 frames in 50ms.
		if framenum == lastframe {
			write_frame(&mut out, i.x, i.y, i.color, false, spin);
			spin = !spin;
		} else {
			spin = false;
			for _ in lastframe+1..framenum {
				write_frame(&mut out, 0, 0, 0, true, false);
			}
			write_frame(&mut out, i.x, i.y, i.color, true, true);
			lastframe = framenum;
		}
		
	}
	out
}

#[derive(Debug)]
struct SendFileAs(&'static str, Vec<u8>);

impl<'r> Responder<'r> for SendFileAs
{
	fn respond_to(self, _request: &Request) -> Result<Response<'r>, Status>
	{
		let mut response = Response::new();
		response.set_status(Status::new(200, "OK"));
		response.set_header(Header::new("Content-Type", self.0));
		response.set_sized_body(Cursor::new(self.1));
		Ok(response)
	}
}

#[get("/scenes/<scene>/lsmv")]
fn scene_get_lsmv(scene: String) -> Result<SendFileAs, Error>
{
	let oldscene = scene;
	let scene = unpermute(oldscene.as_bytes());
	let conn = db_connect();
	let (w, h) = if let Some(row) = conn.query("SELECT width, height FROM scenes WHERE sceneid=$1", &[&scene]).
		unwrap().iter().next() {
		let w: i32 = row.get(0);
		let h: i32 = row.get(1);
		(w, h)
	} else {
		return Err(Error::SceneNotFound);
	};
	let moviedata = conn.query("SELECT timestamp,color,x,y FROM scene_data WHERE sceneid=$1 ORDER BY \
		timestamp, recordid", &[&scene]).unwrap().iter().filter_map(|ev|{
		let ts: i64 = ev.get(0);
		let color: i32 = ev.get(1);
		let x: i32 = ev.get(2);
		let y: i32 = ev.get(3);
		if x < 0 || x >= w || y < 0 || y >= h { return None; }
		Some(MovieEvent{
			timestamp: ts,
			x: x as u16,
			y: y as u16,
			color: color as u32
		})
	}).collect::<Vec<MovieEvent>>();
	let lsmv = write_lsmv_file(&oldscene, w as u16, h as u16, &moviedata);
	Ok(SendFileAs("application/x-lsnes-movie", lsmv))
}

/*
#[get("/<name>/<age>")]
fn hello(name: String, age: u8) -> String {
	format!("Hello, {} year old named {}!", age, name)
}
*/
fn main() {
	rocket::ignite().mount("/pbn", routes![
		//hello,
		scene_options,
		scene_get,
		scene_put,
		scene_post,
		scene_delete,
		scene_get_lsmv,
		scenes_options,
		scenes_get,
		scenes_post,
	]).launch();
}

#[cfg(test)]
mod tests;
