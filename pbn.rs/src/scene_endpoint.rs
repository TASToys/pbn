use ::{db_connect, sink_put, sink_put_remaining};
use ::authentication::AuthenticationInfo;
use ::cors::SendFileAsWithCors;
use ::error::Error;
use ::json::{JsonToken, JsonStream, escape_json_string};
use ::lsmv::scene_get_lsmv as _scene_get_lsmv;
use ::mmapstate::MmapImageState;
use ::png::{scan_image_as_png, scan_image_as_png_size};
use ::scene::Scene;
use rocket::request::{FromRequest, FromForm, Form, FormItems, Request};
use rocket::outcome::Outcome;
use rocket::response::Responder;
use rocket::http::Status;
use rocket::http::uri::Uri;
use rocket::Data;
use std::borrow::Cow;
use std::fmt::Write as FmtWrite;
use std::io::Write as IoWrite;
use std::fs::{File,rename};
use std::io::Cursor;
use std::io::Read as IoRead;
use std::ops::Deref;
use std::str::FromStr;

pub struct EventInfo
{
	pub ts: i64,
	pub username: String,
	pub color: i32,
	pub x: i32,
	pub y: i32,
}

pub struct GetBounds
{
	start: Option<i64>,
	end: Option<i64>
}

impl<'a, 'r> FromRequest<'a, 'r> for GetBounds
{
	type Error = ();
	fn from_request(request: &'a Request<'r>) -> Outcome<GetBounds, (Status, ()), ()> {
		let mut start = None;
		let mut end = None;
		let query = request.uri().query().unwrap_or("");
		for i in query.split("&") {
			let i = Uri::percent_decode(i.as_bytes()).unwrap_or(Cow::Borrowed(""));
			let p = i.deref();
			if p.starts_with("since=") {
				i64::from_str(&p[6..]).map(|x|start = Some(x)).ok();
			}
			if p.starts_with("until=") {
				i64::from_str(&p[6..]).map(|x|end = Some(x)).ok();
			}
		}
		Outcome::Success(GetBounds{start, end})
	}
}

fn format_row(target: &mut String, row: &EventInfo)
{
	let eusername = escape_json_string(&row.username);
	write!(target, r#"{{"ts":{},"u":"{}","c":{},"x":{},"y":{}}}"#, row.ts, eusername, row.color,
		row.x, row.y).unwrap();
}

const SCENE_METHODS: &'static str = "HEAD, GET";
const SCENE_HEADERS: &'static str = "";

pub fn scene_options(scene: Scene) -> Result<impl Responder<'static>, Error>
{
	let _ = scene;	//Shut up.
	Ok(SendFileAsWithCors{
		content_type: "text/plain",
		content: Vec::new(),
		methods: SCENE_METHODS,
		headers: SCENE_HEADERS
	})
}

pub fn scene_get(scene: Scene, range: GetBounds) -> Result<impl Responder<'static>, Error>
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
	Ok(SendFileAsWithCors{
		content_type: "application/json",
		content: out.into_bytes(),
		methods: SCENE_METHODS,
		headers: SCENE_HEADERS
	})
}

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

const SCENE_EDIT_METHODS: &'static str = "PUT, POST, DELETE";
const SCENE_EDIT_HEADERS: &'static str = "api-origin, api-key, content-type";

pub fn scene_edit_options() -> Result<impl Responder<'static>, Error>
{
	Ok(SendFileAsWithCors{
		content_type: "text/plain",
		content: Vec::new(),
		methods: SCENE_EDIT_METHODS,
		headers: SCENE_EDIT_HEADERS,
	})
}

pub fn scene_edit_put(scene: Scene, auth: AuthenticationInfo, upload: Data) -> Result<impl Responder<'static>, Error>
{
	let mut conn = db_connect();

	match auth.check_write(&mut conn, scene) {
		Ok(_) => (),
		Err(false) => return Err(sink_put(upload, Error::SceneNotFound)),	//Don't barf.
		Err(true) => return Err(sink_put(upload, Error::InvalidOrigin)),	//Don't barf.
	};

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
	Ok(SendFileAsWithCors{
		content_type: "text/plain",
		content: format!("Wrote {} event(s)\n", events).into_bytes(),
		methods: SCENE_EDIT_METHODS,
		headers: SCENE_EDIT_HEADERS,
	})
}

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

pub enum ScenePostForm
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

