#![feature(plugin, decl_macro, custom_derive, test, conservative_impl_trait)]
#![plugin(rocket_codegen)]
#![deny(unsafe_code)]

extern crate rocket;
extern crate postgres;
extern crate md5;
extern crate rand;
extern crate libc;
use postgres::{Connection, TlsMode};
use rocket::request::Form;
use rocket::response::Responder;
use rocket::Data;
use std::io::Read as IoRead;
use std::path::PathBuf;

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
	scene_get_lsmv as _scene_get_lsmv, GetBounds, ScenePostForm};

fn db_connect() -> Connection
{
	Connection::connect("pq://pbn@%2fvar%2frun%2fpostgresql%2f/pbndb", TlsMode::None).unwrap()
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
		//Scene.
		scene_options,
		scene_get,
		scene_get_lsmv,
		scene_get_png,
		//Scene edit.
		scene_edit_options,
		scene_edit_put,
		scene_edit_post,
		scene_edit_delete,
		//Scenes.
		scenes_options,
		scenes_get,
		scenes_post,
	]).launch();
}


#[cfg(test)]
mod tests;
