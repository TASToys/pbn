use super::json::json_parse_numeric;
use super::json::json_parse_string;
use super::parse_event_stream;

fn numeric_test_ok(input: &str)
{
	let padded = format!("{}x", input);
	assert_eq!(json_parse_numeric(input, true).unwrap(), (input.len(), input.to_owned()));
	assert_eq!(json_parse_numeric(&padded, false).unwrap(), (input.len(), input.to_owned()));
	assert_eq!(json_parse_numeric(&padded, true).unwrap(), (input.len(), input.to_owned()));
}

fn string_test_ok(input: &str, output: &str)
{
	assert_eq!(json_parse_string(input).unwrap(), (input.len(), output.to_owned()));
}

#[test]
fn test_parse_ok()
{
	let tests: &[&'static str] = &[
		"0", "-0", "1", "-1", "9", "-9", "100", "-100", "999", "-999",
		"0.0", "-0.0", "1.0", "-1.0", "9.0", "-9.0", "100.0", "-100.0", "999.0", "-999.0",
		"0.00", "0.001", "0.999", "1.00", "1.001", "1.999",
		"0e0", "0.1e0", "1.1e0", "-0e0", "-0.1e0", "-1.1e0",
		"0E0", "0.1E0", "1.1E0", "-0E0", "-0.1E0", "-1.1E0",
		"0E000", "0E999", "0E+000", "0E+999", "0E-000", "0E-999",
	];
	for i in tests.iter() { numeric_test_ok(*i); }
}

