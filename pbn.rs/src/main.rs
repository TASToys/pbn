#![feature(plugin, decl_macro)]
#![plugin(rocket_codegen)]

extern crate rocket;
extern crate postgres;
extern crate md5;
use postgres::{Connection, TlsMode};
use rocket::request::Request;
use rocket::response::{Responder, Response};
use rocket::http::{Header, Status};
use std::io::Cursor;

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

#[derive(Debug)]
enum Error
{
	SceneNotFound
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
/*
fn main()
{
	for i in 0..(1<<30) {
		let j = permute(i);
		let k = unpermute(&j);
		println!("\x1B[1A {} -> {} -> {}            ", i, from_utf8(&j).unwrap(), k);
		
		sleep(Duration::from_secs(1));
	}
}
*/

#[get("/scenes")]
fn scenes_get() -> String
{
	let _conn = db_connect();
	String::new()
}

#[get("/scenes/<scene>")]
fn scene_get(scene: String) -> Result<String, Error>
{
	let scene = unpermute(scene.as_bytes());
	let conn = db_connect();
	let (w, h) = if let Some(row) = conn.query("SELECT width, height FROM scenes WHERE sceneid=$1", &[&scene]).unwrap().iter().next() {
		let w: i32 = row.get(0);
		let h: i32 = row.get(1);
		(w, h)
	} else {
		return Err(Error::SceneNotFound);
	};
	Ok(format!("width: {}, height: {}\n", w, h))
}

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
		scene_get,
		scene_get_lsmv,
		scenes_get,
	]).launch();
}
