#![allow(unsafe_code)]

use libc::{c_int, stat, fstat, mmap, open, munmap, close, ssize_t, /*read, */write, off_t, MAP_FAILED, c_void,
	O_RDWR, O_CREAT, PROT_READ, PROT_WRITE, MAP_SHARED, SEEK_END, lseek};
use std::path::Path;
use std::ptr::{read_volatile, write_volatile, null};
//use std::slice::from_raw_parts_mut;
use std::mem::{transmute, zeroed};
use std::io::Error as IoError;
use std::os::unix::ffi::OsStrExt;
use std::ffi::CString;
use std::cmp::min;
use std::mem::size_of;

trait LibcError: Copy { fn is_error(self) -> bool; }
impl LibcError for i32 { fn is_error(self) -> bool { self < 0 } }
impl LibcError for i64 { fn is_error(self) -> bool { self < 0 } }
impl LibcError for isize { fn is_error(self) -> bool { self < 0 } }

fn libc_error<X:LibcError>(x: X) -> Result<X, IoError>
{
	if x.is_error() { return Err(IoError::last_os_error()); }
	Ok(x)
}

struct FileDescriptor(c_int);

impl Drop for FileDescriptor
{
	fn drop(&mut self)
	{
		unsafe{close(self.0)};
	}
}

impl FileDescriptor
{
/*
	fn read(&self, buf: &mut [u8]) -> Result<ssize_t, IoError>
	{
		libc_error(unsafe{read(self.0, transmute(buf.as_mut_ptr()), buf.len())})
	}
*/
	fn write(&self, buf: &[u8]) -> Result<ssize_t, IoError>
	{
		libc_error(unsafe{write(self.0, transmute(buf.as_ptr()), buf.len())})
	}
	fn stat(&self, st: &mut stat) -> Result<(), IoError>
	{
		libc_error(unsafe{fstat(self.0, st as _)}).map(|_|())
	}
	fn seek(&self, off: off_t, whence: c_int) -> Result<off_t, IoError>
	{
		libc_error(unsafe{lseek(self.0, off, whence)})
	}
	fn mmap<T:Copy>(&self, prot: c_int, flags: c_int, size: usize, offset: off_t) ->
		Result<MappedArea<T>, IoError>
	{
		unsafe {
			let ptr = mmap(null::<c_void>() as _, size as _, prot, flags, self.0, offset);
			if ptr == MAP_FAILED { return Err(IoError::last_os_error()); }
			Ok(MappedArea(transmute(ptr), size))
		}
	}
}

#[derive(Debug)]
struct MappedArea<T:Copy>(*mut T, usize);

impl<T:Copy> Drop for MappedArea<T>
{
	fn drop(&mut self)
	{
		unsafe{munmap(transmute(self.0), self.1)};
	}
}

impl<T:Copy> MappedArea<T>
{
	fn read(&self, offset: usize) -> T
	{
		if (offset+1)*size_of::<T>() > self.1 { panic!("Mapped area read out of range"); }
		unsafe{read_volatile(self.0.offset(offset as isize))}
	}
	fn write(&self, offset: usize, value: T)
	{
		if (offset+1)*size_of::<T>() > self.1 { panic!("Mapped area write out of range"); }
		unsafe{write_volatile(self.0.offset(offset as isize), value)}
	}
}

#[derive(Debug)]
pub struct MmapImageState
{
	pdatabase: MappedArea<u32>,
	tdatabase: MappedArea<i64>,
	width: usize,
	height: usize,
}

impl MmapImageState
{
	pub fn new<P:AsRef<Path>>(backing: P, width: usize, height: usize) -> Result<MmapImageState, String>
	{
		let pages1 = (width * height + 1023) / 1024;
		let pages2 = (width * height + 511) / 512;
		let backing = backing.as_ref();
		let backing = backing.as_os_str().as_bytes();
		let backing = CString::new(backing).unwrap();
		let fd = FileDescriptor(libc_error(unsafe{open(backing.as_ptr(), O_CREAT | O_RDWR, 438)}).map_err(
			|x|format!("open: {}", x))?);
		let mut st: stat = unsafe{zeroed()};
		fd.stat(&mut st).map_err(|x|format!("stat: {}", x))?;
		let mut tofill = ((pages1+pages2)<<12).saturating_sub(st.st_size as usize);
		if tofill > 0 {
			fd.seek(0, SEEK_END).map_err(|x|format!("seek: {}", x))?;
			while tofill > 0 {
				let buf = [0;8192];
				let n = min(buf.len(), tofill as usize);
				let n = fd.write(&buf[..n]).map_err(|x|format!("write: {}", x))?;
				tofill -= n as usize;
			}
		}
		let pdatabase = fd.mmap(PROT_READ | PROT_WRITE, MAP_SHARED, pages1<<12, 0).map_err(|x|format!(
			"mmap: {}", x))?;
		let tdatabase = fd.mmap(PROT_READ | PROT_WRITE, MAP_SHARED, pages2<<12, (pages1<<12) as _).map_err(
			|x|format!("mmap: {}", x))?;
		Ok(MmapImageState{
			pdatabase: pdatabase,
			tdatabase: tdatabase,
			width: width,
			height: height,
		})
	}
	pub fn write_pixel(&self, x: i32, y: i32, ts: i64, color: i32)
	{
		if x < 0 || y < 0 { return; }
		let x = x as usize;
		let y = y as usize;
		if x >= self.width || y >= self.height { return; }
		let offset = y * self.width + x;
		if self.tdatabase.read(offset) <= ts {
			self.pdatabase.write(offset, 0xFF000000 | (color & 0xFFFFFF) as u32);
			self.tdatabase.write(offset, ts);
		}
	}
	pub fn get_size(&self) -> (usize, usize)
	{
		(self.width, self.height)
	}
	pub fn read_row(&self, y: usize, buf: &mut [u8])
	{
		let offset = y * self.width;
		for i in 0..min(self.width, buf.len() / 4) {
			let x = self.pdatabase.read(offset + i);
			buf[4*i+0] = (x >> 16) as u8;
			buf[4*i+1] = (x >> 8) as u8;
			buf[4*i+2] = x as u8;
			buf[4*i+3] = (x >> 24) as u8;
		}
	}
}
