use rocket::request::Request;
use rocket::response::{Responder, Response};
use rocket::http::{Header, Status};
use std::iter::Cloned;
use std::slice::Iter as SliceIter;
use std::io::Cursor;
use super::add_default_headers;

#[cfg(test)]
use std::ops::Deref;
#[cfg(test)]
use std::borrow::Cow;
use std::fmt::Write as FmtWrite;
#[cfg(test)]
use self::xhtml::Tag;
#[cfg(test)]
use self::xhtml::TagAttribute as TA;

///Error returned if specified request can not be serialized due to invalid characters or nesting.
#[derive(Copy,Clone,Debug)]
pub struct XmlEncodeError(&'static str);

pub trait Attribute
{
	fn key<'a>(&'a self) -> (&'a str, &'a str);
	fn value<'a>(&'a self) -> &'a str;
}

//Empty attribute.
pub struct EmptyAttribute;

impl Attribute for EmptyAttribute
{
	fn key<'a>(&'a self) -> (&'a str, &'a str) { ("", "") }
	fn value<'a>(&'a self) -> &'a str { "" }
}

//Empty attribute iterator.
#[derive(Clone,Debug)]
pub struct EmptyAttributes;

impl Iterator for EmptyAttributes
{
	type Item = EmptyAttribute;
	fn next(&mut self) -> Option<EmptyAttribute> { None }
}

//XHTML content type.
pub const CONTENT_TYPE_XHTML: &'static str = "application/xhtml+xml; charset=utf-8";

//Main XML serializer.
#[derive(Clone,Debug)]
pub struct XmlSerializer
{
	text: String,
	nodes: Vec<String>,
	any_node: bool,
	error: Option<XmlEncodeError>,
	content_type: &'static str,
	code: u16,
}

//XML subserializer. Restricted to not be able to break above root level.
pub struct XmlSubSerializer<'a>
{
	base: &'a mut XmlSerializer,
	depth: usize,
}

fn is_valid_xml_name(tag1: &str, tag2: &str, attribute: bool) -> Result<(), XmlEncodeError>
{
	if tag1.len() == 0 && tag2.len() == 0 { return Err(XmlEncodeError("Empty name")); }
	let mut _tag = [0;6];
	for (i, c) in tag1.chars().chain(tag2.chars()).enumerate() {
		let _c = c as u32;
		if i < 6 { _tag[i] = if _c < 255 { c as u8 } else { 255 }; }
		match _c {
			58|65...90|95|97...122|0xC0...0xD6|0xD8...0xF6|0xF8...0x2FF|0x370...0x37D => (),
			0x37F...0x1FFF|0x200C|0x200D|0x2070...0x218F|0x2C00...0x2FEF|0x3001...0xD7FF => (),
			0xF900...0xFDCF|0xFDF0...0xFFFD|0x10000...0xEFFFF => (),
			45|46|48...57|0xB7|0x300...0x36F|0x203F|0x2040 if i > 0 => (),
			_ => return Err(XmlEncodeError("Bad name character"))
		};
	}
	//If tag starts with case-insensitive 'xml', that is not valid. Exception is attributes starting with
	//'xmlns:' (or being 'xmlns').
	if ((_tag[0] ^ 88) | (_tag[1] ^ 77) | (_tag[2] ^ 76)) & 0xDF == 0 {
		if (_tag.starts_with(b"xmlns:") || &_tag == b"xmlns\x00") && attribute {
			()	//Ignore.
		} else {
			return Err(XmlEncodeError("Reserved name"));	//Reserved by XML.
		};
	}
	
	Ok(())
}

fn is_raw_text(tag: &str) -> bool
{
	let mut can_fastpath = true;
	for i in tag.chars() {
		match i as u32 {
			//The characters illegal in XML (will be replaced by REPLACEMENT CHARACTER).
			0...8|11|12|14...31|0xD800...0xDFFF|0xFFFE|0xFFFF => can_fastpath = false,
			//Escaped characters.
			34|38|39|60|62 => can_fastpath = false,
			_ => ()
		};
	}
	can_fastpath
}

