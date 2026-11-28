/* Native helper for lmbench lat_proc: the upstream hello writes 12 bytes. */
#include <unistd.h>

int main(void)
{
    (void)write(STDOUT_FILENO, "Hello world\n", 12);
    return 0;
}
