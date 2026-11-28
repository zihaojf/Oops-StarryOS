/* Filesystem and blocking-I/O regressions for lmbench performance changes.
 * Compile with <arch>-linux-{gnu,musl}-gcc -O2 -fno-pie -no-pie -static -pthread and run in the guest.
 */
#define _GNU_SOURCE
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <setjmp.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>
static void *thread_main(void *arg) { return arg; }
/* A hardware exception belongs to the faulting thread even when other
 * threads in the same process have the signal unblocked. */
static _Thread_local sigjmp_buf fault_env;
static _Thread_local volatile sig_atomic_t awaiting_fault;

static void handle_test_fault(int signo)
{
    if (signo != SIGSEGV || !awaiting_fault)
        _exit(100);
    siglongjmp(fault_env, 1);
}

static void *check_fault_thread(void *arg)
{
    if (sigsetjmp(fault_env, 1) == 0) {
        awaiting_fault = 1;
        *(volatile char *)arg = 1;
        _exit(101);
    }
    awaiting_fault = 0;
    return arg;
}

static void test_fault_delivery(void)
{
    long bytes = sysconf(_SC_PAGESIZE);
    assert(bytes > 0);
    void *page = mmap(NULL, bytes, PROT_READ, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(page != MAP_FAILED);
    struct sigaction action = { .sa_handler = handle_test_fault }, saved;
    sigemptyset(&action.sa_mask);
    assert(sigaction(SIGSEGV, &action, &saved) == 0);
    pthread_t worker;
    void *result = NULL;
    assert(pthread_create(&worker, NULL, check_fault_thread, page) == 0);
    assert(pthread_join(worker, &result) == 0 && result == page);
    assert(sigaction(SIGSEGV, &saved, NULL) == 0);
    assert(munmap(page, bytes) == 0);
    puts("PASS synchronous fault delivered to originating thread");
}

static _Thread_local volatile sig_atomic_t received_usr1;

static void note_usr1(int sig)
{
    if (sig == SIGUSR1)
        received_usr1 = 1;
}

static void *wait_for_process_signal(void *arg)
{
    int fd = *(int *)arg;
    sigset_t mask;
    assert(pthread_sigmask(SIG_SETMASK, NULL, &mask) == 0);
    sigdelset(&mask, SIGUSR1);
    assert(write(fd, "r", 1) == 1);
    while (!received_usr1) {
        assert(sigsuspend(&mask) == -1 && errno == EINTR);
    }
    return arg;
}

static void test_self_kill(void)
{
    struct sigaction action = { .sa_handler = note_usr1 }, old;
    sigemptyset(&action.sa_mask);
    assert(sigaction(SIGUSR1, &action, &old) == 0);
    sigset_t set, saved, pending;
    sigemptyset(&set);
    sigaddset(&set, SIGUSR1);
    assert(pthread_sigmask(SIG_UNBLOCK, &set, &saved) == 0);
    assert(kill(getpid(), 0) == 0);
    assert(kill(getpid(), SIGUSR1) == 0 && received_usr1);
    received_usr1 = 0;
    assert(pthread_sigmask(SIG_BLOCK, &set, NULL) == 0);
    assert(kill(getpid(), SIGUSR1) == 0 && !received_usr1);
    assert(sigpending(&pending) == 0 && sigismember(&pending, SIGUSR1));
    assert(pthread_sigmask(SIG_UNBLOCK, &set, NULL) == 0 && received_usr1);
    assert(pthread_sigmask(SIG_BLOCK, &set, NULL) == 0);
    received_usr1 = 0;
    int ready[2];
    assert(pipe(ready) == 0);
    pthread_t thread;
    assert(pthread_create(&thread, NULL, wait_for_process_signal, &ready[1]) == 0);
    char marker;
    assert(read(ready[0], &marker, 1) == 1 && marker == 'r');
    assert(kill(getpid(), SIGUSR1) == 0);
    void *result;
    assert(pthread_join(thread, &result) == 0 && result == &ready[1]);
    assert(!received_usr1);
    close(ready[0]);
    close(ready[1]);
    assert(pthread_sigmask(SIG_SETMASK, &saved, NULL) == 0);
    assert(sigaction(SIGUSR1, &old, NULL) == 0);
    puts("PASS self kill, blocked pending signal and delivery to another thread");
}

int main(void) {
    const char *path="/var/tmp/starry-cache-test";
    const char *alias="/var/tmp/starry-cache-link";
    const char *renamed="/var/tmp/starry-cache-renamed";
    unlink(path); unlink(alias); unlink(renamed);
    int fd=open(path,O_CREAT|O_TRUNC|O_RDWR,0600); assert(fd>=0);
    assert(write(fd,"abc",3)==3);
    struct stat st, other;
    assert(fstat(fd,&st)==0 && st.st_size==3);
    assert(fstat(fd,&st)==0 && st.st_size==3);
    int fd2=open(path,O_RDWR); assert(fd2>=0);
    assert(ftruncate(fd2,9)==0);
    assert(fstat(fd,&st)==0 && st.st_size==9);
    assert(chmod(path,0640)==0);
    assert(fstat(fd,&st)==0 && (st.st_mode&0777)==0640);
    int pathfd=open(path,O_PATH); assert(pathfd>=0);
    assert(fstat(pathfd,&other)==0 && other.st_size==9 && (other.st_mode&0777)==0640);
    assert(fstatat(pathfd,"",&other,AT_EMPTY_PATH)==0 && other.st_ino==st.st_ino);
    close(pathfd);
    assert(link(path,alias)==0);
    assert(fstat(fd,&st)==0 && st.st_nlink==2);
    assert(stat(alias,&other)==0 && other.st_nlink==2 && other.st_ino==st.st_ino);
    assert(unlink(alias)==0);
    assert(fstat(fd,&st)==0 && st.st_nlink==1);
    struct timespec times[2]={{1000,123},{2000,456}};
    assert(utimensat(AT_FDCWD,path,times,0)==0);
    assert(fstat(fd,&st)==0 && st.st_mtim.tv_sec==2000 && st.st_mtim.tv_nsec==456);
    assert(rename(path,renamed)==0);
    assert(stat(renamed,&other)==0 && other.st_ino==st.st_ino);
    unsigned char *mapping=mmap(0,4096,PROT_READ,MAP_SHARED,fd,0);
    assert(mapping!=MAP_FAILED);
    assert(memcmp(mapping,"abc",3)==0);
    for(int i=3;i<4096;i++) assert(mapping[i]==0);
    assert(munmap(mapping,4096)==0);
    close(fd2); close(fd); unlink(renamed);
    puts("PASS metadata invalidation and EOF zero fill"); fflush(stdout);
    int pipefd[2]; assert(pipe(pipefd)==0);
    unsigned char *memory=malloc(128*1024); assert(memory);
    memset(memory,0x5a,128*1024);
    pid_t child=fork(); assert(child>=0);
    if(!child) {
        close(pipefd[0]);
        memory[0]=1; memory[128*1024-1]=2;
        struct timespec delay={0,20000000}; nanosleep(&delay,0);
        assert(write(pipefd[1],"x",1)==1);
        close(pipefd[1]); _exit(0);
    }
    close(pipefd[1]); char byte=0;
    assert(read(pipefd[0],&byte,1)==1 && byte=='x');
    assert(read(pipefd[0],&byte,1)==0);
    close(pipefd[0]);
    int status; assert(waitpid(child,&status,0)==child && WIFEXITED(status) && WEXITSTATUS(status)==0);
    assert(memory[0]==0x5a && memory[128*1024-1]==0x5a); free(memory);
    puts("PASS pipe wakeup and copy on write"); fflush(stdout);
    /* No caller wakes this private futex. A premature second poll must
       not turn an unchanged word into a successful wait. */
    int word = 0;
    const int futex_wait_private = 128;
    struct timespec timeout = { .tv_nsec = 20000000 };
    assert(syscall(SYS_futex, &word, futex_wait_private, 0, &timeout, 0, 0) == -1);
    assert(errno == ETIMEDOUT);
    const int futex_wake_private = 129;
    assert(syscall(SYS_futex, &word, futex_wake_private, 1, 0, 0, 0) == 0);
    for (int i = 0; i < 256; ++i) {
        pthread_t thread;
        void *result = NULL;
        assert(pthread_create(&thread, NULL, thread_main, &word) == 0);
        assert(pthread_join(thread, &result) == 0 && result == &word);
    }
    puts("PASS futex timeout and 256 pthread joins");
    test_fault_delivery();
    test_self_kill();
    return 0;
}
