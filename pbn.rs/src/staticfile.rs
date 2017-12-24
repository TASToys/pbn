use rocket::request::Request;
use rocket::response::{Responder, Response, NamedFile};
use rocket::http::{Status, ContentType};
use std::borrow::Cow;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::marker::PhantomData;

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
pub fn serve_file(file: PathBuf) -> impl Responder<'static>
{
	NamedFile::open(Path::new("/home/pbn/static/").join(&file)).ok().map(|f|MoreContentTypeGuessing::new(f,
		file))
}
