#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <sched.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/select.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <unistd.h>

#define CHECK(test) do { if (!(test)) { \
    fprintf(stderr, "FAIL line %d: %s (errno %d)\n", __LINE__, #test, errno); \
    exit(1); \
} } while (0)

static atomic_int worker_tid;
static atomic_int inspected;

static int ready(int nfds, fd_set *readable, fd_set *writable, fd_set *exceptional) {
    struct timespec zero = {0};
    return syscall(SYS_pselect6, nfds, readable, writable, exceptional, &zero, NULL);
}

static void *private_table(void *unused) {
    (void)unused;
    CHECK(syscall(SYS_close_range, 120, 123, 2 /* CLOSE_RANGE_UNSHARE */) == 0);
    CHECK(fcntl(120, F_GETFD) == -1 && errno == EBADF);
    atomic_store(&worker_tid, syscall(SYS_gettid));
    while (!atomic_load(&inspected)) sched_yield();
    return NULL;
}

static int exec_child(void *program) {
    execl(program, program, "check-exec", (char *)NULL);
    _exit(3);
}

int main(int argc, char **argv) {
    if (argc > 1 && !strcmp(argv[1], "check-exec")) {
        CHECK(fcntl(121, F_GETFD) == -1 && errno == EBADF);
        CHECK(fcntl(120, F_GETFD) == 0);
        return 0;
    }
    int f = open("/tmp/lmbench-fds", O_RDWR | O_CREAT, 0600);
    int p[2];
    CHECK(f >= 0 && f < 64 && pipe(p) == 0);
    CHECK(dup2(f, 120) == 120 && dup3(f, 121, O_CLOEXEC) == 121);
    fd_set r, w, e;
    FD_ZERO(&r);
    FD_SET(120, &r);
    FD_SET(p[0], &r);
    w = e = r;
    CHECK(ready(121, &r, &w, &e) == 2);
    CHECK(FD_ISSET(120, &r) && FD_ISSET(120, &w) && !FD_ISSET(p[0], &r) && !FD_ISSET(120, &e));

    CHECK(dup2(p[0], 120) == 120);
    FD_ZERO(&r);
    FD_SET(120, &r);
    CHECK(ready(121, &r, NULL, NULL) == 0 && !FD_ISSET(120, &r));
    CHECK(write(p[1], "x", 1) == 1);
    FD_SET(120, &r);
    CHECK(ready(121, &r, NULL, NULL) == 1 && FD_ISSET(120, &r));
    CHECK(close(120) == 0);
    FD_SET(f, &r);
    CHECK(ready(121, &r, NULL, NULL) == -1 && errno == EBADF);
    CHECK(dup2(f, 120) == 120 && dup2(f, 1023) == 1023);
    FD_ZERO(&r);
    FD_SET(1023, &r);
    CHECK(ready(1024, &r, NULL, NULL) == 1);
    CHECK(close(1023) == 0);

    size_t page = sysconf(_SC_PAGESIZE);
    char *mem = mmap(NULL, 2 * page, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    CHECK(mem != MAP_FAILED && mprotect(mem + page, page, PROT_NONE) == 0);
    unsigned long *word = (unsigned long *)(mem + page - sizeof(*word));
    *word = 1UL << f;
    CHECK(ready(f + 1, (fd_set *)word, NULL, NULL) == 1 && *word == (1UL << f));
    CHECK(ready(0, (fd_set *)(mem + page), NULL, NULL) == 0);
    CHECK(munmap(mem, 2 * page) == 0);

    char *stack = malloc(65536);
    CHECK(stack != NULL);
    int status;
    pid_t child = clone(exec_child, stack + 65536, CLONE_FILES | SIGCHLD, argv[0]);
    CHECK(child > 0 && waitpid(child, &status, 0) == child && status == 0);
    free(stack);
    CHECK(fcntl(121, F_GETFD) == FD_CLOEXEC);

    pthread_t worker;
    CHECK(pthread_create(&worker, NULL, private_table, NULL) == 0);
    int tid;
    while (!(tid = atomic_load(&worker_tid))) sched_yield();
    CHECK(fcntl(120, F_GETFD) == 0);
    int pidfd = syscall(SYS_pidfd_open, tid, 128 /* PIDFD_THREAD */);
    CHECK(pidfd >= 0);
    CHECK(syscall(SYS_pidfd_getfd, pidfd, 120, 0) == -1 && errno == EBADF);
    int copy = syscall(SYS_pidfd_getfd, pidfd, f, 0);
    CHECK(copy >= 0 && close(copy) == 0 && close(pidfd) == 0);
    atomic_store(&inspected, 1);
    CHECK(pthread_join(worker, NULL) == 0);
    puts("PASS fd table/select");
    return 0;
}