fn escape_text(out: &mut String, text: &str, fastpath: bool)
{
	//Fastpath => No characters to escape.
	if fastpath {
		out.push_str(text);
		return;
	}
	let mut copied_to = 0;
	for (p, c) in text.char_indices() {
		let _c = c as u32;
		let invalid = (_c >> 3) == 0 || _c == 8 || _c == 11 || _c == 12 || (_c >> 1) == 7 ||
			(_c >> 4) == 1 || (_c >> 11) == 27 || (_c >> 1) == 0x7FFF;
		//Check for character that needs escaping.
		if c == '&' || c == '<' || c == '>' || c == '\'' || c == '\"' || invalid {
			if copied_to < p { 
				out.push_str(&text[copied_to..p]);
			}
			//Exploit the fact that all special characters are 1 byte in UTF-8.
			copied_to = p + 1;
		}
		if c == '&' { out.push_str("&amp;"); }
		if c == '<' { out.push_str("&lt;"); }
		if c == '>' { out.push_str("&gt;"); }
		if c == '\'' { out.push_str("&apos;"); }
		if c == '\"' { out.push_str("&quot;"); }
		if invalid { out.push('\u{fffd}'); }
	}
	//Ok, copy the rest of the string.
	if copied_to < text.len() {
		out.push_str(&text[copied_to..]);
	}
}

fn write_attribute(out: &mut String, name: (&str, &str), text: &str, fastpath: bool)
{
	out.push(' ');
	out.push_str(name.0);	//Attribute names never need escaping.
	out.push_str(name.1);	//Attribute names never need escaping.
	out.push_str("=\"");
	escape_text(out, text, fastpath);
	out.push('\"');
}

fn check_tag<'a,'b,I:Attribute, A:Iterator<Item=I>>(name: &str, attributes: A) -> Result<u64, XmlEncodeError>
{
	let mut ret = 0;
	//Check tag name.
	is_valid_xml_name(name, "", false)?;
	//Check all tag names and values.
	for (idx, attr) in attributes.enumerate()  {
		let key = attr.key();
		is_valid_xml_name(key.0, key.1, true)?;
		if is_raw_text(attr.value()) && idx < 64 { ret |= 1 << idx; }
	}
	Ok(ret)
}

pub trait XmlTag
{
	type Attr: Attribute;
	type AttrIter: Iterator<Item=<Self as XmlTag>::Attr>+Clone;
	fn tag<'a>(&'a self) -> &'a str;
	fn namespace<'a>(&'a self) -> Option<&'a str>;
	fn attributes(&self) -> <Self as XmlTag>::AttrIter;
}

impl<'x> Attribute for (&'x str, &'x str)
{
	fn key<'a>(&'a self) -> (&'a str, &'a str) { (self.0, "") }
	fn value<'a>(&'a self) -> &'a str { self.1 }
}

impl<'x> XmlTag for (&'x str, &'x [(&'x str, &'x str)])
{
	type Attr = (&'x str, &'x str);
	type AttrIter = Cloned<SliceIter<'x, <Self as XmlTag>::Attr>>;
	fn tag<'a>(&'a self) -> &'a str { self.0 }
	fn namespace<'a>(&'a self) -> Option<&'a str> { None }
	fn attributes(&self) -> <Self as XmlTag>::AttrIter
	{
		self.1.iter().cloned()
	}
}

impl<'x> XmlTag for &'x str
{
	type Attr = EmptyAttribute;
	type AttrIter = EmptyAttributes;
	fn tag<'a>(&'a self) -> &'a str { self }
	fn namespace<'a>(&'a self) -> Option<&'a str> { None }
	fn attributes(&self) -> EmptyAttributes { EmptyAttributes }
}

pub trait XmlOutputStream
{
	//Open a new tag.
	fn open<XTag:XmlTag>(&mut self, tag: XTag);
	//Open and immediately close a new tag.
	fn impulse<XTag:XmlTag>(&mut self, tag: XTag);
	//Add text.
	fn text(&mut self, text: &str);
	//Close most recent unclosed tag.
	fn close(&mut self);
	//Run closure on subserializer.
	fn subserializer<'a, R:Sized, F>(&mut self, cb: F) -> R where F: FnMut(&mut XmlSubSerializer) -> R;
	//Open a new tag in subserializer.
	fn tag_fn<'a,R,XTag:XmlTag,F>(&mut self, tag: XTag, cb: F) -> R where F: FnMut(&mut XmlSubSerializer) -> R;
}

