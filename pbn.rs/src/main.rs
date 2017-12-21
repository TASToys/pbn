#![feature(plugin, decl_macro, custom_derive, test)]
#![plugin(rocket_codegen)]
#![deny(unsafe_code)]

extern crate rocket;
extern crate postgres;
extern crate md5;
extern crate rand;
extern crate libc;
use postgres::{Connection, TlsMode};
use rocket::request::{FromRequest, FromForm, FromSegments, Form, FormItems, Request};
use rocket::outcome::Outcome;
use rocket::response::{Responder, Response, NamedFile};
use rocket::http::{Header, Status, ContentType};
use rocket::http::uri::Segments;
use rocket::Data;
use std::borrow::Cow;
use std::fmt::Write as FmtWrite;
use std::io::Cursor;
use std::ops::Deref;
use std::str::{FromStr, from_utf8};
use std::io::Read as IoRead;
use std::path::{Path, PathBuf};
use std::marker::PhantomData;

mod json;
use json::{JsonStream, JsonToken, escape_json_string};
mod lsmv;
use lsmv::{scene_get_lsmv as _scene_get_lsmv, SendFileAs};
mod error;
use error::Error;
mod authentication;
use authentication::AuthenticationInfo;
mod scene;
use scene::Scene;
mod mmapstate;
use mmapstate::MmapImageState;
mod png;
use png::{scan_image_as_png, scan_image_as_png_size};

