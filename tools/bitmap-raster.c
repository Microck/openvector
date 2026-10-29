/* FreeType performs coverage; this wrapper supplies only owned memory views. */
#include <stddef.h>
#include "qgrayraster.c"

void *memset(void *target, int value, size_t length) {
    unsigned char *bytes=target;
    for (size_t index=0;index<length;index++) bytes[index]=(unsigned char)value;
    return target;
}

typedef struct {
    unsigned char *pixels;
    int width;
} Coverage;

/* The allocator's reference quota is independent of the worker's C layout. */
_Static_assert(sizeof(TWorker)<=gray_control_bytes && sizeof(TCell)==16,
               "The coverage kernel must fit the reference allocator contract.");

static void write_spans(int count, const QT_FT_Span *spans, void *user) {
    Coverage *coverage=user;
    for (int index=0;index<count;index++) {
        const QT_FT_Span *span=spans+index;
        memset(coverage->pixels+span->y*coverage->width+span->x,span->coverage,span->len);
    }
}

int ov_coverage(int width, int height, QT_FT_Vector *points, char *tags, int point_count,
                int *contours, int contour_count, int even_odd, unsigned char *pixels,
                char *pool, int pool_size) {
    TRaster raster={0};
    QT_FT_Outline outline={0};
    QT_FT_Raster_Params parameters={0};
    Coverage coverage={pixels,width};
    outline.n_points=point_count;outline.n_contours=contour_count;
    outline.points=points;outline.tags=tags;outline.contours=contours;
    outline.flags=even_odd?QT_FT_OUTLINE_EVEN_ODD_FILL:QT_FT_OUTLINE_NONE;
    parameters.source=&outline;
    parameters.flags=QT_FT_RASTER_FLAG_AA|QT_FT_RASTER_FLAG_DIRECT|QT_FT_RASTER_FLAG_CLIP;
    parameters.gray_spans=write_spans;parameters.user=&coverage;
    parameters.clip_box.xMax=width;parameters.clip_box.yMax=height;
    gray_raster_reset((QT_FT_Raster)&raster,pool,pool_size);
    return gray_raster_render(&raster,&parameters);
}
