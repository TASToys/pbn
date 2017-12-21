use super::{db_connect,Error, Scene};
use rocket::request::Request;
use rocket::response::{Responder, Response};
use rocket::http::{Header, Status};
use std::io::Cursor;
use std::str::from_utf8;

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
pub struct SendFileAs(pub &'static str, pub Vec<u8>);

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

pub fn scene_get_lsmv(scene: Scene) -> Result<SendFileAs, Error>
{
	let oldscene = from_utf8(&scene.scramble()).unwrap().to_owned();
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
