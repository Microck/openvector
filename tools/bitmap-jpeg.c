/* One synchronous, memory-only IJG compression. Reset reclaims the previous
 * call's arena; individual frees cannot outlive this operation. No Qt code. */
#include <stddef.h>
#include <stdio.h>
#include "jpeglib.h"
#include "jerror.h"

extern unsigned char __heap_base;
static size_t arena;
static int jpeg_error;
static unsigned char *jpeg_output;

void ov_reset(void) { arena=(size_t)&__heap_base;jpeg_error=0;jpeg_output=0; }
int ov_jpeg_error(void) { return jpeg_error; }
unsigned char *ov_jpeg_output(void) { return jpeg_output; }

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

void *malloc(size_t size) {
    size_t start=(arena+7)&~(size_t)7;
    if (start>0x7fffffff || size>0x7fffffff-start) return 0;
    size_t end=start+size,current=__builtin_wasm_memory_size(0)*65536;
    if (end>current && __builtin_wasm_memory_grow(0,(end-current+65535)/65536)==(size_t)-1) return 0;
    arena=end;
    return (void *)start;
}

void free(void *pointer) { (void)pointer; }

static void fail(j_common_ptr context) {
    jpeg_error=context->err->msg_code;
    __builtin_trap();
}

static void message(j_common_ptr context, int level) { (void)context;(void)level; }
static void reset_error(j_common_ptr context) { context->err->num_warnings=0;context->err->msg_code=0; }

typedef struct {
    struct jpeg_destination_mgr manager;
    unsigned char *bytes;
    size_t capacity;
} destination;

static void begin_destination(j_compress_ptr context) {
    destination *output=(destination *)context->dest;
    output->capacity=4096;output->bytes=malloc(output->capacity);
    if (!output->bytes) ERREXIT1(context,JERR_OUT_OF_MEMORY,0);
    output->manager.next_output_byte=output->bytes;
    output->manager.free_in_buffer=output->capacity;
}

static boolean grow_destination(j_compress_ptr context) {
    destination *output=(destination *)context->dest;
    if (output->capacity>0x3fffffff) ERREXIT1(context,JERR_OUT_OF_MEMORY,0);
    unsigned char *expanded=malloc(output->capacity*2);
    if (!expanded) ERREXIT1(context,JERR_OUT_OF_MEMORY,0);
    memcpy(expanded,output->bytes,output->capacity);
    output->manager.next_output_byte=expanded+output->capacity;
    output->manager.free_in_buffer=output->capacity;
    output->bytes=expanded;output->capacity*=2;
    return TRUE;
}

static void end_destination(j_compress_ptr context) { (void)context; }

unsigned ov_jpeg_encode(unsigned char *bgra, unsigned width, unsigned height) {
    struct jpeg_compress_struct context={0};
    struct jpeg_error_mgr errors={0};
    errors.error_exit=fail;errors.emit_message=message;errors.reset_error_mgr=reset_error;
    context.err=&errors;
    jpeg_create_compress(&context);
    destination output={0};
    output.manager.init_destination=begin_destination;
    output.manager.empty_output_buffer=grow_destination;
    output.manager.term_destination=end_destination;
    context.dest=&output.manager;
    context.image_width=width;context.image_height=height;
    context.input_components=3;context.in_color_space=JCS_RGB;
    jpeg_set_defaults(&context);
    context.density_unit=1;context.X_density=96;context.Y_density=96;
    jpeg_set_quality(&context,75,TRUE);
    jpeg_start_compress(&context,TRUE);
    unsigned char *row=malloc(width*3);
    if (!row) ERREXIT1(&context,JERR_OUT_OF_MEMORY,0);
    while (context.next_scanline<height) {
        const unsigned char *source=bgra+context.next_scanline*width*4;
        for (unsigned x=0;x<width;x++) {
            row[x*3]=source[x*4+2];row[x*3+1]=source[x*4+1];row[x*3+2]=source[x*4];
        }
        JSAMPROW scanline=row;
        jpeg_write_scanlines(&context,&scanline,1);
    }
    jpeg_finish_compress(&context);
    unsigned written=output.capacity-output.manager.free_in_buffer;
    jpeg_output=output.bytes;
    jpeg_destroy_compress(&context);
    return written;
}
