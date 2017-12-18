#!/usr/bin/python3
from bottle import route, run, template, request, response, redirect, abort, hook, parse_auth, local;
from bottle import HTTPError;
import postgresql;
import time
import sys;


DB_SOCKET=":var:run:postgresql:.s.PGSQL.5432";
DB_NAME="pbndb";
DB_USER="pbn";

@hook('before_request')
def database_connect():
	if hasattr(local, 'priv_db'): return;
	local.priv_db = postgresql.open("pq://%s@[unix:%s]/%s" % (DB_USER, DB_SOCKET, DB_NAME));

def SQL(sql):
        return local.priv_db.prepare(sql)

import hashlib;
import time;

SEED = b"9vk2VmEsHICVXQNMYHAOF7Fe6lzR7eMq";

def randomF(n):
	h = hashlib.md5(SEED + n.to_bytes(3, byteorder="big")).digest();
	return int.from_bytes(h[:2], byteorder="big") & 0x7FFF;

def encode_scene(n):
	l = n >> 15;
	r = n & 0x7FFF;
	r = r ^ randomF(0x00000 | l);
	l = l ^ randomF(0x08000 | r);
	r = r ^ randomF(0x10000 | l);
	l = l ^ randomF(0x18000 | r);
	n = (l << 15) | r;
	LETTERS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
	x = "";
	for i in range(0,6):
		x = x + LETTERS[(n >> (5 * i)) & 31];
	return x;

def decode_scene(s):
	if len(s) != 6: return -1;
	n = 0;
	for i in range(0,6):
		c = ord(s[i]);
		if c >= 65 and c <= 90: n = n | ((c - 65) << 5 * i);
		elif c >= 50 and c <= 55: n = n | ((c - 24) << 5 * i);
		else: return -1;
	l = n >> 15;
	r = n & 0x7FFF;
	l = l ^ randomF(0x18000 | r);
	r = r ^ randomF(0x10000 | l);
	l = l ^ randomF(0x08000 | r);
	r = r ^ randomF(0x00000 | l);
	return (l << 15) | r;


def send_status(code, message):
	response.status = 403;
	response.content_type="application/json";
	set_cors();
	return {"status":message};

def set_cors():
	origin = request.headers.get('origin');
	if origin:
		response.set_header("Access-Control-Allow-Origin", origin);
		response.set_header("Access-Control-Allow-Methods", "GET, PUT, POST, OPTIONS");
		response.set_header("Access-Control-Allow-Headers", "api-origin, api-key, since, until, Content-Type");

def cleanup_origins():
	fn = SQL("DELETE FROM applications WHERE temporary=true AND expires < $1");
	fn(int(time.time()));

def get_origin(require_authenticated):
	origin = request.headers.get('api-origin');
	if not origin:
		origin = request.headers.get('origin');
		if not origin:
			return False;
		#Only permanent origins are allowed.
		sqlfunc = SQL('SELECT COUNT(*) FROM applications WHERE origin=$1 AND login=true AND temporary=false');
		if int(sqlfunc(origin)[0][0]) < 1:
			return False;
		if not require_authenticated:
			return origin;
	apikey = request.headers.get('api-key');
	if not apikey:
		return False;
	cleanup_origins();
	sqlfunc = SQL('SELECT origin FROM applications WHERE origin=$1 AND apikey=$2 AND login=true');
	res = sqlfunc(origin, apikey);
	if len(res) > 0:
		parts = origin.rsplit('#', 1);
		return parts[0] if len(parts) == 2 else origin;
	return False;

def auth_scene(scene, require_key):
	origin = get_origin(require_key);
	if not origin or not scene:
		return False;
	sqlfunc = SQL('SELECT scenes.name AS name FROM applications, application_scene, scenes WHERE origin=$1 AND applications.appid=application_scene.appid AND application_scene.sceneid=scenes.sceneid AND scenes.sceneid=$2');
	res = sqlfunc(origin, scene);
	return len(res) > 0;


@route('/scenes',method='OPTIONS')
def handle_scenes_options():
	set_cors();
	return "";

@route('/scenes',method='GET')
def handle_scenes_get():
	origin = get_origin(False);
	if not origin:
		response.status = 403;
		response.content_type="text/plain";
		set_cors();
		return "<<no origin set>>\n";
	sqlfunc = SQL('SELECT scenes.sceneid AS id, scenes.name AS name FROM applications, application_scene, scenes WHERE origin=$1 AND applications.appid=application_scene.appid AND application_scene.sceneid=scenes.sceneid');
	res = sqlfunc(origin);
	res2 = {};
	for i in res:
		res2[encode_scene(i[0])] = i[1];
	response.content_type="application/json";
	set_cors();
	return res2;

