/* One synchronous, memory-only IJG compression. Native counterpart of
 * tools/bitmap-jpeg.c: the same default profile and scanline conversion, with
 * setjmp error recovery instead of a trap and host allocation instead of the
 * per-call wasm arena. Upstream jpeg-6b files stay unmodified. */
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <setjmp.h>
#include "jpeglib.h"
#include "jerror.h"

typedef struct {
    struct jpeg_error_mgr manager;
    jmp_buf recover;
} failure;

static void fail(j_common_ptr context) {
    longjmp(((failure *)context->err)->recover,1);
}

static void message(j_common_ptr context, int level) { (void)context;(void)level; }
static void reset_error(j_common_ptr context) { context->err->num_warnings=0;context->err->msg_code=0; }

typedef struct {
    struct jpeg_destination_mgr manager;
    unsigned char *bytes;
    size_t capacity;
    unsigned char *row;
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
    unsigned char *expanded=realloc(output->bytes,output->capacity*2);
    if (!expanded) ERREXIT1(context,JERR_OUT_OF_MEMORY,0);
    output->manager.next_output_byte=expanded+output->capacity;
    output->manager.free_in_buffer=output->capacity;
    output->bytes=expanded;output->capacity*=2;
    return TRUE;
}

static void end_destination(j_compress_ptr context) { (void)context; }

void ov_jpeg_free(unsigned char *bytes) { free(bytes); }

/* Returns 0 and an owned buffer, or the IJG message code on failure. */
int ov_jpeg_encode(const unsigned char *bgra, unsigned width, unsigned height,
                   unsigned char **encoded, size_t *length) {
    struct jpeg_compress_struct context;
    failure errors;
    destination output;
    memset(&context,0,sizeof context);memset(&errors,0,sizeof errors);memset(&output,0,sizeof output);
    errors.manager.error_exit=fail;errors.manager.emit_message=message;errors.manager.reset_error_mgr=reset_error;
    context.err=&errors.manager;
    *encoded=0;*length=0;
    if (setjmp(errors.recover)) {
        int code=errors.manager.msg_code;
        jpeg_destroy_compress(&context);
        free(output.bytes);free(output.row);
        return code?code:-1;
    }
    jpeg_create_compress(&context);
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
    output.row=malloc((size_t)width*3);
    if (!output.row) ERREXIT1(&context,JERR_OUT_OF_MEMORY,0);
    while (context.next_scanline<height) {
        const unsigned char *source=bgra+(size_t)context.next_scanline*width*4;
        unsigned char *row=output.row;
        for (unsigned x=0;x<width;x++) {
            row[x*3]=source[x*4+2];row[x*3+1]=source[x*4+1];row[x*3+2]=source[x*4];
        }
        JSAMPROW scanline=row;
        jpeg_write_scanlines(&context,&scanline,1);
    }
    jpeg_finish_compress(&context);
    *length=output.capacity-output.manager.free_in_buffer;
    *encoded=output.bytes;
    jpeg_destroy_compress(&context);
    free(output.row);
    return 0;
}
