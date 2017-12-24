use ::db_connect;
use ::authentication::AuthenticationInfo;
use ::cors::SendFileAsWithCors;
use ::error::Error;
use ::json::escape_json_string;
use ::scene::Scene;
use rocket::request::Form;
use rocket::response::Responder;
use std::str::from_utf8;
use std::fmt::Write as FmtWrite;

const SCENES_METHODS: &'static str = "HEAD, GET, POST";
const SCENES_HEADERS: &'static str = "api-origin, api-key, content-type";

pub fn scenes_options() -> impl Responder<'static>
{
	if false { return Err(Error::SceneNotFound); }	//Dummy error for type inference.
	Ok(SendFileAsWithCors{
		content_type: "text/plain",
		content: Vec::new(),
		methods: SCENES_METHODS,
		headers: SCENES_HEADERS,
	})
}

pub fn scenes_get(auth: AuthenticationInfo) -> impl Responder<'static>
{
	if false { return Err(Error::SceneNotFound); }	//Dummy error for type inference.
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
	Ok(SendFileAsWithCors{
		content_type: "application/json",
		content: out.into_bytes(),
		methods: SCENES_METHODS,
		headers: SCENES_HEADERS,
	})
}

const MAXI32: u32 = 0x7FFFFFFF;
const MAXPIXELS: u32 = 1 << 21;

#[derive(FromForm)]
pub struct SceneInfo
{
	name: String,
	width: u32,
	height: u32,
}

pub fn scenes_post(auth: AuthenticationInfo, upload: Form<SceneInfo>) -> impl Responder<'static>
{
	let mut conn = db_connect();
	let appid = auth.get_origin(&mut conn, true).map_err(|_|Error::InvalidOrigin)?;

	let upload = upload.into_inner();
	let name = upload.name;
	let (w, h) = if upload.width > 0 && upload.height > 0 && upload.width <= MAXI32 && upload.height <= MAXI32
		&& upload.width.checked_mul(upload.height).unwrap_or(MAXI32) <= MAXPIXELS {
		(upload.width as i32, upload.height as i32)
	} else {
		return Err(Error::InvalidDimensions);
	};
	let scene: Scene = conn.query("INSERT INTO scenes (name,width,height) VALUES ($1,$2,$3) RETURNING sceneid",
		&[&name, &w, &h]).unwrap().iter().next().unwrap().get(0);
	conn.execute("INSERT INTO application_scene (appid,sceneid) VALUES ($1,$2)", &[&appid, &scene]).unwrap();
	let out = format!(r#"{{"scene":{}}}"#, from_utf8(&scene.scramble()).unwrap());
	//Return with headers.
	Ok(SendFileAsWithCors{
		content_type: "application/json",
		content: out.into_bytes(),
		methods: SCENES_METHODS,
		headers: SCENES_HEADERS,
	})
}