@route('/scenes',method='POST')
def handle_scenes_post():
	origin = get_origin(True);
	name = request.forms.name;
	width = request.forms.width;
	height = request.forms.height;
	if not origin: return send_status(403, "noorigin");
	sqlfunc = SQL("SELECT appid FROM applications WHERE origin=$1");
	res = sqlfunc(origin);
	appid = int(res[0][0]);
	sqlfunc = SQL('INSERT INTO scenes (name,width,height) VALUES ($1,$2,$3) RETURNING sceneid');
	res = sqlfunc(name, int(width), int(height));
	sceneid = int(res[0][0]);
	sqlfunc = SQL('INSERT INTO application_scene (appid,sceneid) VALUES ($1,$2)');
	res = sqlfunc(appid, sceneid);
	response.content_type="application/json";
	set_cors();
	return {'scene':encode_scene(sceneid)};

def return_remainder(scene, since, until):
	sqlfunc = SQL('SELECT width, height FROM scenes WHERE sceneid=$1');
	res = sqlfunc(scene);
	if len(res) == 0: return send_status(404, "notfound");
	width = int(res[0][0]);
	height = int(res[0][1]);
	sqlfunc = SQL('SELECT timestamp,username,color,x,y FROM scene_data WHERE sceneid=$1 AND timestamp>=$2 AND timestamp <= $3 ORDER BY timestamp, recordid');
	if not since: since = -8999999999999999999;
	if not until: until =  8999999999999999999;
	res = sqlfunc(scene, int(since), int(until));
	res2 = [];
	for i in res:
		tmp = {
			'ts': i[0],
			'u': i[1],
			'c': i[2],
			'x': i[3],
			'y': i[4],
		};
		res2.append(tmp);
	response.content_type="application/json";
	set_cors();
	return {'data':res2,'width':width,'height':height};

@route('/scenes/<scene>',method='OPTIONS')
def handle_scene_options(scene):
	set_cors();
	return "";

@route('/scenes/<scene>',method='GET')
def handle_scene_get(scene):
	scene = decode_scene(scene);
	since = request.headers.get('since');
	until = request.headers.get('until');
	return return_remainder(scene, since, until);

@route('/scenes/<scene>',method='PUT')
def handle_scene_put(scene):
	starttime = time.time();
	scene = decode_scene(scene);
	if not auth_scene(scene, True): return send_status(403, "noaccess");
	json = request.json;
	json = json["data"];

	sqlfunc = SQL('SELECT width, height FROM scenes WHERE sceneid=$1');
	res = sqlfunc(scene);
	if len(res) == 0: return send_status(404, "notfound");
	width = int(res[0][0]);
	height = int(res[0][1]);

	buffer = MappedImageState("currentstate/"+str(int(scene)), width, height);
	sqlfunc = SQL('INSERT INTO scene_data (sceneid,timestamp,username,color,x,y) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING');
	transstarttime = time.time();
	SQL('BEGIN TRANSACTION')();
	for i in json:
		timestamp = i["ts"];
		username = i["u"];
		color = i["c"];
		x = i["x"];
		y = i["y"];
		try:
			sqlfunc(scene, int(timestamp), username, int(color), int(x), int(y));
			buffer.write_pixel(int(x), int(y), int(timestamp), 0xff000000 | int(color));
		except:
			pass;
	buffer.flush();
	del buffer;
	commitstarttime = time.time();
	SQL('COMMIT')();
	commitendtime = time.time();
	response.content_type="application/json";
	set_cors();
	print("start="+str(transstarttime-starttime)+", write="+str(commitstarttime-transstarttime)+", commit="+str(commitendtime-commitstarttime), file=sys.stderr);
	return {};

