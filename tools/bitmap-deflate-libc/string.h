/* Freestanding byte operations supplied by bitmap-deflate.c. */
#include <stddef.h>
void *memcpy(void *target, const void *source, size_t size);
void *memset(void *target, int value, size_t size);
int memcmp(const void *left, const void *right, size_t size);
