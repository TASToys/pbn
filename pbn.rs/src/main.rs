#![feature(plugin, decl_macro, custom_derive)]
#![plugin(rocket_codegen)]

extern crate rocket;
extern crate postgres;
extern crate md5;
use postgres::{Connection, TlsMode};
use rocket::request::{FromRequest, Form, Request};
use rocket::outcome::Outcome;
use rocket::response::{Responder, Response};
use rocket::http::{Header, Status};
use rocket::Data;
use std::borrow::Cow;
use std::fmt::Write as FmtWrite;
use std::io::Cursor;
use std::str::{FromStr, from_utf8};

#[derive(Debug)]
enum Error
{
	SceneNotFound,
	InvalidOrigin,
	InvalidDimensions,
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
				let body = "Invalid origin\n".to_owned().into_bytes();
				response.set_status(Status::new(418, "Invalid dimensions"));
				response.set_header(Header::new("Content-Type", "text/plain"));
				response.set_sized_body(Cursor::new(body));
				Ok(response)
			}
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

#[get("/scenes")]
fn scenes_get(auth: AuthenticationInfo) -> Result<SendFileAsWithCors, Error>
{
	let mut conn = db_connect();
	let origin = auth.is_ok(&mut conn, false).map_err(|_|Error::InvalidOrigin)?;
	let appid: i32 = conn.query("SELECT appid FROM applications WHERE origin=$1", &[&origin]).unwrap().iter().
		next().unwrap().get(0);
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
	out.push_str(r#"}\n"#);
	//Return with headers.
	Ok(SendFileAsWithCors("application/json", out.into_bytes()))
}

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

struct AuthenticationInfo
{
	origin: Option<String>,
	overridden: bool,
	key: Option<String>,
}

impl AuthenticationInfo
{
	fn is_ok(&self, conn: &mut Connection, privileged: bool) -> Result<String, ()>
	{
		let origin = self.origin.as_ref().ok_or(())?;
		//If overridden is set, force privileged.
		let privileged = privileged | self.overridden;
		//If privileged is set, the apikey has to be set. This can login to any origin with.
		//login set. Otherwise login is only allowed to those with login set and not temporary.
		let matches: i32 = if privileged {
			let apikey: String = self.key.as_ref().ok_or(())?.to_owned();
			let origin = origin.to_owned();
			conn.query("SELECT COUNT(*) FROM applications WHERE origin=$1 AND apikey=$2 AND \
				login=true", &[&origin, &apikey]).unwrap().iter().next().unwrap().get(0)
		} else {
			let origin = origin.to_owned();
			conn.query("SELECT COUNT(*) FROM applications WHERE origin=$1 AND temporary=false AND \
				login=true", &[&origin]).unwrap().iter().next().unwrap().get(0)
		};
		if matches == 0 { return Err(()); }
		if let Some(pos) = origin.find('#') {
			Ok((&origin[..pos]).to_owned())
		} else {
			Ok(origin.to_owned())
		}
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

fn check_write_access(conn: &Connection, origin: &str, scene: i32) -> Result<(), ()>
{
	let origin = origin.to_owned();
	let has_access: i32 = conn.query("SELECT COUNT(sceneid) FROM application_scene, applications WHERE \
		origin=$1 AND sceneid=$2 AND application_scene.appid=applications.appid", &[&origin,&scene]).
		unwrap().iter().next().unwrap().get(0);
	if has_access > 0 { Ok(()) } else { Err(()) }
}


#[options("/scenes")]
fn scenes_options() -> Result<SendFileAsWithCors, Error>
{
	Ok(SendFileAsWithCors("text/plain", Vec::new()))
}

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
	let origin = auth.is_ok(&mut conn, true).map_err(|_|Error::InvalidOrigin)?;

	let upload = upload.into_inner();
	let name = upload.name;
	let (w, h) = if upload.width > 0 && upload.height > 0 && upload.width < 1999999999 && upload.height < 1999999999 {
		(upload.width as i32, upload.height as i32)
	} else {
		return Err(Error::InvalidDimensions);
	};
	let scene: i32 = conn.query("INSERT INTO scenes (name,width,height) VALUES ($1,$2,$3) RETURNING sceneid",
		&[&name, &w, &h]).unwrap().iter().next().unwrap().get(0);
	let appid: i32 = conn.query("SELECT appid FROM applications WHERE origin=$1", &[&origin]).unwrap().iter().
		next().unwrap().get(0);
	conn.execute("INSERT INTO application_scene (appid,sceneid) VALUES ($1,$2)", &[&appid, &scene]).unwrap();
	let out = format!(r#"{{"scene":{}}}"#, from_utf8(&permute(scene)).unwrap());
	Ok(SendFileAsWithCors("application/json", out.into_bytes()))
}


struct EventInfo
{
	ts: i64,
	username: String,
	color: i32,
	x: i32,
	y: i32,
}

impl EventInfo
{
	fn from_ei2(e: EventInfo2) -> EventInfo
	{
		EventInfo{ts:e.ts, username:e.u, color:e.c, x:e.x, y:e.y}
	}
}

#[derive(FromForm)]
struct EventInfo2
{
	ts: i64,
	u: String,
	c: i32,
	x: i32,
	y: i32,
}


#[options("/scenes/<scene>")]
fn scene_options(scene: String) -> Result<SendFileAsWithCors, Error>
{
	let _ = scene;	//Shut up.
	Ok(SendFileAsWithCors("text/plain", Vec::new()))
}

#[put("/scenes/<scene>", data = "<upload>")]
fn scene_put(scene: String, auth: AuthenticationInfo, upload: Data) -> Result<SendFileAsWithCors, Error>
{
	let scene = unpermute(scene.as_bytes());
	let mut conn = db_connect();

	let origin = auth.is_ok(&mut conn, true).map_err(|_|Error::InvalidOrigin)?;
	check_write_access(&mut conn, &origin, scene).map_err(|_|Error::InvalidOrigin)?;

	let upload = upload.open();
	let events: Vec<EventInfo> = Vec::new();
	//FIXME: Parse input stream (which acts like Read) into Vector of EventInfo.
	conn.execute("BEGIN TRANSACTION", &[]).unwrap();
	for i in events.iter() {
		conn.execute("INSERT INTO scene_data (sceneid,timestamp,username,color,x,y) VALUES ($1,$2,$3,$4,$5,$6) \
			ON CONFLICT DO NOTHING", &[&scene, &i.ts, &i.username, &i.color, &i.x, &i.y]).unwrap();
	}
	conn.execute("COMMIT", &[]).unwrap();
	//Ok.
	Ok(SendFileAsWithCors("text/plain", Vec::new()))
}


#[post("/scenes/<scene>", data = "<upload>")]
fn scene_post(scene: String, auth: AuthenticationInfo, upload: Form<EventInfo2>) -> Result<SendFileAsWithCors, Error>
{
	let scene = unpermute(scene.as_bytes());
	let mut conn = db_connect();

	let origin = auth.is_ok(&mut conn, true).map_err(|_|Error::InvalidOrigin)?;
	check_write_access(&mut conn, &origin, scene).map_err(|_|Error::InvalidOrigin)?;

	let ev = EventInfo::from_ei2(upload.into_inner());
	conn.execute("INSERT INTO scene_data (sceneid,timestamp,username,color,x,y) VALUES ($1,$2,$3,$4,$5,$6) \
		ON CONFLICT DO NOTHING", &[&scene, &ev.ts, &ev.username, &ev.color, &ev.x, &ev.y]).unwrap();
	//Ok.
	Ok(SendFileAsWithCors("text/plain", Vec::new()))
}

#[delete("/scenes/<scene>")]
fn scene_delete(scene: String, auth: AuthenticationInfo) -> Result<SendFileAsWithCors, Error>
{
	let scene = unpermute(scene.as_bytes());
	let mut conn = db_connect();

	let origin = auth.is_ok(&mut conn, true).map_err(|_|Error::InvalidOrigin)?;
	check_write_access(&mut conn, &origin, scene).map_err(|_|Error::InvalidOrigin)?;

	if conn.execute("DELETE FROM scenes WHERE sceneid=$1", &[&scene]).unwrap() == 0 {
		return Err(Error::SceneNotFound);
	}
	//Ok.
	Ok(SendFileAsWithCors("text/plain", Vec::new()))
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
		if let Some(origin) = origin {
			response.set_header(Header::new("Content-Type", self.0));
			response.set_header(Header::new("Access-Control-Allow-Origin", origin));
			response.set_header(Header::new("Access-Control-Allow-Methods", "GET, PUT, POST, OPTIONS"));
			response.set_header(Header::new("Access-Control-Allow-Headers", "api-origin, api-key, since, until, Content-Type"));
		}
		response.set_sized_body(Cursor::new(self.1));
		Ok(response)
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
	write!(out, r#"],"width":{},"height":{}}}\n"#, w, h).unwrap();
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
	rocket::ignite().mount("/test", routes![
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
