/* Regression: waitpid must deliver a handler that terminates its child.
 * Cross-compile with <arch>-linux-musl-gcc -static and run in the guest.
 * Both cases should finish in about one second each; timeout after 15s.
 */
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>
static volatile sig_atomic_t seen;
static pid_t target;
static void handler(int sig) { (void)sig; seen = 1; kill(target, SIGTERM); }
static int run_case(int restart) {
    seen = 0;
    struct sigaction sa = {0};
    sa.sa_handler = handler;
    sa.sa_flags = restart ? SA_RESTART : 0;
    sigemptyset(&sa.sa_mask);
    if (sigaction(SIGUSR1, &sa, 0)) return 1;
    target = fork();
    if (target < 0) return 1;
    if (!target) { sleep(30); _exit(0); }
    pid_t sender = fork();
    if (sender < 0) { kill(target, SIGKILL); return 1; }
    if (!sender) { sleep(1); kill(getppid(), SIGUSR1); _exit(0); }
    printf("BEGIN waitpid SA_RESTART=%d\n", restart); fflush(stdout);
    int status;
    errno = 0;
    pid_t result = waitpid(target, &status, 0);
    int saved_errno = errno;
    int ok = seen && (restart ? result == target : result == -1 && saved_errno == EINTR);
    while (waitpid(-1, &status, 0) > 0 || errno == EINTR) {}
    printf("%s waitpid SA_RESTART=%d result=%d errno=%d handled=%d\n",
           ok ? "PASS" : "FAIL", restart, result, saved_errno, seen);
    fflush(stdout);
    return !ok;
}
int main(void) { return run_case(0) || run_case(1); }
