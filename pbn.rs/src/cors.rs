use rocket::request::Request;
use rocket::response::{Responder, Response};
use rocket::http::{Header, Status};
use std::io::Cursor;

#[derive(Debug)]
pub struct SendFileAsWithCors
{
	pub content_type: &'static str,
	pub content: Vec<u8>,
	pub methods: &'static str,
	pub headers: &'static str,
}

impl<'r> Responder<'r> for SendFileAsWithCors
{
	fn respond_to(self, request: &Request) -> Result<Response<'r>, Status>
	{
		let h = request.headers();
		let origin = h.get_one("origin").map(|x|x.to_owned());

		let mut response = Response::new();
		response.set_status(Status::new(200, "OK"));
		response.set_header(Header::new("Content-Type", self.content_type));
		if let Some(origin) = origin { if origin.starts_with("https://") && (self.methods.len() > 0 ||
			self.headers.len() > 0) {
			response.set_header(Header::new("Access-Control-Allow-Origin", origin));
			if self.methods.len() > 0 {
				response.set_header(Header::new("Access-Control-Allow-Methods", self.methods));
			}
			if self.headers.len() > 0 {
				response.set_header(Header::new("Access-Control-Allow-Headers", self.headers));
			}
		}}
		response.set_sized_body(Cursor::new(self.content));
		Ok(response)
	}
}

