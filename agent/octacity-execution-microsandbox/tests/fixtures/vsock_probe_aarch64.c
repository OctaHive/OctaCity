/* Test-only Linux AArch64 transport probe; not an installed reader.
 * Linux syscall ABI: openat=56, close=57, read=63, write=64, exit=93,
 * socket=198, connect=203. AF_VSOCK=40, host CID=2.
 * Reproduction: docs/testing/backend-contracts.md.
 */
typedef unsigned short u16;
typedef unsigned int u32;
struct address { u16 family, reserved; u32 port, cid; unsigned char flags, zero[3]; };
static long call(long number,long a,long b,long c) {
  register long x0 __asm__("x0")=a;
  register long x1 __asm__("x1")=b;
  register long x2 __asm__("x2")=c;
  register long x8 __asm__("x8")=number;
  __asm__ volatile("svc 0" : "+r"(x0) : "r"(x1), "r"(x2), "r"(x8) : "memory");
  return x0;
}
void _start(void) {
  char response[64];
  long bypass=call(56,-100,(long)"/workspace/source/input",0);
  if(bypass>=0){call(57,bypass,0,0);call(93,43,0,0);for(;;){}}
  struct address address={40,0,5001,2,0,{0,0,0}};
  long socket=call(198,40,1,0);
  long result=80;
  if (socket>=0) {
    long connected=call(203,socket,(long)&address,sizeof(address));
    result=connected<0 ? 100-connected : 81;
    if (connected==0 && call(64,socket,(long)"PING",4)==4) {
      /* This fixture expects the file "backend-only-source".
       * Stream fragmentation must not change its independently known result. */
      const long expected=sizeof("backend-only-source")-1;
      long received=0;
      while (received<expected) {
        long length=call(63,socket,(long)(response+received),expected-received);
        if (length==-4) continue; /* EINTR */
        if (length<=0) break;
        received+=length;
      }
      result=82;
      if (received==expected && call(64,1,(long)response,received)==received) result=0;
    }
  }
  if(socket>=0)call(57,socket,0,0);
  call(93,result,0,0);
  for(;;){}
}