impl XmlOutputStream for XmlSerializer
{
	fn open<XTag:XmlTag>(&mut self, tag: XTag)
	{
		if self.error.is_some() { return; }
		let namespace = tag.namespace();
		let name = tag.tag();
		let attributes = tag.attributes();
		//Only one root allowed.
		if self.nodes.len() == 0 && self.any_node {
			self.error = Some(XmlEncodeError("Root already done"));
			return;
		}
		let ret = match check_tag(name, attributes.clone()) { Ok(x) => x, Err(x) => {
			self.error = Some(x);
			return;
		}};
		if let Some(ns) = namespace {
			//Check namespace.
			match is_valid_xml_name(ns, "", false) { Ok(x) => x, Err(x) => {
				self.error = Some(x);
				return;
			}};
		}
		self.text.push('<');
		if let Some(ns) = namespace {
			self.text.push_str(ns);
			self.text.push(':');
		};
		self.text.push_str(name);		//Tag names never need escaping.
		for (idx, attr) in attributes.enumerate()  {
			write_attribute(&mut self.text, attr.key(), attr.value(), idx < 64 && (ret >> idx) & 1 != 0)
		}
		self.text.push('>');
		if let Some(ns) = namespace {
			self.nodes.push(format!("{}:{}", ns, name));
		} else {
			self.nodes.push(name.to_owned());
		}
		self.any_node = true;
	}
	fn impulse<XTag:XmlTag>(&mut self, tag: XTag)
	{
		if self.error.is_some() { return; }
		let namespace = tag.namespace();
		let name = tag.tag();
		let attributes = tag.attributes();
		//Only one root allowed.
		if self.nodes.len() == 0 && self.any_node {
			self.error = Some(XmlEncodeError("Root already done"));
			return;
		}
		let ret = match check_tag(name, attributes.clone()) { Ok(x) => x, Err(x) => {
			self.error = Some(x);
			return;
		}};
		if let Some(ns) = namespace {
			//Check namespace.
			match is_valid_xml_name(ns, "", false) { Ok(x) => x, Err(x) => {
				self.error = Some(x);
				return;
			}};
		}
		self.text.push('<');
		if let Some(ns) = namespace {
			self.text.push_str(ns);
			self.text.push(':');
		};
		self.text.push_str(name);		//Tag names never need escaping.
		for (idx, attr) in attributes.enumerate()  {
			write_attribute(&mut self.text, attr.key(), attr.value(), idx < 64 && (ret >> idx) & 1 != 0)
		}
		self.text.push_str("/>");
		//This tag is immediately closed, so not added to nodes.
		self.any_node = true;
	}
	fn text(&mut self, text: &str)
	{
		if self.error.is_some() { return; }
		if self.nodes.len() == 0 {
			//Not allowed at root.
			self.error = Some(XmlEncodeError("Need root tag"));
			return;
		}
		let fastpath = is_raw_text(text);
		escape_text(&mut self.text, text, fastpath);
	}
	fn close(&mut self)
	{
		if self.error.is_some() { return; }
		let last = match self.nodes.pop() { Some(x) => x, None => {
			self.error = Some(XmlEncodeError("No more open tags"));
			return;
		}};
		//open() checked that the tag is valid.
		write!(self.text, "</{}>", last).unwrap();	//This should never fail.
		//Ok. No need to set any_node, because it must already been set.
	}
	fn subserializer<'a, R:Sized, F>(&mut self, mut cb: F) -> R where F: FnMut(&mut XmlSubSerializer) -> R
	{
		//Run the subserializer even in failed state.
		if self.nodes.len() == 0 {
			//Not allowed at root.
			self.error = Some(XmlEncodeError("Subserializer not allowed at root"));
		}
		let mut x = XmlSubSerializer{base: self, depth:0};
		let r = cb(&mut x);
		for _ in 0..x.depth { x.base.close(); }
		r
	}
	fn tag_fn<'a,R,XTag:XmlTag,F>(&mut self, tag: XTag, mut cb: F) -> R where F: FnMut(
		&mut XmlSubSerializer) -> R
	{
		//These all check for error status.
		self.open(tag);
		let mut x = XmlSubSerializer{base: self, depth:0};
		let r = cb(&mut x);
		for _ in 0..x.depth { x.base.close(); }
		x.base.close();		
		r
	}
}

