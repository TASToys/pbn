#include "c-interface.h"
#include <stdint.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>

unsigned screenwidth = 64;
unsigned screenheight = 56;
uint32_t* screen;
short (*get_input)(unsigned port, unsigned index, unsigned control) = NULL;
void (*submit_sound)(const int16_t* samples, size_t count, int stereo, double rate);
void (*submit_frame)(struct lsnes_core_framebuffer_info* fb, uint32_t fps_n, uint32_t fps_d);
int emulated_one = 0;

static int do_enumerate_cores(struct lsnes_core_enumerate_cores* x)
{
	static unsigned sysregions[2] = {0,LSNES_END_OF_LIST};
	get_input = x->get_input;
	submit_sound = x->submit_sound;
	submit_frame = x->submit_frame;
	x->sysregions = sysregions;
	return 0;
}


static int do_core_get_core_info(struct lsnes_core_get_core_info* x)
{
	x->json = "\
{\
	\"X\":{\"type\":\"lightgun\", \"name\":\"x\", \"min\":0, \"max\":255},\
	\"Y\":{\"type\":\"lightgun\", \"name\":\"y\", \"min\":0, \"max\":255},\
	\"R\":{\"type\":\"taxis\", \"name\":\"r\", \"min\":0, \"max\":255},\
	\"G\":{\"type\":\"taxis\", \"name\":\"g\", \"min\":0, \"max\":255},\
	\"B\":{\"type\":\"taxis\", \"name\":\"b\", \"min\":0, \"max\":255},\
	\"P\":{\"type\":\"button\", \"name\":\"s\"},\
	\"F\":{\"type\":\"button\", \"name\":\"framesync\", \"symbol\":\"F\", \"shadow\":true},\
	\"d\":{\"type\":\"pbn\", \"class\":\"pbn\", \"buttons\":[\"X\",\"Y\",\"R\",\"G\",\"B\",\"P\"]},\
	\"s\":{\"type\":\"(system)\", \"class\":\"(system)\", \"buttons\":[\"F\"]},\
	\"port\":[{\"symbol\":\"pbn\", \"name\":\"pbn\", \"hname\":\"pbn\", \
		\"controllers\":[\"s\", \"d\"], \"legal\":[0]}]\
}\
";
	x->root_ptr = "port";
	x->shortname = "pbn";
	x->fullname = "pbn";
	x->cap_flags1 = LSNES_CORE_CAP1_SCALE | LSNES_CORE_CAP1_LIGHTGUN;
	return 0;
}

static int do_core_get_type_info(struct lsnes_core_get_type_info* x)
{
	static unsigned regions[2] = {0, LSNES_END_OF_LIST};
	static struct lsnes_core_get_type_info_param settings[3] = {
		{"width", "width", "64", NULL, "[1-9]|([1-9]|1[0-9]|2[0-4])[0-9]|25[0-6]"},
		{"height", "height", "56", NULL, "[1-9]|([1-9]|1[0-9]|2[0-4])[0-9]|25[0-6]"},
		{NULL, NULL, NULL, NULL, NULL}
	};
	static struct lsnes_core_get_type_info_romimage images[2] = {
		{"pbn", "pbn", 1, 0, 0, "pbn"},
		{NULL, NULL, 0, 0, 0, NULL},
	};
	x->core = 0;
	x->iname = "pbn";
	x->hname = "pbn";
	x->sysname = "pbn";
	x->bios = NULL;
	x->regions = regions;
	x->images = images;
	x->settings = settings;
	return 0;
}

static int do_core_get_region_info(struct lsnes_core_get_region_info* x)
{
	static unsigned compatible[2] = {0, LSNES_END_OF_LIST};
        x->iname = "pbn";
        x->hname = "pbn";
	x->priority = 0;
	x->multi = 0;
	x->fps_n = 60;
	x->fps_d = 1;
	x->compatible_runs = compatible;
	return 0;
}

static int do_core_get_sysregion_info(struct lsnes_core_get_sysregion_info* x)
{
	x->name = "pbn";
	x->type = 0;
	x->region = 0;
	x->for_system = "pbn";
	return 0;
}

static int do_core_get_av_state(struct lsnes_core_get_av_state* x)
{
	x->fps_n = 60;
	x->fps_d = 1;
	x->par = 1;
	x->rate_n = 12000;
	x->rate_d = 1;
	x->lightgun_width = screenwidth;
	x->lightgun_height = screenheight;
	return 1;
}

static int do_core_emulate(struct lsnes_core_emulate* x)
{
	static short sound[400];
	emulated_one = 1;
	int last_spin = 0;
	while(1) {
		short xcoord = get_input(0, 1, 0);
		short ycoord = get_input(0, 1, 1);
		uint32_t r = get_input(0, 1, 2);
		uint32_t g = get_input(0, 1, 3);
		uint32_t b = get_input(0, 1, 4);
		int spin = get_input(0, 1, 5);
		if(spin == last_spin) break;
		last_spin = spin;
		uint32_t v = r * 65536 + g * 256 + b;
		if(xcoord >= 0 && ycoord >= 0 && xcoord < screenwidth && ycoord < screenheight)
			screen[ycoord * screenwidth + xcoord] = v;
	}
	submit_sound(sound, 200, 1, 12000);
	static struct lsnes_core_framebuffer_info fb;
	fb.type = LSNES_CORE_PIXFMT_RGB32;
	fb.mem = (const char*)screen;
	fb.physwidth = screenwidth;
	fb.physheight = screenheight;
	fb.physstride = 4 * screenwidth;
	fb.width = screenwidth;
	fb.height = screenheight;
	fb.stride = 4 * screenwidth;
	fb.offset_x = 0;
	fb.offset_y = 0;
	submit_frame(&fb, 60, 1);
}

