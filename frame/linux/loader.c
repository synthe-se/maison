/*
 * The boot's loader: a ring turning under Maison's mark on /dev/fb0, drawn by the initramfs until
 * the interface takes the panel (maison-ui.service ends it), or for <seconds> at most. Static, no
 * library: it outlives the initramfs it came from. The angle and the coverage of each pixel are
 * computed once, so that a frame costs a table's walk. A ring has no up: the panel's turn does not
 * matter, only where its centre lands.
 *
 *     maison-loader <centre x> <centre y> <ground 0xRRGGBB> <ink 0xRRGGBB> <seconds>
 */
#include <fcntl.h>
#include <linux/fb.h>
#include <math.h>
#include <stdint.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <time.h>

#define OUTER 26.0f
#define INNER 21.0f
#define SIDE 60
#define FPS 30
#define TURN_FRAMES 36

static uint8_t coverage[SIDE * SIDE], angle[SIDE * SIDE];

static float clamp01(float v) { return v < 0 ? 0 : v > 1 ? 1 : v; }

static uint32_t mix(uint32_t ground, uint32_t ink, unsigned alpha) {
	uint32_t out = 0;
	for (int shift = 0; shift <= 16; shift += 8) {
		unsigned g = (ground >> shift) & 255, i = (ink >> shift) & 255;
		out |= ((g * (255 - alpha) + i * alpha) / 255) << shift;
	}
	return out;
}

int main(int argc, char **argv) {
	if (argc != 6) return 2;
	int cx = atoi(argv[1]), cy = atoi(argv[2]), seconds = atoi(argv[5]);
	uint32_t ground = strtoul(argv[3], 0, 16), ink = strtoul(argv[4], 0, 16);
	int fd = open("/dev/fb0", O_RDWR);
	struct fb_var_screeninfo var;
	struct fb_fix_screeninfo fix;
	if (fd < 0 || ioctl(fd, FBIOGET_VSCREENINFO, &var) || ioctl(fd, FBIOGET_FSCREENINFO, &fix)) return 1;
	if (var.bits_per_pixel != 32) return 1;
	uint8_t *fb = mmap(0, fix.smem_len, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
	if (fb == MAP_FAILED) return 1;
	if (cx < SIDE / 2 || cy < SIDE / 2 || cx + SIDE / 2 > (int)var.xres || cy + SIDE / 2 > (int)var.yres) return 2;

	for (int y = 0; y < SIDE; y++)
		for (int x = 0; x < SIDE; x++) {
			float dx = x + 0.5f - SIDE / 2.0f, dy = y + 0.5f - SIDE / 2.0f, r = sqrtf(dx * dx + dy * dy);
			coverage[y * SIDE + x] = (uint8_t)(255 * clamp01(OUTER + 0.5f - r) * clamp01(r - INNER + 0.5f));
			angle[y * SIDE + x] = (uint8_t)((atan2f(dy, dx) / (2 * (float)M_PI) + 0.5f) * 255);
		}

	struct timespec pause = {0, 1000000000 / FPS};
	for (long frame = 0; frame < (long)seconds * FPS; frame++) {
		unsigned head = (unsigned)(frame * 256 / TURN_FRAMES) & 255;
		for (int y = 0; y < SIDE; y++) {
			uint32_t *row = (uint32_t *)(fb + (cy - SIDE / 2 + y) * fix.line_length) + cx - SIDE / 2;
			for (int x = 0; x < SIDE; x++) {
				unsigned c = coverage[y * SIDE + x];
				if (!c) continue;
				/* the tail behind the head fades over three quarters of the turn */
				unsigned behind = (head - angle[y * SIDE + x]) & 255;
				unsigned tail = behind < 192 ? 255 - behind * 255 / 192 : 0;
				row[x] = mix(ground, ink, c * tail / 255);
			}
		}
		nanosleep(&pause, 0);
	}
	return 0;
}