fn db_connect() -> Connection
{
	Connection::connect("pq://pbn@%2fvar%2frun%2fpostgresql%2f/pbndb", TlsMode::None).unwrap()
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

/************************* RAW FILE SERVING ************************************************************************/
#[derive(Debug)]
struct MoreContentTypeGuessing<'r,R:Responder<'r>>(R, PathBuf, PhantomData<&'r u8>);

impl<'r,R:Responder<'r>> MoreContentTypeGuessing<'r,R>
{
	fn new(raw: R, path: PathBuf) -> MoreContentTypeGuessing<'r,R>
	{
		MoreContentTypeGuessing(raw, path, PhantomData)
	}
	fn guess_more(p: &Path) -> (&'static str, &'static str)
	{
		let ext = p.extension().map(|x|x.to_string_lossy()).unwrap_or(Cow::Borrowed(""));
		let ext = ext.deref();
		match ext {
			"lsmv" => ("application", "x-lsnes-movie"),
			"mkv" => ("application", "matroska"),
			_ => ("application", "octet-stream")
		}
	}
}

impl<'r,R:Responder<'r>> Responder<'r> for MoreContentTypeGuessing<'r,R>
{
	fn respond_to(self, request: &Request) -> Result<Response<'r>, Status>
	{
		let ans = self.0.respond_to(request);
		if let Ok(mut ans) = ans {
			if ans.content_type().is_none() {
				let (top, sub) = Self::guess_more(&self.1);
				ans.set_header(ContentType::new(top, sub));
			}
			Ok(ans)
		} else {
			ans
		}
	}	
}

//Pathbuf as parameter does not accept path transversal.
#[get("/static/<file..>")]
fn serve_static_files(file: PathBuf) -> Option<MoreContentTypeGuessing<'static,NamedFile>> {
	NamedFile::open(Path::new("/home/pbn/static/").join(&file)).ok().map(|f|MoreContentTypeGuessing::new(f,
		file))
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
		let x: Scene = row.get(0);
		let x = from_utf8(&x.scramble()).unwrap().to_owned();
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
	let scene: Scene = conn.query("INSERT INTO scenes (name,width,height) VALUES ($1,$2,$3) RETURNING sceneid",
		&[&name, &w, &h]).unwrap().iter().next().unwrap().get(0);
	conn.execute("INSERT INTO application_scene (appid,sceneid) VALUES ($1,$2)", &[&appid, &scene]).unwrap();
	let out = format!(r#"{{"scene":{}}}"#, from_utf8(&scene.scramble()).unwrap());
	Ok(SendFileAsWithCors("application/json", out.into_bytes()))
}


/************************* PUT SCENE *******************************************************************************/
//Assumes last token was StartObject.
fn parse_one_event<R:IoRead>(stream: &mut JsonStream<R>) -> Result<EventInfo, String>
{
	let mut ts = None;
	let mut username = None;
	let mut color = None;
	let mut x = None;
	let mut y = None;
	stream.do_object(|stream, key|{
		if key == "ts" {
			ts = Some(match stream.next(&|x|format!("Error reading Event: {}", x))? {
				JsonToken::NumericInteger(value) => value,
				x => return Err(format!("Expected integer for event key 'ts', got {:?}", x))
			});
		} else if key == "u" {
			username = Some(match stream.next(&|x|format!("Error reading Event: {}", x))? {
				JsonToken::String(value) => value,
				x => return Err(format!("Expected string for event key 'u', got {:?}", x))
			});
		} else if key == "c" {
			color = Some(match stream.next(&|x|format!("Error reading Event: {}", x))? {
				JsonToken::NumericInteger(value) => checkpos2(value, "c")?,
				x => return Err(format!("Expected integer for event key 'c', got {:?}", x))
			});
		} else if key == "x" {
			x = Some(match stream.next(&|x|format!("Error reading Event: {}", x))? {
				JsonToken::NumericInteger(value) => checkpos2(value, "x")?,
				x => return Err(format!("Expected integer for event key 'x', got {:?}", x))
			});
		} else if key == "y" {
			y = Some(match stream.next(&|x|format!("Error reading Event: {}", x))? {
				JsonToken::NumericInteger(value) => checkpos2(value, "y")?,
				x => return Err(format!("Expected integer for event key 'y', got {:?}", x))
			});
		} else { return Err(format!("Unrecognized event key '{}'", key)); }
		Ok(())
	}, &|x|format!("Error in Event object: {}", x))?;
	match (ts, username, color, x, y) {
		(Some(ts), Some(username), Some(color), Some(x), Some(y)) => Ok(EventInfo{
			ts: ts,
			username: username,
			color: color,
			x: x,
			y: y,
		}),
		_=> Err(format!("Need fields ts, u, c, x and y for Event object"))
	}
}

fn parse_event_stream<R:IoRead,F>(stream: &mut R, sink: &F) -> Result<u64, String> where F: Fn(EventInfo)
{
	let mut events = 0;
	let mut stream = JsonStream::new(stream);
	stream.expect_object().map_err(|x|format!("Expecting start of events object: {}", x))?;
	stream.do_object(|stream, key|{
		if key.deref() == "data" {
			stream.expect_array().map_err(|x|format!("Expected events->data to be an array: {}", x))?;
			stream.do_array(|stream|{
				stream.expect_object().map_err(|x|format!("Expecting start of event object: {}",
					x))?;
				let ev = parse_one_event(stream)?;
				sink(ev);
				events += 1;
				Ok(())
			}, &|x|format!("Error in events->data array: {}", x))
		} else {
			Err(format!("Unexpected field '{}' in events object", key))
		}
	}, &|x|format!("Error in events object: {}", x))?;
	stream.expect_end_of_json().map_err(|x|format!("Expected end of JSON: {}", x))?;
	Ok(events)
}

#[put("/scenes/<scene>", data = "<upload>")]
fn scene_put(scene: Scene, auth: AuthenticationInfo, upload: Data) -> Result<SendFileAsWithCors, Error>
{
	let mut conn = db_connect();

	match auth.check_write(&mut conn, scene) { Ok(_) => (), Err(_) => {
		return Err(sink_put(upload, Error::InvalidOrigin));	//Don't barf.
	}};

	//Grab width and height of scene.
	let (w, h) = if let Some(row) = conn.query("SELECT width, height FROM scenes WHERE sceneid=$1", &[&scene]).
		unwrap().iter().next() {
		let w: i32 = row.get(0);
		let h: i32 = row.get(1);
		(w, h)
	} else {
		return Err(sink_put(upload, Error::SceneNotFound));	//Don't barf.
	};

	//Use prepared statement to improve performance.
	let mmap = MmapImageState::new(format!("/home/pbn/currentstate/{}", scene.as_inner()), w as usize, h as
		usize).unwrap();
	let stmt = conn.prepare("INSERT INTO scene_data (sceneid,timestamp,username,color,x,y) VALUES \
		($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING").unwrap();
	conn.execute("BEGIN TRANSACTION", &[]).unwrap();
	let mut upload = upload.open();
	let events = match parse_event_stream(&mut upload, &|ev|{
			mmap.write_pixel(ev.x, ev.y, ev.ts, ev.color);
			stmt.execute(&[&scene, &ev.ts, &ev.username, &ev.color, &ev.x, &ev.y]).unwrap();
		}).map_err(|x|Error::BadEventStream(x)) {
		Ok(x) => x,
		Err(x) => return Err(sink_put_remaining(upload, x))
	};
	conn.execute("COMMIT", &[]).unwrap();
	//Ok.
	Ok(SendFileAsWithCors("text/plain", format!("Wrote {} event(s)\n", events).into_bytes()))
}

/************************* OPTIONS SCENE ***************************************************************************/
#[options("/scenes/<scene>")]
fn scene_options(scene: Scene) -> Result<SendFileAsWithCors, Error>
{
	let _ = scene;	//Shut up.
	Ok(SendFileAsWithCors("text/plain", Vec::new()))
}


/************************* POST SCENE ******************************************************************************/
fn checkpos2(x: i64, name: &str) -> Result<i32, String>
{
	if x >= 0 && x <= 0x7FFFFFFF { return Ok(x as i32); }
	Err(format!("Integer value for field '{}' out of range", name))
}

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
fn scene_post(scene: Scene, auth: AuthenticationInfo, upload: Form<ScenePostForm>) -> Result<SendFileAsWithCors, Error>
{
	let mut conn = db_connect();

	auth.check_write(&mut conn, scene).map_err(|_|Error::InvalidOrigin)?;

	//Grab width and height of scene.
	let (w, h) = if let Some(row) = conn.query("SELECT width, height FROM scenes WHERE sceneid=$1", &[&scene]).
		unwrap().iter().next() {
		let w: i32 = row.get(0);
		let h: i32 = row.get(1);
		(w, h)
	} else {
		return Err(Error::SceneNotFound);
	};

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
			let mmap = MmapImageState::new(format!("/home/pbn/currentstate/{}", scene.as_inner()),
				w as usize, h as usize).unwrap();
			mmap.write_pixel(ev.x, ev.y, ev.ts, ev.color);
			conn.execute("INSERT INTO scene_data (sceneid,timestamp,username,color,x,y) VALUES ($1,$2,\
				$3,$4,$5,$6) ON CONFLICT DO NOTHING", &[&scene, &ev.ts, &ev.username, &ev.color,
				&ev.x, &ev.y]).unwrap();
		}
	}
	//Ok.
	Ok(SendFileAsWithCors("text/plain", format!("Wrote an event\n").into_bytes()))
}