pub fn scene_edit_post(scene: Scene, auth: AuthenticationInfo, upload: Form<ScenePostForm>) ->
	Result<impl Responder<'static>, Error>
{
	let mut conn = db_connect();

	auth.check_write(&mut conn, scene).map_err(|x|
		if x { Error::InvalidOrigin } else { Error::SceneNotFound }
	)?;

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
	Ok(SendFileAsWithCors{
		content_type: "text/plain",
		content: format!("Wrote an event\n").into_bytes(),
		methods: SCENE_EDIT_METHODS,
		headers: SCENE_EDIT_HEADERS,
	})
}


pub fn scene_edit_delete(scene: Scene, auth: AuthenticationInfo) -> Result<impl Responder<'static>, Error>
{
	let mut conn = db_connect();

	auth.check_write(&mut conn, scene).map_err(|x|
		if x { Error::InvalidOrigin } else { Error::SceneNotFound }
	)?;

	if conn.execute("DELETE FROM scenes WHERE sceneid=$1", &[&scene]).unwrap() == 0 {
		return Err(Error::SceneNotFound);
	}
	//Ok.
	Ok(SendFileAsWithCors{
		content_type: "text/plain",
		content: format!("Deleted a scene\n").into_bytes(),
		methods: SCENE_EDIT_METHODS,
		headers: SCENE_EDIT_HEADERS,
	})
}

pub fn scene_get_png(scene: Scene) -> Result<impl Responder<'static>, Error>
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
	Ok(SendFileAsWithCors{
		content_type: "application/png",
		content: out,
		methods: "HEAD, GET",
		headers: ""
	})
}

const SCENE_CONFIG_METHODS: &'static str = "HEAD, GET, PUT";
const SCENE_CONFIG_HEADERS: &'static str = "api-origin, api-key, content-type";

pub fn scene_get_lsmv(scene: Scene) -> Result<impl Responder<'static>, Error>
{
	_scene_get_lsmv(scene)
}


pub fn scene_config_options() -> Result<impl Responder<'static>, Error>
{
	Ok(SendFileAsWithCors{
		content_type: "text/plain",
		content: Vec::new(),
		methods: SCENE_CONFIG_METHODS,
		headers: SCENE_CONFIG_HEADERS
	})
}

pub fn scene_config_get(scene: Scene) -> Result<impl Responder<'static>, Error>
{
	let conn = db_connect();
	let (_w, _h) = if let Some(row) = conn.query("SELECT width, height FROM scenes WHERE sceneid=$1", &[&scene]).
		unwrap().iter().next() {
		let w: i32 = row.get(0);
		let h: i32 = row.get(1);
		(w, h)
	} else {
		return Err(Error::SceneNotFound);
	};
	let mut content = Vec::new();
	if File::open(format!("/home/pbn/sconfigs/{}", scene.as_inner())).and_then(|mut f|f.read_to_end(
		&mut content)).is_err() {
		return Ok(SendFileAsWithCors{
			content_type: "application/octet-stream",
			content: Vec::new(),
			methods: SCENE_CONFIG_METHODS,
			headers: SCENE_CONFIG_HEADERS
		})
	}
	return Ok(SendFileAsWithCors{
		content_type: "application/octet-stream",
		content: content,
		methods: SCENE_CONFIG_METHODS,
		headers: SCENE_CONFIG_HEADERS
	})
}

pub fn scene_config_put(scene: Scene, auth: AuthenticationInfo, upload: Data) ->
	Result<impl Responder<'static>, Error>
{
	let mut conn = db_connect();

	match auth.check_write(&mut conn, scene) {
		Ok(_) => (),
		Err(false) => return Err(sink_put(upload, Error::SceneNotFound)),	//Don't barf.
		Err(true) => return Err(sink_put(upload, Error::InvalidOrigin)),	//Don't barf.
	};
	let mut upload = upload.open();
	let mut upbuf = [0;16385];
	let mut fill = 0;
	loop {
		if fill >= upbuf.len() {
			return Err(sink_put_remaining(upload, Error::ConfigTooBig));
		}
		let amt = upload.read(&mut upbuf[fill..]).unwrap();
		if amt == 0 { break; }
		fill += amt;
	}
	let tname = format!("/home/pbn/sconfigs/{}.tmp", scene.as_inner());
	let fname = format!("/home/pbn/sconfigs/{}", scene.as_inner());
	File::create(&tname).and_then(|mut f|f.write_all(&upbuf[..fill])).unwrap();
	rename(&tname, &fname).unwrap();
	//Ok.
	return Ok(SendFileAsWithCors{
		content_type: "text/plain",
		content: Vec::new(),
		methods: SCENE_CONFIG_METHODS,
		headers: SCENE_CONFIG_HEADERS
	})
}
