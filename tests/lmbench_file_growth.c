#include <assert.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

enum { block = 4096 };
static unsigned char data[block * 3];

static void verify_growth(const char *path, size_t keep, size_t grown) {
    int fd = open(path, O_CREAT | O_TRUNC | O_RDWR, 0600);
    assert(fd >= 0);
    memset(data, 0xa5, sizeof(data));
    assert(write(fd, data, sizeof(data)) == sizeof(data));
    assert(fsync(fd) == 0 && close(fd) == 0);
    fd = open(path, O_RDWR);
    assert(fd >= 0 && ftruncate(fd, keep) == 0);
    assert(ftruncate(fd, grown) == 0);
    assert(fsync(fd) == 0 && close(fd) == 0);
    fd = open(path, O_RDONLY);
    assert(fd >= 0 && read(fd, data, sizeof(data)) == (ssize_t)grown);
    for (size_t i = 0; i < grown; ++i) {
        assert(data[i] == (i < keep ? 0xa5 : 0));
    }
    assert(close(fd) == 0);
}

int main(void) {
    const char *path = "./file-growth-check";
    verify_growth(path, 0, block);
    verify_growth(path, block, block * 2 + 19);
    verify_growth(path, block + 13, block + 27);
    verify_growth(path, block - 7, block * 2 + 15);
    assert(unlink(path) == 0);
    puts("PASS file growth preserves prefix and clears new bytes");
    return 0;
}