static int do_core_savestate(struct lsnes_core_savestate* x)
{
	static unsigned char* buffer;
	if(buffer) free(buffer);
	size_t buffersize = screenwidth * screenheight * 3 + 1;
	buffer = malloc(buffersize);
	for(size_t i = 0; i < screenwidth * screenheight; i++) {
		buffer[3*i+0] = screen[i] >> 16;
		buffer[3*i+1] = screen[i] >> 8;
		buffer[3*i+2] = screen[i];
	}
	buffer[screenwidth*screenheight*3] = emulated_one;
	x->size = buffersize;
	x->data = buffer;
	return 0;
}

static int do_core_loadstate(struct lsnes_core_loadstate* x)
{
	if(x->size != screenwidth * screenheight * 3 + 1) return -1;
	for(size_t i = 0; i < screenwidth * screenheight; i++) {
		screen[i] = x->data[3*i+0];
		screen[i] = 256 * screen[i] + x->data[3*i+1];
		screen[i] = 256 * screen[i] + x->data[3*i+2];
	}
	emulated_one = x->data[screenwidth*screenheight*3];
	return 0;
}

static int do_core_get_controllerconfig(struct lsnes_core_get_controllerconfig* x)
{
	static struct lsnes_core_get_controllerconfig_logical_entry logicals[2] = {{0, 1}, {0, 0}};
	static unsigned ctypes[2] = {0, LSNES_END_OF_LIST};
	x->controller_types = ctypes;
	x->logical_map = logicals;
	return 0;
}

static int do_core_load_rom(struct lsnes_core_load_rom* x)
{
	unsigned width = 64;
	unsigned height = 56;
	struct lsnes_core_system_setting* setting = x->settings;
	while(setting->name) {
		if(!strcmp(setting->name, "width"))
			width = atoi(setting->value);
		if(!strcmp(setting->name, "height"))
			height = atoi(setting->value);
		setting++;
	}
	//Just reset the image.
	emulated_one = 0;
	if(screen) free(screen);
	screenwidth = width;
	screenheight = height;
	screen = calloc(screenwidth * screenheight, 4);
	return 0;
}

static int do_core_compute_scale(struct lsnes_core_compute_scale* x)
{
	unsigned xf = 512 / (screenwidth ? screenwidth : 1);
	unsigned yf = 512 / (screenheight ? screenheight : 1);
	x->hfactor = emulated_one ? (xf ? xf : 1) : 1;
	x->vfactor = emulated_one ? (yf ? yf : 1) : 1;
	return 0;
}

#define CASE(X,Y) case X: r = do_#Y ((struct lsnes_#Y *)params); break

int lsnes_core_entrypoint(unsigned action, unsigned item, void* params, const char** error)
{
	int r = -1;
	switch(action) {
case LSNES_CORE_ENUMERATE_CORES: r = do_enumerate_cores ((struct lsnes_core_enumerate_cores *)params); break;
case LSNES_CORE_GET_CORE_INFO: r = do_core_get_core_info ((struct lsnes_core_get_core_info *)params); break;
case LSNES_CORE_GET_TYPE_INFO: r = do_core_get_type_info ((struct lsnes_core_get_type_info *)params); break;
case LSNES_CORE_GET_REGION_INFO: r = do_core_get_region_info ((struct lsnes_core_get_region_info *)params); break;
case LSNES_CORE_GET_SYSREGION_INFO: r = do_core_get_sysregion_info ((struct lsnes_core_get_sysregion_info *)params); break;
case LSNES_CORE_GET_AV_STATE: r = do_core_get_av_state ((struct lsnes_core_get_av_state *)params); break;
case LSNES_CORE_EMULATE: r = do_core_emulate ((struct lsnes_core_emulate *)params); break;
case LSNES_CORE_SAVESTATE: r = do_core_savestate ((struct lsnes_core_savestate *)params); break;
case LSNES_CORE_LOADSTATE: r = do_core_loadstate ((struct lsnes_core_loadstate *)params); break;
case LSNES_CORE_GET_CONTROLLERCONFIG: r = do_core_get_controllerconfig ((struct lsnes_core_get_controllerconfig *)params); break;
case LSNES_CORE_LOAD_ROM: r = do_core_load_rom ((struct lsnes_core_load_rom *)params); break;
case LSNES_CORE_COMPUTE_SCALE: r = do_core_compute_scale ((struct lsnes_core_compute_scale *)params); break;
	};
	if(r < 0) *error = "ERROR";
	return r;
}
