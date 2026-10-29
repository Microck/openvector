/* One synchronous zlib 1.2.3 compression of a filtered PNG stream.
 * Native counterpart of tools/bitmap-deflate.c: upstream zlib stays
 * unmodified and the host allocator replaces the per-call wasm arena. */
#include <string.h>
#include "zlib.h"

int ov_deflate(const unsigned char *input, unsigned length, unsigned char *output, unsigned capacity) {
    z_stream stream;
    memset(&stream,0,sizeof stream);
    int status=deflateInit2(&stream,6,Z_DEFLATED,15,8,Z_FILTERED);
    if (status!=Z_OK) return status;
    stream.next_in=(Bytef *)input;stream.avail_in=length;
    stream.next_out=output;stream.avail_out=capacity;
    status=deflate(&stream,Z_FINISH);
    unsigned written=(unsigned)stream.total_out;
    deflateEnd(&stream);
    return status==Z_STREAM_END?(int)written:status;
}