impl<'r> Responder<'r> for XmlSerializer
{
	fn respond_to(self, _request: &Request) -> Result<Response<'r>, Status>
	{
		let mut response = Response::new();
		let code = self.code;
		let ct = self.content_type;
		let body = match self.into_inner() {
			Ok(x) => {
				let body: Vec<u8> = x.into_bytes();
				response.set_status(Status::new(code, "OK"));
				response.set_header(Header::new("Content-Type", ct));
				body
			},
			Err(_) => {
				let body: Vec<u8> = (&b"Bad XHTML to output"[..]).to_owned();
				response.set_status(Status::new(500, "Bad XHTML"));
				response.set_header(Header::new("Content-Type", "text/plain"));
				body
			}
		};
		add_default_headers(&mut response);
		response.set_sized_body(Cursor::new(body));
		Ok(response)
	}
}

impl XmlSerializer
{
	///New XML encoder.
	pub fn new() -> XmlSerializer
	{
		let mut text = String::new();
		//Push initial text.
		text.push_str("<?xml version=\"1.0\"?>\n");
		XmlSerializer{
			text: text,
			nodes: Vec::new(),
			any_node: false,
			error: None,
			content_type: "text/xml; charset=utf8",
			code: 200,
		}
	}
	///Unwrap and return the text.
	pub fn into_inner(self) -> Result<String, XmlEncodeError>
	{
		//The write must be complete and error-free.
		if let Some(err) = self.error { return Err(err); }
		if self.nodes.len() > 0 || !self.any_node { return Err(XmlEncodeError("Unclosed tags")); }
		Ok(self.text)
	}
	///Set content type.
	pub fn set_content_type(&mut self, ct: &'static str)
	{
		self.content_type = ct;
	}
	///Set response code.
	pub fn set_response_code(&mut self, code: u16)
	{
		self.code = code;
	}
}

impl<'x> XmlOutputStream for XmlSubSerializer<'x>
{
	fn open<XTag:XmlTag>(&mut self, tag: XTag)
	{
		self.base.open(tag);
		self.depth += 1;
	}
	fn impulse<XTag:XmlTag>(&mut self, tag: XTag)
	{
		self.base.impulse(tag)
	}
	fn text(&mut self, text: &str)
	{
		self.base.text(text)
	}
	fn close(&mut self)
	{
		if self.depth == 0 { return; }
		self.depth -= 1;
		self.base.close()
	}
	fn subserializer<'a, R:Sized, F>(&mut self, mut cb: F) -> R where F: FnMut(&mut XmlSubSerializer) -> R
	{
		let mut x = XmlSubSerializer{base: self.base, depth:0};
		let r = cb(&mut x);
		for _ in 0..x.depth { x.base.close(); }
		r
	}
	fn tag_fn<'a,R,XTag:XmlTag,F>(&mut self, tag: XTag, mut cb: F) -> R where F: FnMut(
		&mut XmlSubSerializer) -> R
	{
		self.open(tag);
		let r = {
			let mut x = XmlSubSerializer{base: self.base, depth:0};
			let r = cb(&mut x);
			for _ in 0..x.depth { x.base.close(); }
			r
		};
		self.close();
		r
	}
}

#[macro_use]
pub mod xhtml
{
	use std::borrow::Cow;
	use std::mem::replace;
	use std::ops::Deref;
	use super::{Attribute, XmlTag};

	#[derive(Clone,Debug)]
	pub struct HtmlTagAttributes(bool);
	
	pub struct HtmlTagAttributeNs;

