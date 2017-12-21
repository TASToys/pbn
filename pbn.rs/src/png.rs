use std::io::Write;
use std::ptr::null;
use ::mmapstate::MmapImageState;
use std::io::Error as IoError;

#[derive(Copy,Clone,Debug)]
struct Adler32(u32);

impl Adler32
{
	#[allow(unsafe_code)]
	fn new() -> Adler32
	{
		Adler32(unsafe{adler32(0, null(), 0)})
	}
	#[allow(unsafe_code)]
	fn add(&mut self, data: &[u8])
	{
		self.0 = unsafe{adler32(self.0, data.as_ptr(), data.len() as u32)};
	}
	fn as_inner(&self) -> u32 { self.0 }
}

#[derive(Copy,Clone,Debug)]
struct Crc32(u32);

impl Crc32
{
	#[allow(unsafe_code)]
	fn new() -> Crc32
	{
		Crc32(unsafe{crc32(0, null(), 0)})
	}
	#[allow(unsafe_code)]
	fn add(&mut self, data: &[u8])
	{
		self.0 = unsafe{crc32(self.0, data.as_ptr(), data.len() as u32)};
	}
	fn add_wt<W:Write>(&mut self, data: &[u8], target: &mut W) -> Result<(), IoError>
	{
		self.add(data);
		target.write_all(data)
	}
	fn flush<W:Write>(&self, target: &mut W) -> Result<(), IoError>
	{
		let c = [(self.0 >> 24) as u8, (self.0 >> 16) as u8, (self.0 >> 8) as u8, self.0 as u8];
		target.write_all(&c)
	}
}

//Write the first IDAT chunk.
fn write_first_idat<W:Write>(out: &mut W)
{
	let mut crc = Crc32::new();
	out.write_all(&[0, 0, 0, 2]).unwrap();
	crc.add_wt(&[73, 68, 65, 84, 8, 29], out).unwrap();
	crc.flush(out).unwrap();
}

//sdata must be of exactly one scanline, and must be at most 65534 bytes.
fn write_scanline<W:Write>(out: &mut W, sdata: &[u8], chksum: &mut Adler32, bottom: bool)
{
	let sdlen = sdata.len() + 6 + if bottom { 4 } else { 0 };
	out.write_all(&[(sdlen >> 24) as u8, (sdlen >> 16) as u8, (sdlen >> 8) as u8, sdlen as u8]).unwrap();
	let mut crc = Crc32::new();
	let sdlena = ((sdata.len() + 1) >> 8) as u8;
	let sdlenb = (sdata.len() + 1) as u8;
	crc.add_wt(&[73, 68, 65, 84, if bottom { 1 } else { 0 }, sdlenb, sdlena, !sdlenb, !sdlena, 0], out).
		unwrap();
	chksum.add(&[0]);		//Zero is part of data.
	chksum.add(sdata);
	crc.add_wt(sdata, out).unwrap();
	if bottom {
		let chksum = chksum.as_inner();
		crc.add_wt(&[(chksum >> 24) as u8, (chksum >> 16) as u8, (chksum >> 8) as u8, chksum as u8], out).
			unwrap();
	}
	crc.flush(out).unwrap();
}

fn write_ihdr<W:Write>(out: &mut W, w: usize, h: usize)
{
	let mut crc = Crc32::new();
	out.write_all(&[137,80,78,71,13,10,26,10,0,0,0,13]).unwrap();
	crc.add_wt(&[73,72,68,82, (w >> 24) as u8, (w >> 16) as u8, (w >> 8) as u8, w as u8, (h >> 24) as u8,
		(h >> 16) as u8, (h >> 8) as u8, h as u8, 8, 6, 0, 0, 0], out).unwrap();
	crc.flush(out).unwrap()
}

fn write_iend<W:Write>(out: &mut W)
{
	let mut crc = Crc32::new();
	out.write_all(&[0, 0, 0, 0]).unwrap();
	crc.add_wt(&[73,69,78,68], out).unwrap();
	crc.flush(out).unwrap();
}

//Size of the scanned image.
pub fn scan_image_as_png_size(img: &MmapImageState) -> usize
{
	//PNG signature, IHDR, fixed IDAT, IDAT end overhead and IEND.
	let mut size = 8 + 3 * 12 + 13 + 2 + 4;
	let (w, h) = img.get_size();
	//For each row, 18 bytes of overhead.
	size += h * (18 + 4 * w);
	size
}

//Scan all IDAT chunks.
pub fn scan_image_as_png<W:Write>(out: &mut W, img: &MmapImageState)
{
	let (w, h) = img.get_size();
	let mut buf = vec![0;4*w];
	write_ihdr(out, w, h);
	write_first_idat(out);
	let mut chksum = Adler32::new();
	for y in 0..h {
		img.read_row(y, &mut buf);
		write_scanline(out, &buf, &mut chksum, y + 1 == h);
	}
	write_iend(out);
}

#[link(name = "z")]
extern
{
	fn adler32(crc: u32, buf: *const u8, len: u32) -> u32;
	fn crc32(crc: u32, buf: *const u8, len: u32) -> u32;
}

