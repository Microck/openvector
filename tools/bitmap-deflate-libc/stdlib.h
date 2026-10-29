/* Freestanding allocation interface supplied by bitmap-deflate.c. */
#include <stddef.h>
void *malloc(size_t size);
void *calloc(size_t count, size_t size);
void free(void *pointer);
