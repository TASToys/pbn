use std::fmt::Write as FmtWrite;
use std::io::Write as IoWrite;
use std::fs::File;
use std::path::Path;
use std::env::var;

const ATTRIBUTES: [(&'static str, &'static str);6] = [
	("id", "Id"),
	("class", "Clazz"),
	("src", "Src"),
	("href", "Href"),
	("type", "Type"),
	("rel", "Rel"),
];

const TAGS: [&'static str; 8] = [
	"p",
	"span",
	"div",
	"head",
	"title",
	"body",
	"link",
	"script",
];

fn main()
{
	let out_dir = var("OUT_DIR").unwrap();
	let dest_path = Path::new(&out_dir).join("xhtml.rs");
	let mut f = File::create(&dest_path).unwrap();

	let mut tag_attribute_defs = String::new();
	let mut tag_attribute_next = String::new();
	let mut tag_attribute_raw_defs = String::new();
	let mut tag_attribute_raw_to = String::new();
	let mut attr_defs = String::new();
	let mut tags_rules = String::new();

		
	for i in TAGS.iter() {
		writeln!(tags_rules, "\t\t({x}) => {{ $crate::xml::xhtml::Tag::new(\"{x}\", &[]) }};", x=i).unwrap();
		writeln!(tags_rules, "\t\t({x} $($args:expr),*) => {{ $crate::xml::xhtml::Tag::new(\"{x}\", \
			&[$($args),*]) }};", x=i).unwrap();
	}
	for i in ATTRIBUTES.iter() {
		writeln!(tag_attribute_raw_defs, "\t\t{},", i.1).unwrap();
		writeln!(tag_attribute_raw_to, "\t\t\t\tTagAttributeRaw::{x} => TagAttribute::{x}(value.to()),",
			x=i.1).unwrap();
		writeln!(tag_attribute_defs, "\t\t{}(Cow<'static, str>),", i.1).unwrap();
		writeln!(attr_defs, "\t\t({}=$v:expr) => {{ $crate::xml::xhtml::TagAttributeRaw::{}.to($v) }};", i.0,
			i.1).unwrap();
		writeln!(tag_attribute_next, "\t\t\t\t&TagAttribute::{}(ref x) => Some(XhtmlAttribute(\"\", \
			\"{}\", x.clone())),", i.1, i.0).unwrap();
	}

	writeln!(f, "\
{b}	#[derive(Clone)]
	pub enum TagAttributeRaw
	{{
		Data(&'static str),
		{tag_attribute_raw_defs}\
{b}	}}

	impl TagAttributeRaw
	{{
		pub fn to<S:StaticStrCow>(self, value: S) -> TagAttribute
		{{
			match self {{
				TagAttributeRaw::Data(x) => TagAttribute::Data(x, value.to()),
				{tag_attribute_raw_to}\
{b}			}}
		}}
	}}
	
	#[derive(Clone)]
	pub enum TagAttribute
	{{
		None,
		Data(&'static str, Cow<'static, str>),
		{tag_attribute_defs}\
{b}	}}

	#[macro_export]
	macro_rules! attr {{
		(data($k:expr)=$v:expr) => {{ $crate::xml::xhtml::TagAttributeRaw::Data($k).to($v) }};
		{attr_defs}\
{b}	}}

	#[macro_export]
	macro_rules! tag {{
		{tags_rules}\
{b}	}}

	#[derive(Clone)]
	pub struct TagAttributes<'x>(&'x [TagAttribute], usize);

	impl<'x> Iterator for TagAttributes<'x>
	{{
		type Item = XhtmlAttribute;
		fn next(&mut self) -> Option<XhtmlAttribute> {{
			let g = {{
				//Skip any None attributes.
				let mut g = &TagAttribute::None;
				while match g {{ &TagAttribute::None => true, _ => false }} {{
					g = match self.0.get(self.1) {{ Some(x) => x, None => return None }};
					self.1 += 1;
				}}
				g
			}};
			match g {{
				&TagAttribute::None => unreachable!(),	//Should not happen.
				&TagAttribute::Data(y, ref x) => Some(XhtmlAttribute(\"data-\", y, x.clone())),
				{tag_attribute_next}\
{b}			}}
		}}
	}}
", b="", tag_attribute_defs=tag_attribute_defs, tag_attribute_raw_defs=tag_attribute_raw_defs,
	tag_attribute_raw_to=tag_attribute_raw_to, attr_defs=attr_defs, tag_attribute_next=tag_attribute_next,
	tags_rules=tags_rules).unwrap();
}
