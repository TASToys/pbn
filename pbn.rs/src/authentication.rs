use super::Scene;
use postgres::Connection;
use std::str::from_utf8;
use std::time::{SystemTime, UNIX_EPOCH};
use rocket::request::{FromRequest, Request};
use rocket::outcome::Outcome;
use rocket::http::Status;
use rand::os::OsRng;
use rand::Rng;


pub struct AuthenticationInfo
{
	origin: Option<String>,
	overridden: bool,
	key: Option<String>,
}

impl AuthenticationInfo
{
	pub fn get_origin(&self, conn: &mut Connection, privileged: bool) -> Result<i32, ()>
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
	pub fn check_write(&self, conn: &mut Connection, scene: Scene) -> Result<i32, bool>
	{
		let appid = self.get_origin(conn, true).map_err(|_|true)?;
		let has_access: i64 = conn.query("SELECT COUNT(sceneid) FROM application_scene WHERE \
			appid=$1 AND sceneid=$2", &[&appid,&scene]).unwrap().iter().next().unwrap().get(0);
		if has_access == 0 {
			//Check if this exists at all.
			let exists: i64 = conn.query("SELECT COUNT(sceneid) FROM scenes WHERE sceneid=$1",
				&[&scene]).unwrap().iter().next().unwrap().get(0);
			Err(exists > 0)
		} else {
			Ok(appid)
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

//Returns sub-origin and apikey.
pub fn create_local_token(conn: &mut Connection, username: &str, expiry: u64) -> (String, String)
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
