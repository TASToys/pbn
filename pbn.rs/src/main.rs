#![feature(plugin, decl_macro, custom_derive, test, conservative_impl_trait)]
#![plugin(rocket_codegen)]
#![deny(unsafe_code)]

extern crate rocket;
extern crate postgres;
extern crate md5;
extern crate rand;
extern crate libc;
extern crate time;
use postgres::{Connection, TlsMode};
use rocket::request::Form;
use rocket::response::{Response, Responder};
use rocket::http::Header;
use rocket::Data;
use std::char::from_u32;
use std::fs::File;
use std::path::Path;
use std::io::Read as IoRead;
use std::fmt::Write as FmtWrite;
use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;
use std::ffi::CStr;
use libc::{getuid, getpwuid};

#[macro_use]
pub mod xml;
use xml::{XmlSerializer, XmlOutputStream, CONTENT_TYPE_XHTML};
use xml::xhtml::Html;

mod json;
mod lsmv;
mod error;
use error::Error;
mod authentication;
use authentication::AuthenticationInfo;
mod scene;
use scene::Scene;
mod mmapstate;
mod png;
mod staticfile;
use staticfile::serve_file;
mod cors;
mod scenes_endpoint;
use scenes_endpoint::{scenes_get as _scenes_get, scenes_options as _scenes_options, scenes_post as _scenes_post,
	SceneInfo};
mod scene_endpoint;
use scene_endpoint::{scene_get as _scene_get, scene_options as _scene_options,
	scene_edit_delete as _scene_edit_delete, scene_edit_options as _scene_edit_options,
	scene_edit_post as _scene_edit_post, scene_edit_put as _scene_edit_put, scene_get_png as _scene_get_png,
	scene_get_lsmv as _scene_get_lsmv, GetBounds, ScenePostForm, scene_config_options as _scene_config_options,
	scene_config_get as _scene_config_get, scene_config_put as _scene_config_put,
	scene_describe as _scene_describe, Xss};


fn db_connect() -> Connection
{
	Connection::connect(get_db_url(), TlsMode::None).unwrap()
}


//Pathbuf as parameter does not accept path transversal.
#[get("/static/<file..>")]
fn serve_static_files(file: PathBuf) -> impl Responder<'static>
{
	serve_file(file)
}

#[options("/scenes")]
fn scenes_options() -> impl Responder<'static>
{
	_scenes_options()
}

#[get("/scenes")]
fn scenes_get(auth: AuthenticationInfo) -> impl Responder<'static>
{
	_scenes_get(auth)
}

#[post("/scenes", data = "<upload>")]
fn scenes_post(auth: AuthenticationInfo, upload: Form<SceneInfo>) -> impl Responder<'static>
{
	_scenes_post(auth, upload)
}

#[options("/scenes/<scene>")]
fn scene_options(scene: Option<Scene>) -> Result<impl Responder<'static>, Error>
{
	let scene = scene.ok_or(Error::SceneNotFound)?;
	_scene_options(scene)
}

#[get("/scenes/<scene>")]
fn scene_get(scene: Option<Scene>, range: GetBounds) -> Result<impl Responder<'static>, Error>
{
	let scene = scene.ok_or(Error::SceneNotFound)?;
	_scene_get(scene, range)
}

#[options("/scenes/<scene>/edit")]
fn scene_edit_options(scene: Option<Scene>) -> Result<impl Responder<'static>, Error>
{
	scene.ok_or(Error::SceneNotFound)?;
	_scene_edit_options()
}

#[put("/scenes/<scene>/edit", data = "<upload>")]
fn scene_edit_put(scene: Option<Scene>, auth: AuthenticationInfo, upload: Data) ->
	Result<impl Responder<'static>, Error>
{
	match scene {
		Some(scene) => _scene_edit_put(scene, auth, upload),
		None => Err(sink_put(upload, Error::SceneNotFound))
	}
}

#[post("/scenes/<scene>/edit", data = "<upload>")]
fn scene_edit_post(scene: Option<Scene>, auth: AuthenticationInfo, upload: Form<ScenePostForm>) ->
	Result<impl Responder<'static>, Error>
{
	let scene = scene.ok_or(Error::SceneNotFound)?;
	_scene_edit_post(scene, auth, upload)
}

#[delete("/scenes/<scene>/edit", data = "<upload>")]
fn scene_edit_delete(scene: Option<Scene>, auth: AuthenticationInfo, upload: Data) ->
	Result<impl Responder<'static>, Error>
{
	sink_put(upload, ());
	let scene = scene.ok_or(Error::SceneNotFound)?;
	_scene_edit_delete(scene, auth)
}

#[get("/scenes/<scene>/png")]
fn scene_get_png(scene: Option<Scene>) -> Result<impl Responder<'static>, Error>
{
	let scene = scene.ok_or(Error::SceneNotFound)?;
	_scene_get_png(scene)
}

#[get("/scenes/<scene>/lsmv")]
fn scene_get_lsmv(scene: Option<Scene>) -> Result<impl Responder<'static>, Error>
{
	let scene = scene.ok_or(Error::SceneNotFound)?;
	_scene_get_lsmv(scene)
}

#[options("/scenes/<scene>/config")]
fn scene_config_options(scene: Option<Scene>) -> Result<impl Responder<'static>, Error>
{
	scene.ok_or(Error::SceneNotFound)?;
	_scene_config_options()
}

#[get("/scenes/<scene>/config")]
fn scene_config_get(scene: Option<Scene>) -> Result<impl Responder<'static>, Error>
{
	let scene = scene.ok_or(Error::SceneNotFound)?;
	_scene_config_get(scene)
}

