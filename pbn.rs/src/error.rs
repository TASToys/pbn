use rocket::request::Request;
use rocket::response::{Responder, Response};
use rocket::http::{Header, Status};
use std::io::Cursor;

#[derive(Debug)]
pub enum Error
{
	SceneNotFound,
	InvalidOrigin,
	InvalidDimensions,
	BadFormField(String),
	BadGrant,
	BadEventStream(String),
	MethodNotSupported,
	NotFound,
}

trait StringTrait { fn get(self) -> String; }
impl<'a> StringTrait for &'a str { fn get(self) -> String { self.to_owned() } }
impl<'a> StringTrait for String { fn get(self) -> String { self } }

fn make_response<B:StringTrait>(response: &mut Response, code: u16, statstr: &'static str, body: B)
{
	let body = body.get().into_bytes();
	response.set_status(Status::new(code, statstr));
	response.set_header(Header::new("Content-Type", "text/plain"));
	response.set_sized_body(Cursor::new(body));
}

impl<'r> Responder<'r> for Error
{
	fn respond_to(self, _request: &Request) -> Result<Response<'r>, Status>
	{
		let mut response = Response::new();
		match self {
			Error::SceneNotFound => make_response(&mut response, 404, "Scene not found",
				"Scene not found\n"),
			Error::NotFound => make_response(&mut response, 404, "Not found",
				"Not found\n"),
			Error::MethodNotSupported => make_response(&mut response, 405, "Method not supported",
				"Method not supported\n"),
			Error::InvalidOrigin => make_response(&mut response, 403, "Forbidden", "Invalid origin\n"),
			Error::InvalidDimensions => make_response(&mut response, 422, "Invalid dimensions",
				"Invalid dimensions\n"),
			Error::BadFormField(f) => make_response(&mut response, 422, "Bad form field", format!(
				"Bad form field: {}\n", f)),
			Error::BadGrant => make_response(&mut response, 422, "Bad grant", "Bad grant\n"),
			Error::BadEventStream(f) => make_response(&mut response, 422, "Bad event stream", format!(
				"Bad event stream {}\n", f)),
		}
		Ok(response)
	}
}