@route('/scenes/<scene>',method='POST')
def handle_scene_post(scene):
	scene = decode_scene(scene);
	if not auth_scene(scene, True): return send_status(403, "noaccess");
	if request.forms.a:
		neworigin = request.forms.a;
		sqlfunc = SQL('SELECT appid FROM applications WHERE origin=$1');
		res = sqlfunc(neworigin);
		if len(res) == 0: return send_status(418, "unknownorigin");
		appid = int(res[0][0]);
		sqlfunc = SQL('INSERT INTO application_scene (appid,sceneid) VALUES ($1,$2)');
		sqlfunc(appid, scene);
		response.content_type="application/json";
		set_cors();
		return {};
	if request.forms.d:
		neworigin = request.forms.a;
		sqlfunc = SQL('SELECT appid FROM applications WHERE origin=$1');
		res = sqlfunc(neworigin);
		if len(res) == 0: return send_status(418, "unknownorigin");
		appid = int(res[0][0]);
		sqlfunc = SQL('DELETE FROM application_scene WHERE appid=$1 AND sceneid=$2');
		sqlfunc(appid, scene);
		response.content_type="application/json";
		set_cors();
		return {};


	sqlfunc = SQL('SELECT width, height FROM scenes WHERE sceneid=$1');
	res = sqlfunc(scene);
	if len(res) == 0: return send_status(404, "notfound");
	width = int(res[0][0]);
	height = int(res[0][1]);

	username = request.forms.u;
	color = request.forms.c;
	x = request.forms.x;
	y = request.forms.y;
	timestamp = request.forms.ts;
	buffer = MappedImageState("currentstate/"+str(int(scene)), width, height);
	sqlfunc = SQL('INSERT INTO scene_data (sceneid,timestamp,username,color,x,y) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING');
	sqlfunc(scene, int(timestamp), username, int(color), int(x), int(y));
	buffer.write_pixel(int(x), int(y), int(timestamp), int(color));
	buffer.flush();
	del buffer;
	response.content_type="application/json";
	set_cors();
	return {};

@route('/scenes/<scene>',method='DELETE')
def handle_scene_post(scene):
	scene = decode_scene(scene);
	if not auth_scene(scene, True): return send_status(403, "noaccess");
	sqlfunc = SQL('DELETE FROM scenes WHERE sceneid=$1');
	sqlfunc(scene);
	response.content_type="application/json";
	set_cors();
	return {};

from sys import argv;
import math;
import json;

def write_byte(byte):
	return byte.to_bytes(1,byteorder='little');

def write_integer(integer):
	out = b"";
	while integer >= 128:
		out = out + write_byte(128 + (integer % 128));
		integer = integer // 128;
	return out + write_byte(integer);

def write_string(string):
	tmp = string.encode('utf-8')
	return write_integer(len(tmp)) + tmp;

def write_header(kind):
	return (0xADDB2D86).to_bytes(4, byteorder='big') + kind.to_bytes(4, byteorder='big');

def write_frame(sync, spin, x, y, color):
	t = 0;
	r = (color >> 16) & 255;
	g = (color >> 8) & 255;
	b = color & 255;
	if sync: t = t + 1;
	if spin: t = t + 2;
	return t.to_bytes(1, byteorder='little') + x.to_bytes(2, byteorder='little') + \
		y.to_bytes(2, byteorder='little') + r.to_bytes(2, byteorder='little') + \
		g.to_bytes(2, byteorder='little') + b.to_bytes(2, byteorder='little');

def write_file(sceneid, width, height,moviearr):
	tmp = (0x6C736D761A).to_bytes(5, byteorder='big');	#The file magic.
	tmp = tmp + write_string("pbn");			#systemtype.
	#Settings.
	tmp = tmp + write_byte(1) + write_string("width") + write_string("{}".format(width));
	tmp = tmp + write_byte(1) + write_string("height") + write_string("{}".format(height));
	tmp = tmp + write_byte(0);				#End of settings.
	#MOVIETIME.
	movietime = write_integer(1000000000) + write_integer(0);
	tmp = tmp + (write_header(0x18C3A975) + write_integer(len(movietime)) + movietime);
	#COREVERSION
	coreversion = b"pbn";
	tmp = tmp + (write_header(0xE4344C7E) + write_integer(len(coreversion)) + coreversion);
	#ROMHASH:
	romhash = b"\x00e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
	tmp = tmp + (write_header(0x0428ACFC) + write_integer(len(romhash)) + romhash);
	#ROMHINT.
	romhint = b"\x00pbn";
	tmp = tmp + (write_header(0x6F715830) + write_integer(len(romhint)) + romhint);
	#RRDATA.
	rrdata = b"\x1f\x00";
	tmp = tmp + (write_header(0xA3A07F71) + write_integer(len(rrdata)) + rrdata);
	#PROJECTID.
	projectid = "scene{}".format(sceneid).encode('utf-8');
	tmp = tmp + (write_header(0x359BFBAB) + write_integer(len(projectid)) + projectid);
	#MOVIEDATA.
	iframecnt = 0;
	lastframe = -1;
	if len(moviearr) > 0:
		timebase = moviearr[0][0];
		for i in range(0,len(moviearr)):
			evtime = moviearr[i][0] - timebase;
			framenum = 3 * evtime // 50;		# 3 frames in 50ms.
			if framenum == lastframe:
				iframecnt = iframecnt + 1;	#New subframe.
			else:
				iframecnt = iframecnt + (framenum - lastframe);	#Padding + new frame.
				lastframe = framenum;
	tmp = tmp + write_header(0xF3DCA44B) + write_integer(11*iframecnt);
	spin = True;
	lastframe = -1;
	if len(moviearr) > 0:
		timebase = moviearr[0][0];
		for i in range(0,len(moviearr)):
			xframe = moviearr[i];
			evtime = xframe[0] - timebase;
			framenum = 3 * evtime // 50;	# 3 frames in 50ms.
			if framenum == lastframe:
				tmp = tmp + write_frame(False, spin, xframe[3], xframe[4], xframe[2]); 
				spin = not spin;
			else:
				spin = False;
				for i in range(lastframe + 1, framenum):
					tmp = tmp + write_frame(True, False, 0, 0, 0); 
				tmp = tmp + write_frame(True, True, xframe[3], xframe[4], xframe[2]); 
				lastframe = framenum;
	return tmp;