#[put("/scenes/<scene>/config", data="<upload>")]
fn scene_config_put(scene: Option<Scene>, auth: AuthenticationInfo, upload: Data) ->
	Result<impl Responder<'static>, Error>
{
	match scene {
		Some(scene) => _scene_config_put(scene, auth, upload),
		None => Err(sink_put(upload, Error::SceneNotFound))
	}
}

#[get("/scenes/<scene>/describe")]
fn scene_describe(scene: Option<Scene>, xss: Xss) -> Result<impl Responder<'static>, Error>
{
	let scene = scene.ok_or(Error::SceneNotFound)?;
	_scene_describe(scene, xss)
}

//Pathbuf as parameter does not accept path transversal.
#[get("/app/<file..>")]
fn serve_app(file: PathBuf) -> Result<impl Responder<'static>, Error>
{
	eprintln!("App request for '{}'", file.display());
	let rpath = root_path();
	let bpath = Path::new(&format!("{}/static/apps/", rpath)).join(&file);
	let jpath = bpath.join("main.js");
	let cpath = bpath.join("main.css");
	if !jpath.is_file() { return Err(Error::NotFound); }
	let mut xml = XmlSerializer::new();
	xml.set_content_type(CONTENT_TYPE_XHTML);
	xml.tag_fn(Html, |xml|{
		xml.tag_fn(tag!(head), |xml|{
			if cpath.is_file() {
				xml.impulse(tag!(link attr!(rel="stylesheet"), attr!(type="text/css"),
					attr!(href=format!("/{}", cpath.strip_prefix(&rpath).unwrap().display()))));
			}
			xml.impulse(tag!(script attr!(type="text/javascript"), attr!(src=format!("/{}", jpath.
				strip_prefix(&rpath).unwrap().display()))));
			xml.tag_fn(tag!(title), |xml|{
				xml.text(&format!("{}", file.display()));
			});
		});
		xml.tag_fn(tag!(body), |_|{
		});
	});
	Ok(xml)
}

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

fn main() {
	rocket::ignite().mount("/", routes![
		//Static files,
		serve_static_files,
		//Applications.
		serve_app,
		//Scene.
		scene_options,
		scene_get,
		scene_get_lsmv,
		scene_get_png,
		scene_describe,
		//Scene edit.
		scene_edit_options,
		scene_edit_put,
		scene_edit_post,
		scene_edit_delete,
		//Scene config.
		scene_config_options,
		scene_config_get,
		scene_config_put,
		//Scenes.
		scenes_options,
		scenes_get,
		scenes_post,
	]).launch();
}

struct Config
{
	db_user: String,
	db_path: String,
	db_name: String,
	scene_key: Vec<u8>,
	rootpath: String,
}

impl Config
{
	#[allow(unsafe_code)]
	fn new() -> Config
	{
		let pwd = unsafe{getpwuid(getuid())};
		if pwd.is_null() { panic!("No home directory for current user in user database"); }
		if unsafe{(*pwd).pw_uid} == 0 { panic!("Running as root??? Are you insane???"); }
		let root = match unsafe{CStr::from_ptr((*pwd).pw_dir)}.to_str() { Ok(x) => x.to_owned(), Err(_) => {
			panic!("User home directory in user database is not valid UTF-8");
		}};
		let cfilename = format!("{}/pbn.conf", root);
		let mut content = String::new();
		match File::open(&cfilename).and_then(|mut f|f.read_to_string(&mut content)) {
			Ok(_) => (),
			Err(x) => panic!("Failed to read the config file {}: {}", cfilename, x)
		};
		let mut i = content.lines();
		let duser = i.next().unwrap().to_owned();
		let dpath = i.next().unwrap().to_owned();
		let dname = i.next().unwrap().to_owned();
		let key = i.next().unwrap().as_bytes().to_owned();
		Config{
			db_user: duser,
			db_path: dpath,
			db_name: dname,
			scene_key: key,
			rootpath: root,
		}
	}
	fn get<F>(mut cb: F) where F: FnMut(&Config)
	{
		thread_local!(static CONFIG_FOR_THREAD: Rc<RefCell<Config>> = {Rc::new(RefCell::new(
			Config::new()))});
		CONFIG_FOR_THREAD.with(|y|cb(&y.borrow()));
	}
}

fn get_db_url() -> String
{
	let mut user = String::new();
	let mut path = String::new();
	let mut name = String::new();
	Config::get(|c|{
		user = c.db_user.clone();
		path = c.db_path.clone();
		name = c.db_name.clone();
	});
	//Escape all non alphanumerics from path.
	let mut path2 = String::new();
	for i in path.as_bytes().iter() {
		if (*i >= 48 && *i <= 57) || (*i >= 65 && *i <= 90) || (*i >= 97 && *i <= 122) {
			path2.push(from_u32(*i as u32).unwrap());
		} else {
			write!(path2, "%{:02x}", i).unwrap();
		}
	}
	format!("pq://{}@{}/{}", user, path2, name)
}

fn get_scene_key() -> Vec<u8>
{
	let mut key = Vec::new();
	Config::get(|c|{
		key = c.scene_key.clone();
	});
	key
}

fn root_path() -> String
{
	let mut path = String::new();
	Config::get(|c|{
		path = c.rootpath.clone();
	});
	path
}

fn add_default_headers(response: &mut Response)
{
	response.set_header(Header::new("X-XSS-Protection", "0"));
	response.set_header(Header::new("X-Content-Type-Options", "nosniff"));
	response.set_header(Header::new("Content-Security-Policy",
		"default-src https://*; script-src 'self'; object-src 'none'; style-src 'self'; \
		font-src 'self';"));
	response.set_header(Header::new("Referrer-Policy", "no-referrer"));

}

#[cfg(test)]
mod tests;