#[test]
fn parse_strings()
{
	string_test_ok(r#""""#, "");
	string_test_ok(r#""x""#, "x");
	string_test_ok(r#""foo""#, "foo");
	string_test_ok(r#""foo\bbar""#, "foo\u{8}bar");
	string_test_ok(r#""foo\tbar""#, "foo\tbar");
	string_test_ok(r#""foo\tbar""#, "foo\tbar");
	string_test_ok(r#""foo\nbar""#, "foo\nbar");
	string_test_ok(r#""foo\fbar""#, "foo\u{c}bar");
	string_test_ok(r#""foo\rbar""#, "foo\rbar");
	string_test_ok(r#""foo\u0000bar""#, "foo\0bar");
	string_test_ok(r#""foo\u0005bar""#, "foo\u{5}bar");
	string_test_ok(r#""foo\u1234bar""#, "foo\u{1234}bar");
	string_test_ok(r#""foo\uFFFDbar""#, "foo\u{fffd}bar");
	string_test_ok(r#""foo\ufffdbar""#, "foo\u{fffd}bar");
	string_test_ok(r#""foo\ud800\udc00bar""#, "foo\u{10000}bar");
	string_test_ok(r#""foo\ud800\udc01bar""#, "foo\u{10001}bar");
	string_test_ok(r#""foo\ud800\udfffbar""#, "foo\u{103FF}bar");
	string_test_ok(r#""foo\ud801\udc00bar""#, "foo\u{10400}bar");
	string_test_ok(r#""foo\udbff\udffdbar""#, "foo\u{10FFFD}bar");
	string_test_ok(r#""foo\b""#, "foo\u{8}");
	string_test_ok(r#""foo\t""#, "foo\t");
	string_test_ok(r#""foo\t""#, "foo\t");
	string_test_ok(r#""foo\n""#, "foo\n");
	string_test_ok(r#""foo\f""#, "foo\u{c}");
	string_test_ok(r#""foo\r""#, "foo\r");
	string_test_ok(r#""foo\u0000""#, "foo\0");
	string_test_ok(r#""foo\udbff\udffd""#, "foo\u{10FFFD}");
}

#[test]
fn parse_strings_invalid()
{
	json_parse_string(r#"""#).ok_or(()).unwrap_err();
	json_parse_string(r#""\""#).ok_or(()).unwrap_err();
	json_parse_string(r#""\u""#).ok_or(()).unwrap_err();
	json_parse_string(r#""\u1""#).ok_or(()).unwrap_err();
	json_parse_string(r#""\u12""#).ok_or(()).unwrap_err();
	json_parse_string(r#""\u123""#).ok_or(()).unwrap_err();
	json_parse_string(r#""\ud800""#).ok_or(()).unwrap_err();
	json_parse_string(r#""\udbff""#).ok_or(()).unwrap_err();
	json_parse_string(r#""\udc00""#).ok_or(()).unwrap_err();
	json_parse_string(r#""\udfff""#).ok_or(()).unwrap_err();
	json_parse_string(r#""\ud800\""#).ok_or(()).unwrap_err();
	json_parse_string(r#""\ud800\u""#).ok_or(()).unwrap_err();
	json_parse_string(r#""\ud800\ud""#).ok_or(()).unwrap_err();
	json_parse_string(r#""\ud800\udc""#).ok_or(()).unwrap_err();
	json_parse_string(r#""\ud800\udc0""#).ok_or(()).unwrap_err();
	json_parse_string(r#""\ud800\udbff""#).ok_or(()).unwrap_err();
	json_parse_string(r#""\ud800\ue000"#).ok_or(()).unwrap_err();
}

#[test]
fn test_parse_numeric_zero_digits()
{
	json_parse_numeric("01", false).ok_or(()).unwrap_err();
	json_parse_numeric("01", true).ok_or(()).unwrap_err();
}

#[test]
fn test_parse_numeric_trailing_crap()
{
	json_parse_numeric("0.", false).ok_or(()).unwrap_err();
	json_parse_numeric("0.", true).ok_or(()).unwrap_err();
	json_parse_numeric("0e", false).ok_or(()).unwrap_err();
	json_parse_numeric("0e", true).ok_or(()).unwrap_err();
	json_parse_numeric("1.", false).ok_or(()).unwrap_err();
	json_parse_numeric("1.", true).ok_or(()).unwrap_err();
	json_parse_numeric("1e", false).ok_or(()).unwrap_err();
	json_parse_numeric("1e", true).ok_or(()).unwrap_err();
	json_parse_numeric("1.1e", false).ok_or(()).unwrap_err();
	json_parse_numeric("1.1e", true).ok_or(()).unwrap_err();
	json_parse_numeric("1.1e+", false).ok_or(()).unwrap_err();
	json_parse_numeric("1.1e+", true).ok_or(()).unwrap_err();
	json_parse_numeric("1.1e-", false).ok_or(()).unwrap_err();
	json_parse_numeric("1.1e-", true).ok_or(()).unwrap_err();
}

#[test]
fn test_parse_numeric_empty()
{
	json_parse_numeric("", false).ok_or(()).unwrap_err();
	json_parse_numeric("", true).ok_or(()).unwrap_err();
}

#[test]
fn parse_event_streams()
{
	let stream = br#"{"data":[]}"#;
	let mut stream = &stream[..];
	assert_eq!(parse_event_stream(&mut stream).unwrap().len(), 0);

	let stream = br#"{"data":[{"ts":2,"u":"foo","c":5,"x":4,"y":8}]}"#;
	let mut stream = &stream[..];
	let strm = parse_event_stream(&mut stream).unwrap();
	assert_eq!(strm.len(), 1);
	assert_eq!(strm[0].ts, 2);
	assert_eq!(strm[0].username, "foo".to_owned());
	assert_eq!(strm[0].color, 5);
	assert_eq!(strm[0].x, 4);
	assert_eq!(strm[0].y, 8);

	let stream = br#"{"data":[{"ts":2,"u":"foo","c":5,"x":4,"y":"8"}]}"#;
	let mut stream = &stream[..];
	let strm = parse_event_stream(&mut stream).unwrap();
	assert_eq!(strm.len(), 1);
	assert_eq!(strm[0].ts, 2);
	assert_eq!(strm[0].username, "foo".to_owned());
	assert_eq!(strm[0].color, 5);
	assert_eq!(strm[0].x, 4);
	assert_eq!(strm[0].y, 8);

	let stream = br#"{"data":[{"ts":-2,"u":"foo","c":5,"x":4,"y":"8"}]}"#;
	let mut stream = &stream[..];
	let strm = parse_event_stream(&mut stream).unwrap();
	assert_eq!(strm.len(), 1);
	assert_eq!(strm[0].ts, -2);
	assert_eq!(strm[0].username, "foo".to_owned());
	assert_eq!(strm[0].color, 5);
	assert_eq!(strm[0].x, 4);
	assert_eq!(strm[0].y, 8);

	let stream = br#"{"data":[{"ts":2,"u":"foo","c":5,"x":4,"y":8},{"ts":12,"u":"bar","c":15,"x":14,"y":18}]}"#;
	let mut stream = &stream[..];
	let strm = parse_event_stream(&mut stream).unwrap();
	assert_eq!(strm.len(), 2);
	assert_eq!(strm[0].ts, 2);
	assert_eq!(strm[0].username, "foo".to_owned());
	assert_eq!(strm[0].color, 5);
	assert_eq!(strm[0].x, 4);
	assert_eq!(strm[0].y, 8);
	assert_eq!(strm[1].ts, 12);
	assert_eq!(strm[1].username, "bar".to_owned());
	assert_eq!(strm[1].color, 15);
	assert_eq!(strm[1].x, 14);
	assert_eq!(strm[1].y, 18);
}

#[test]
fn parse_event_streams_invalid()
{
	let stream = br#"{"data":[{"ts":2,"u":"foo","c":5,"x":4,"y":-8}]}"#;
	let mut stream = &stream[..]; assert!(parse_event_stream(&mut stream).is_err());
	let stream = br#"{"data":[{"ts":2,"u":"foo","c":5,"x":-4,"y":8}]}"#;
	let mut stream = &stream[..]; assert!(parse_event_stream(&mut stream).is_err());
	let stream = br#"{"data":[{"ts":2,"u":"foo","c":-5,"x":4,"y":8}]}"#;
	let mut stream = &stream[..]; assert!(parse_event_stream(&mut stream).is_err());
	let stream = br#"{"data":[{"ts":2,"u":"foo","c":5,"x":4,"y":2200000000}]}"#;
	let mut stream = &stream[..]; assert!(parse_event_stream(&mut stream).is_err());
	let stream = br#"{"data":[{"ts":2,"u":"foo","c":5,"x":4}]}"#;
	let mut stream = &stream[..]; assert!(parse_event_stream(&mut stream).is_err());
	let stream = br#"{"data":[{"ts":2,"u":"foo","c":5,"y":8}]}"#;
	let mut stream = &stream[..]; assert!(parse_event_stream(&mut stream).is_err());
	let stream = br#"{"data":[{"ts":2,"u":"foo","x":4,"y":8}]}"#;
	let mut stream = &stream[..]; assert!(parse_event_stream(&mut stream).is_err());
	let stream = br#"{"data":[{"ts":2,"c":5,"x":4,"y":8}]}"#;
	let mut stream = &stream[..]; assert!(parse_event_stream(&mut stream).is_err());
	let stream = br#"{"data":[{"u":"foo","c":5,"x":4,"y":8}]}"#;
	let mut stream = &stream[..]; assert!(parse_event_stream(&mut stream).is_err());
	let stream = br#"{"data":[{"ts":2,"u":"foo","c":5,"x":4,"y":8,"z":5}]}"#;
	let mut stream = &stream[..]; assert!(parse_event_stream(&mut stream).is_err());
	let stream = br#"{"data":[{"ts":2,"u":"foo","c":5,"x":4,"y":8,}]}"#;
	let mut stream = &stream[..]; assert!(parse_event_stream(&mut stream).is_err());
	let stream = br#"{"data":[{"ts":2,"u":"foo","c":5,"x":4,"y":8]}"#;
	let mut stream = &stream[..]; assert!(parse_event_stream(&mut stream).is_err());
	let stream = br#"{"data":[{"ts":2,"u":"foo","c":5,"x":4,"y":8}}"#;
	let mut stream = &stream[..]; assert!(parse_event_stream(&mut stream).is_err());
	let stream = br#"{"data":[{"ts":2,"u":"foo","c":5,"x":4,"y":8}]"#;
	let mut stream = &stream[..]; assert!(parse_event_stream(&mut stream).is_err());
}