	impl Attribute for HtmlTagAttributeNs
	{
		fn key<'a>(&'a self) -> (&'a str, &'a str) { ("xmlns:", "html") }
		fn value<'a>(&'a self) -> &'a str { "http://www.w3.org/1999/xhtml" }
	}

	impl Iterator for HtmlTagAttributes
	{
		type Item = HtmlTagAttributeNs;
		fn next(&mut self) -> Option<HtmlTagAttributeNs> {
			if !replace(&mut self.0, true) { Some(HtmlTagAttributeNs) } else { None }
		}
	}
	
	pub struct Html;
	impl XmlTag for Html
	{
		type Attr = HtmlTagAttributeNs;
		type AttrIter = HtmlTagAttributes;
		fn tag<'a>(&'a self) -> &'a str { "html" }
		fn namespace<'a>(&'a self) -> Option<&'a str> { Some("html") }
		fn attributes(&self) -> HtmlTagAttributes { HtmlTagAttributes(false) }
	}

	pub struct XhtmlAttribute(&'static str, &'static str, Cow<'static, str>);

	impl Attribute for XhtmlAttribute
	{
		fn key<'a>(&'a self) -> (&'a str, &'a str) { (self.0, self.1) }
		fn value<'a>(&'a self) -> &'a str { self.2.deref() }
	}

	pub trait StaticStrCow { fn to(self) -> Cow<'static, str>; }
	impl StaticStrCow for &'static str { fn to(self) -> Cow<'static, str> { Cow::Borrowed(self) } }
	impl StaticStrCow for String { fn to(self) -> Cow<'static, str> { Cow::Owned(self) } }
	impl StaticStrCow for Cow<'static, str> { fn to(self) -> Cow<'static, str> { self } }

	include!(concat!(env!("OUT_DIR"), "/xhtml.rs"));

	#[derive(Clone)]
	pub struct Tag<'x>
	{
		tag: &'static str,
		//attributes: Rc<Vec<TagAttribute>>
		attributes: &'x [TagAttribute]
	}

	impl<'x> Tag<'x>
	{
		//Create a new tag.
		pub fn new<'y>(tag: &'static str, attributes: &'y [TagAttribute]) -> Tag<'y>
		{
			Tag{tag: tag, attributes: attributes}
		}
	}


	impl<'x> XmlTag for Tag<'x>
	{
		type Attr = XhtmlAttribute;
		type AttrIter = TagAttributes<'x>;
		fn tag<'a>(&'a self) -> &'a str { self.tag }
		fn namespace<'a>(&'a self) -> Option<&'a str> { Some("html") }
		fn attributes(&self) -> TagAttributes<'x> { TagAttributes(self.attributes, 0) }
	}
}


#[test]
fn strange_tag()
{
	let mut x = XmlSerializer::new();
	x.open("<");
	x.close();
	x.into_inner().unwrap_err();
}

#[test]
fn strange_attribute()
{
	let mut x = XmlSerializer::new();
	x.open(("foo", &[("<","foo")][..]));
	x.close();
	x.into_inner().unwrap_err();
}

#[test]
fn strange_value()
{
	let mut x = XmlSerializer::new();
	x.open(("foo", &[("bar","\u{8}")][..]));
	x.close();
	assert_eq!(x.into_inner().unwrap().deref(), "<?xml version=\"1.0\"?>\n\
		<foo bar=\"\u{fffd}\"></foo>");
}

#[test]
fn reserved_attribute()
{
	let mut x = XmlSerializer::new();
	x.open(("foo", &[("xmlfoo","foo")][..]));
	x.close();
	x.into_inner().unwrap_err();
	let mut x = XmlSerializer::new();
	x.open(("foo", &[("XmLfoo","foo")][..]));
	x.close();
	x.into_inner().unwrap_err();
}

#[test]
fn reserved_tag()
{
	let mut x = XmlSerializer::new();
	x.open("xmlns");
	x.into_inner().unwrap_err();
	let mut x = XmlSerializer::new();
	x.open("XmLns");
	x.into_inner().unwrap_err();
}



#[test]
fn basic_xhtml()
{
	let mut x = XmlSerializer::new();
	x.open(self::xhtml::Html);
	x.tag_fn(tag!(head), |x|{;
		x.tag_fn(tag!(title), |x|{
			x.text("foobar");
		});
	});
	x.tag_fn(tag!(body), |x|{
		x.open(tag!(p));
		x.text("qux");
		x.impulse(tag!(p));
		x.open(tag!(p attr!(id="foo"), attr!(class="bar")));
		x.close();
		x.impulse(Tag::new("zzz", &[attr!(data("src")="foo")]));
		x.impulse(Tag::new("www", &[TA::Data("zot", Cow::Borrowed("bar"))]));
		x.impulse(("yyy", &[("data-foo","foo<>&\"zot\'bar")][..]));
		x.impulse(Tag::new("www2", &[attr!(id="zot"), TA::None, attr!(class="foobar")]));
		x.text("zot");
	});
	x.close();
	println!("{}", x.into_inner().unwrap());
	assert!(false);
}