/************************* DELETE SCENE ****************************************************************************/
#[delete("/scenes/<scene>")]
fn scene_delete(scene: Scene, auth: AuthenticationInfo) -> Result<SendFileAsWithCors, Error>
{
	let mut conn = db_connect();

	auth.check_write(&mut conn, scene).map_err(|_|Error::InvalidOrigin)?;

	if conn.execute("DELETE FROM scenes WHERE sceneid=$1", &[&scene]).unwrap() == 0 {
		return Err(Error::SceneNotFound);
	}
	//Ok.
	Ok(SendFileAsWithCors("text/plain", format!("Deleted a scene\n").into_bytes()))
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

fn format_row(target: &mut String, row: &EventInfo)
{
	let eusername = escape_json_string(&row.username);
	write!(target, r#"{{"ts":{},"u":"{}","c":{},"x":{},"y":{}}}"#, row.ts, eusername, row.color,
		row.x, row.y).unwrap();
}

#[get("/scenes/<scene>")]
fn scene_get(scene: Scene, range: GetBounds) -> Result<SendFileAsWithCors, Error>
{
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

/************************* PNG EXPORT ******************************************************************************/
#[get("/scenes/<scene>/png")]
fn scene_get_png(scene: Scene) -> Result<SendFileAs, Error>
{
	let conn = db_connect();
	//Grab width and height of scene.
	let (w, h) = if let Some(row) = conn.query("SELECT width, height FROM scenes WHERE sceneid=$1", &[&scene]).
		unwrap().iter().next() {
		let w: i32 = row.get(0);
		let h: i32 = row.get(1);
		(w, h)
	} else {
		return Err(Error::SceneNotFound);
	};
	let mmap = MmapImageState::new(format!("/home/pbn/currentstate/{}", scene.as_inner()),
		w as usize, h as usize).unwrap();
	let mut out = Cursor::new(Vec::with_capacity(scan_image_as_png_size(&mmap)));
	scan_image_as_png(&mut out, &mmap);
	let out = out.into_inner();
	Ok(SendFileAs("application/png", out))
}


/************************* LSMV EXPORT *****************************************************************************/
#[get("/scenes/<scene>/lsmv")]
fn scene_get_lsmv(scene: Scene) -> Result<SendFileAs, Error>
{
	_scene_get_lsmv(scene)
}

/************************* PUT FALLBACK *****************************************************************************/
fn sink_put_remaining<R:IoRead,T:Sized>(mut stream: R, error: T) -> T
{
	//Read the event stream to the end to avoid Rocket barfing.
	let mut buf = [0;4096];
	while stream.read(&mut buf).unwrap() > 0 {}
	error
}

fn sink_put<T:Sized>(upload: Data, error: T) -> T
{
	sink_put_remaining(upload.open(), error)
}

struct AnySegments;

impl<'a> FromSegments<'a> for AnySegments
{
	type Error = ();
	fn from_segments(_: Segments<'a>) -> Result<Self, Self::Error>
	{
		//Always trivially succeeds.
		Ok(AnySegments)
	}
}

//These routes act as catch-all for requests. This is mainly to sink failing PUT requests, as those would cause
//rocket to barf. 998 and 999 is very low priority.
#[put("/scenes/<_x..>", data = "<upload>", rank = 998)]
fn put_scene_fallback(_x: AnySegments, upload: Data) -> Error { sink_put(upload, Error::SceneNotFound) }
#[post("/scenes/<_x..>", data = "<upload>", rank = 998)]
fn post_scene_fallback(_x: AnySegments, upload: Data) -> Error { sink_put(upload, Error::SceneNotFound) }
#[delete("/scenes/<_x..>", data = "<upload>", rank = 998)]
fn delete_scene_fallback(_x: AnySegments, upload: Data) -> Error { sink_put(upload, Error::SceneNotFound) }
#[get("/scenes/<_x..>", rank = 998)]
fn get_scene_fallback(_x: AnySegments) -> Error { Error::SceneNotFound }
#[options("/scenes/<_x..>", rank = 998)]
fn options_scene_fallback(_x: AnySegments) -> Error { Error::SceneNotFound }
#[put("/<_x..>", data = "<upload>", rank = 999)]
fn put_fallback(_x: AnySegments, upload: Data) -> Error { sink_put(upload, Error::MethodNotSupported) }
#[post("/<_x..>", data = "<upload>", rank = 999)]
fn post_fallback(_x: AnySegments, upload: Data) -> Error { sink_put(upload, Error::MethodNotSupported) }
#[delete("/<_x..>", data = "<upload>", rank = 999)]
fn delete_fallback(_x: AnySegments, upload: Data) -> Error { sink_put(upload, Error::MethodNotSupported) }
#[get("/<_x..>", rank = 999)]
fn get_fallback(_x: AnySegments) -> Error { Error::NotFound }
#[options("/<_x..>", rank = 999)]
fn options_fallback(_x: AnySegments) -> Error { Error::NotFound }

fn main() {
	rocket::ignite().mount("/", routes![
		//hello,
		serve_static_files,
		scene_options,
		scene_get,
		scene_put,
		scene_post,
		scene_delete,
		scene_get_lsmv,
		scene_get_png,
		scenes_options,
		scenes_get,
		scenes_post,
		//Scene Fallbacks.
		put_scene_fallback,
		post_scene_fallback,
		delete_scene_fallback,
		get_scene_fallback,
		options_scene_fallback,
		//Global Fallbacks.
		put_fallback,
		post_fallback,
		delete_fallback,
		get_fallback,
		options_fallback,
	]).launch();
}


#[cfg(test)]
mod tests;