@route('/scenes/<scene>/lsmv',method='GET')
def handle_scene_get_lsmv(scene):
	scene = decode_scene(scene);
	sqlfunc = SQL('SELECT width, height FROM scenes WHERE sceneid=$1');
	res = sqlfunc(scene);
	if len(res) == 0:
		response.status = 404;
		response.content_type="text/plain";
		set_cors();
		return "<<not found>>\n";
	width = int(res[0][0]);
	height = int(res[0][1]);
	sqlfunc = SQL('SELECT timestamp,username,color,x,y FROM scene_data WHERE sceneid=$1 ORDER BY timestamp, recordid');
	res = sqlfunc(scene);
	lsmv = write_file(encode_scene(scene), width, height, res);
	response.content_type="application/x-lsnes-movie";
	return lsmv;

@route('/scenes/<scene>/image',method='GET')
def handle_scene_get_image(scene):
	scene = decode_scene(scene);
	sqlfunc = SQL('SELECT width, height FROM scenes WHERE sceneid=$1');
	res = sqlfunc(scene);
	if len(res) == 0:
		response.status = 404;
		response.content_type="text/plain";
		set_cors();
		return "<<not found>>\n";
	width = int(res[0][0]);
	height = int(res[0][1]);
	buffer = MappedImageState("currentstate/"+str(int(scene)), width, height);
	buffer2,w,h = buffer.image_as_bytes();
	buffer2 = w.to_bytes(4, byteorder='little') + h.to_bytes(4, byteorder='little') + buffer2;
	del buffer;
	response.content_type="application/octet-stream";
	return buffer2;

import mmap;
import ctypes;

class MappedImageState:
	def __init__(self, backingfile, width, height):
		self._width = width;
		self._height = height;
		#Note that these have to be multiples of 4096.
		self._section1offset = 0;
		self._section1size = 4 * self._width * self._height;
		self._section1size += (4096 - self._section1size % 4096) % 4096;
		self._section2offset = self._section1offset + self._section1size;
		self._section2size = 8 * self._width * self._height;
		self._section2size += (4096 - self._section2size % 4096) % 4096;
		self._totalsize = self._section2offset + self._section2size;
		#Ok, mmap the file, creating if it does not exist.
		try:
			self._fd = open(backingfile, 'x+b');
			content = b'\0' * self._totalsize;
			self._fd.write(content);
			self._fd.flush();
		except FileExistsError:
			self._fd = open(backingfile, 'r+b');
		self._fd.seek(0)
		self._rawfile = mmap.mmap(self._fd.fileno(), self._totalsize, mmap.MAP_SHARED);
		section1type = ctypes.c_uint * (self._width * self._height);
		section2type = ctypes.c_ulonglong * (self._width * self._height);
		self._section1 = section1type.from_buffer(self._rawfile, self._section1offset);
		self._section2 = section2type.from_buffer(self._rawfile, self._section2offset);

	def write_pixel(self, x, y, timestamp, color):
		index = y * self._width + x;
		if x < 0 or y < 0 or x >= self._width or y >= self._height:
			return;
		if timestamp >= self._section2[index]:
			self._section1[index] = color;
			self._section2[index] = timestamp;

	def flush(self):
		self._rawfile.flush();	#Should do nothing.

	def image_as_bytes(self):
		bcnt = 4 * self._width * self._height;
		self._rawfile.seek(self._section1offset);
		tmp = self._rawfile.read(bcnt);
		return tmp, self._width, self._height;



run(server='flipflop')

