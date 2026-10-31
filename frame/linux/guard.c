/*
 * An update on trial, watched from outside the system it tries: started by the initramfs, it
 * outlives it, and restarts the frame after <seconds> unless maison-commit ends it first, whatever
 * became of PID 1. With `arm`, the SoC's watchdog is left running (closed without its magic
 * character) so that a kernel started by kexec that hangs resets the frame; the driver and systemd
 * feed it once they are up, CONFIG_WATCHDOG_OPEN_TIMEOUT bounds the wait for them. Static, no
 * library.
 *
 *     maison-guard arm
 *     maison-guard <seconds>
 */
#include <fcntl.h>
#include <linux/watchdog.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/reboot.h>
#include <time.h>
#include <unistd.h>

int main(int argc, char **argv) {
	if (argc != 2) return 2;
	if (!strcmp(argv[1], "arm")) {
		int fd = open("/dev/watchdog", O_WRONLY), seconds = 30;
		if (fd < 0) return 1;
		ioctl(fd, WDIOC_SETTIMEOUT, &seconds);
		close(fd);
		return 0;
	}
	struct timespec wait = {atoi(argv[1]), 0};
	nanosleep(&wait, 0);
	sync();
	reboot(RB_AUTOBOOT);
	return 1;
}
