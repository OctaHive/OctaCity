/* Test-only Linux AArch64 process-limit probe; no fork bomb.
 * At most twelve children wait on a pipe, making their lifetimes overlap.
 * The parent releases and reaps every child before reporting UID and count.
 * Linux syscall ABI: pipe2=59, clone=220, wait4=260, getuid=174.
 */
static long call(long number, long a, long b, long c, long d, long e) {
  register long x0 __asm__("x0") = a;
  register long x1 __asm__("x1") = b;
  register long x2 __asm__("x2") = c;
  register long x3 __asm__("x3") = d;
  register long x4 __asm__("x4") = e;
  register long x8 __asm__("x8") = number;
  __asm__ volatile("svc 0" : "+r"(x0) : "r"(x1), "r"(x2), "r"(x3), "r"(x4), "r"(x8) : "memory");
  return x0;
}

static void finish(long code) {
  call(93, code, 0, 0, 0, 0);
  for (;;) {}
}

static void print_number(unsigned long number) {
  char bytes[32];
  long start = sizeof(bytes) - 1;
  bytes[start] = '\n';
  do {
    bytes[--start] = '0' + number % 10;
    number /= 10;
  } while (number);
  if (call(64, 1, (long)(bytes + start), sizeof(bytes) - start, 0, 0) != sizeof(bytes) - start) finish(83);
}

void _start(void) {
  int pipe[2];
  if (call(59, (long)pipe, 0, 0, 0, 0) < 0) finish(81);
  long children = 0;
  for (long attempt = 0; attempt < 12; ++attempt) {
    long pid = call(220, 17, 0, 0, 0, 0); /* SIGCHLD, independent address space. */
    if (pid < 0) break;
    if (pid == 0) {
      call(57, pipe[1], 0, 0, 0, 0);
      char released;
      long result;
      do { result = call(63, pipe[0], (long)&released, 1, 0, 0); } while (result == -4);
      finish(result == 1 ? 0 : 82);
    }
    ++children;
  }
  call(57, pipe[0], 0, 0, 0, 0);
  char releases[12] = {0};
  if (children && call(64, pipe[1], (long)releases, children, 0, 0) != children) finish(84);
  call(57, pipe[1], 0, 0, 0, 0);
  for (long child = 0; child < children; ++child) {
    long result;
    do { result = call(260, -1, 0, 0, 0, 0); } while (result == -4);
    if (result < 0) finish(85);
  }
  print_number(call(174, 0, 0, 0, 0, 0));
  print_number(children);
  long no_new_privileges = call(167, 39, 0, 0, 0, 0); /* PR_GET_NO_NEW_PRIVS. */
  if (no_new_privileges < 0) finish(86);
  struct { unsigned int version; int pid; } header = {0x20080522, 0};
  struct { unsigned int effective, permitted, inheritable; } data[2] = {{0}};
  if (call(90, (long)&header, (long)data, 0, 0, 0) < 0) finish(87);
  print_number(no_new_privileges);
  print_number(((unsigned long)data[1].effective << 32) | data[0].effective);
  finish(0);
}
