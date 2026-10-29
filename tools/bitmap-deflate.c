/* Host-independent arena for one synchronous PNG compression at a time.
 * Upstream zlib stays unmodified. Reset releases the whole previous call;
 * individual zlib frees need no bookkeeping inside this per-call arena. */
#include <stddef.h>
#include "zlib.h"

extern unsigned char __heap_base;
static size_t arena;

void ov_reset(void) { arena=(size_t)&__heap_base; }

void *memset(void *target, int value, size_t size) {
    unsigned char *bytes=target;
    for (size_t index=0;index<size;index++) bytes[index]=(unsigned char)value;
    return target;
}

void *memcpy(void *target, const void *source, size_t size) {
    unsigned char *output=target;const unsigned char *input=source;
    for (size_t index=0;index<size;index++) output[index]=input[index];
    return target;
}

int memcmp(const void *left, const void *right, size_t size) {
    const unsigned char *first=left,*second=right;
    for (size_t index=0;index<size;index++) {
        if (first[index]!=second[index]) return first[index]-second[index];
    }
    return 0;
}

void *malloc(size_t size) {
    size_t start=(arena+7)&~(size_t)7;
    if (start>0x7fffffff || size>0x7fffffff-start) return 0;
    size_t end=start+size,current=__builtin_wasm_memory_size(0)*65536;
    if (end>current && __builtin_wasm_memory_grow(0,(end-current+65535)/65536)==(size_t)-1) return 0;
    arena=end;
    return (void *)start;
}

void *calloc(size_t count, size_t size) {
    if (count && size>0x7fffffff/count) return 0;
    void *pointer=malloc(count*size);
    return pointer?memset(pointer,0,count*size):0;
}

void free(void *pointer) { (void)pointer; }

int ov_deflate(unsigned char *input, unsigned length, unsigned char *output, unsigned capacity) {
    z_stream stream={0};
    int status=deflateInit2(&stream,6,Z_DEFLATED,15,8,Z_FILTERED);
    if (status!=Z_OK) return status;
    stream.next_in=input;stream.avail_in=length;
    stream.next_out=output;stream.avail_out=capacity;
    status=deflate(&stream,Z_FINISH);
    unsigned written=stream.total_out;
    deflateEnd(&stream);
    return status==Z_STREAM_END?(int)written:status;
}
