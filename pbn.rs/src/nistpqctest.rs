use ::{db_connect};
use ::xml::{XmlSerializer, XmlOutputStream};
use ::xml::xhtml::Html;
use ::xml::CONTENT_TYPE_XHTML;
use std::borrow::ToOwned;

fn emit_table(xml: &mut XmlSerializer, classid: i32)
{
	xml.tag_fn(tag!(table), |xml|{
		xml.tag_fn(tag!(tr), |xml|{
			xml.tag_fn(tag!(th), |xml|{xml.text("Name")});
			xml.tag_fn(tag!(th), |xml|{xml.text("Type")});
			xml.tag_fn(tag!(th), |xml|{xml.text("Problem")});
			xml.tag_fn(tag!(th), |xml|{xml.text("Level")});
			xml.tag_fn(tag!(th), |xml|{xml.text("Pfail")});
			xml.tag_fn(tag!(th), |xml|{xml.text("Sksize")});
			xml.tag_fn(tag!(th), |xml|{xml.text("Pksize")});
			xml.tag_fn(tag!(th), |xml|{xml.text("Ctsize")});
			xml.tag_fn(tag!(th), |xml|{xml.text("Totalsize")});
			xml.tag_fn(tag!(th), |xml|{xml.text("Status")});
		});
		for row in conn.query("SELECT type, name, level, sksize, pksize, ctsize, status, \
			pfail, problem FROM nistpqc WHERE type=$1 ORDER BY name, level, pfail", &[&classid]).
			unwrap().iter() {
			let xtype: i32 = row.get(0);
			let name: String = row.get(1);
			let level: i32 = row.get(2);
			let sksize: Option<i32> = row.get(3);
			let pksize: Option<i32> = row.get(4);
			let ctsize: Option<i32> = row.get(5);
			let status: String = row.get(6);
			let problem: String = row.get(8);
			let pfail: Option<i32> = row.get(7);
			let tsize = if let (Some(pk),Some(ct)) = (pksize,ctsize) {
				Some(pk + ct)
			} else {
				None
			};
			let xtype = match xtype {
				1 => "Key exchange",
				2 => "Encryption",
				3 => "Sigature",
				_ => "?",
			};
			let sksize = sksize.map(|x|format!("{}",x)).unwrap_or_else(||"?".to_owned());
			let pksize = pksize.map(|x|format!("{}",x)).unwrap_or_else(||"?".to_owned());
			let ctsize = ctsize.map(|x|format!("{}",x)).unwrap_or_else(||"?".to_owned());
			let tsize = tsize.map(|x|format!("{}",x)).unwrap_or_else(||"?".to_owned());
			let pfail = pfail.map(|x|format!("2^-{}",x)).unwrap_or_else(||"0".to_owned());
			xml.tag_fn(tag!(tr), |xml|{
				xml.tag_fn(tag!(td), |xml|{xml.text(&name)});
				xml.tag_fn(tag!(td), |xml|{xml.text(&xtype)});
				xml.tag_fn(tag!(td), |xml|{xml.text(&problem)});
				xml.tag_fn(tag!(td), |xml|{xml.text(&format!("{}",level))});
				xml.tag_fn(tag!(td), |xml|{xml.text(&pfail)});
				xml.tag_fn(tag!(td), |xml|{xml.text(&sksize)});
				xml.tag_fn(tag!(td), |xml|{xml.text(&pksize)});
				xml.tag_fn(tag!(td), |xml|{xml.text(&ctsize)});
				xml.tag_fn(tag!(td), |xml|{xml.text(&tsize)});
				xml.tag_fn(tag!(td), |xml|{xml.text(&status)});
			});
		}
	});
}

pub fn nistpqctest() -> XmlSerializer
{
	let conn = db_connect();
	let mut xml = XmlSerializer::new();
	xml.set_content_type(CONTENT_TYPE_XHTML);
	xml.tag_fn(Html, |xml|{
		xml.tag_fn(tag!(head), |xml|{
			xml.impulse(tag!(link attr!(rel="stylesheet"), attr!(type="text/css"),
				attr!(href="/static/nistpqctest.css")));
			xml.impulse(tag!(script attr!(type="text/javascript"), attr!(src="/static/nistpqctest.js")));
			xml.tag_fn(tag!(title), |xml|{
				xml.text("NIST PQC test");
			});
		});
		xml.tag_fn(tag!(body), |xml|{
			xml.tag_fn(tag!(table), |xml|{
				xml.tag_fn(tag!(tr), |xml|{
					xml.tag_fn(tag!(th), |xml|{xml.text("Name")});
					xml.tag_fn(tag!(th), |xml|{xml.text("Type")});
					xml.tag_fn(tag!(th), |xml|{xml.text("problem")});
					xml.tag_fn(tag!(th), |xml|{xml.text("Level")});
					xml.tag_fn(tag!(th), |xml|{xml.text("Pfail")});
					xml.tag_fn(tag!(th), |xml|{xml.text("Sksize")});
					xml.tag_fn(tag!(th), |xml|{xml.text("Pksize")});
					xml.tag_fn(tag!(th), |xml|{xml.text("Ctsize")});
					xml.tag_fn(tag!(th), |xml|{xml.text("Totalsize")});
					xml.tag_fn(tag!(th), |xml|{xml.text("Status")});
				});
				for row in conn.query("SELECT type, name, level, sksize, pksize, ctsize, status, \
					pfail, problem FROM nistpqc ORDER BY name, level, pfail", &[]).unwrap().
					iter() {
					let xtype: i32 = row.get(0);
					let name: String = row.get(1);
					let level: i32 = row.get(2);
					let sksize: Option<i32> = row.get(3);
					let pksize: Option<i32> = row.get(4);
					let ctsize: Option<i32> = row.get(5);
					let status: String = row.get(6);
					let problem: String = row.get(8);
					let pfail: Option<i32> = row.get(7);
					let tsize = if let (Some(pk),Some(ct)) = (pksize,ctsize) {
						Some(pk + ct)
					} else {
						None
					};
					let xtype = match xtype {
						1 => "Kex",
						2 => "Pke",
						3 => "Sig",
						_ => "?",
					};
					let sksize = sksize.map(|x|format!("{}",x)).unwrap_or_else(||"?".to_owned());
					let pksize = pksize.map(|x|format!("{}",x)).unwrap_or_else(||"?".to_owned());
					let ctsize = ctsize.map(|x|format!("{}",x)).unwrap_or_else(||"?".to_owned());
					let tsize = tsize.map(|x|format!("{}",x)).unwrap_or_else(||"?".to_owned());
					let pfail = pfail.map(|x|format!("2^-{}",x)).unwrap_or_else(||"0".
						to_owned());
					xml.tag_fn(tag!(tr), |xml|{
						xml.tag_fn(tag!(td), |xml|{xml.text(&name)});
						xml.tag_fn(tag!(td), |xml|{xml.text(&xtype)});
						xml.tag_fn(tag!(td), |xml|{xml.text(&problem)});
						xml.tag_fn(tag!(td), |xml|{xml.text(&format!("{}",level))});
						xml.tag_fn(tag!(td), |xml|{xml.text(&pfail)});
						xml.tag_fn(tag!(td), |xml|{xml.text(&sksize)});
						xml.tag_fn(tag!(td), |xml|{xml.text(&pksize)});
						xml.tag_fn(tag!(td), |xml|{xml.text(&ctsize)});
						xml.tag_fn(tag!(td), |xml|{xml.text(&tsize)});
						xml.tag_fn(tag!(td), |xml|{xml.text(&status)});
					});
				}
			});
		});
	});
	xml
}
