#define _GNU_SOURCE
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>

enum { children = 144 };

static void wave(unsigned generation) {
    int ready[2], gate[2];
    pid_t child[children];
    volatile uint32_t *word = mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                                   MAP_ANONYMOUS | MAP_PRIVATE, -1, 0);
    assert(word != MAP_FAILED);
    *word = generation;
    assert(pipe(ready) == 0 && pipe(gate) == 0);
    for (unsigned i = 0; i < children; i++) {
        child[i] = fork();
        assert(child[i] >= 0);
        if (child[i] == 0) {
            const uint32_t expected = generation + i + 1;
            char token = 'R';
            close(gate[1]);
            close(ready[0]);
            assert(*word == generation);
            *word = expected;
            assert(mprotect((void *)word, 4096, PROT_READ) == 0);
            assert(write(ready[1], &token, 1) == 1);
            assert(read(gate[0], &token, 1) == 0);
            assert(*word == expected);
            assert(mprotect((void *)word, 4096, PROT_READ | PROT_WRITE) == 0);
            *word += 1;
            void *old = (void *)word;
            assert(munmap(old, 4096) == 0);
            word = mmap(old, 4096, PROT_READ | PROT_WRITE,
                        MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1, 0);
            assert((void *)word == old && *word == 0);
            _exit(0);
        }
    }
    close(ready[1]);
    close(gate[0]);
    for (unsigned i = 0; i < children; i++) {
        char token;
        assert(read(ready[0], &token, 1) == 1 && token == 'R');
    }
    close(gate[1]);
    for (unsigned i = 0; i < children; i++) {
        int status;
        assert(waitpid(child[i], &status, 0) == child[i]);
        assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
    }
    assert(*word == generation);
    close(ready[0]);
    assert(munmap((void *)word, 4096) == 0);
}

int main(void) {
    wave(1000);
    wave(2000);
    puts("PASS address-space reuse, COW, mprotect and remap");
    return 0;
}
