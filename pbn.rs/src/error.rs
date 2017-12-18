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
			},
			Error::InvalidOrigin => {
				let mut response = Response::new();
				let body = "Invalid origin\n".to_owned().into_bytes();
				response.set_status(Status::new(403, "Forbidden"));
				response.set_header(Header::new("Content-Type", "text/plain"));
				response.set_sized_body(Cursor::new(body));
				Ok(response)
			},
			Error::InvalidDimensions => {
				let mut response = Response::new();
				let body = "Invalid dimensions\n".to_owned().into_bytes();
				response.set_status(Status::new(422, "Invalid dimensions"));
				response.set_header(Header::new("Content-Type", "text/plain"));
				response.set_sized_body(Cursor::new(body));
				Ok(response)
			},
			Error::BadFormField(f) => {
				let mut response = Response::new();
				let body = format!("Bad form field {}\n", f).into_bytes();
				response.set_status(Status::new(422, "Bad form field"));
				response.set_header(Header::new("Content-Type", "text/plain"));
				response.set_sized_body(Cursor::new(body));
				Ok(response)
			},
			Error::BadGrant => {
				let mut response = Response::new();
				let body = "Bad grant\n".to_owned().into_bytes();
				response.set_status(Status::new(422, "Bad grant"));
				response.set_header(Header::new("Content-Type", "text/plain"));
				response.set_sized_body(Cursor::new(body));
				Ok(response)
			},
			Error::BadEventStream(f) => {
				let mut response = Response::new();
				let body = format!("Bad event stream {}\n", f).into_bytes();
				response.set_status(Status::new(422, "Bad event stream"));
				response.set_header(Header::new("Content-Type", "text/plain"));
				response.set_sized_body(Cursor::new(body));
				Ok(response)
			},
		}
	}
}

