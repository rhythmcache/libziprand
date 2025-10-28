/* Enable POSIX extensions for fseeko/ftello */
#ifndef _MSC_VER
#define _POSIX_C_SOURCE 200809L
#endif

#include "ziprand.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef _MSC_VER
#include <sys/types.h>
#endif

typedef struct {
    FILE* fp;
    int should_close;
} file_io_ctx_t;

static int64_t file_read(void* ctx, uint64_t offset, void* buffer, size_t size)
{
    file_io_ctx_t* fctx = ctx;

#ifdef _MSC_VER
    if (_fseeki64(fctx->fp, offset, SEEK_SET) != 0)
        return -1;
#else
    if (fseeko(fctx->fp, offset, SEEK_SET) != 0)
        return -1;
#endif

    return fread(buffer, 1, size, fctx->fp);
}

static int64_t file_size(void* ctx)
{
    file_io_ctx_t* fctx = ctx;

#ifdef _MSC_VER
    int64_t current = _ftelli64(fctx->fp);
    if (current < 0)
        return -1;

    if (_fseeki64(fctx->fp, 0, SEEK_END) != 0)
        return -1;
    int64_t size = _ftelli64(fctx->fp);
    _fseeki64(fctx->fp, current, SEEK_SET);
#else
    off_t current = ftello(fctx->fp);
    if (current < 0)
        return -1;

    if (fseeko(fctx->fp, 0, SEEK_END) != 0)
        return -1;
    off_t size = ftello(fctx->fp);
    fseeko(fctx->fp, current, SEEK_SET);
#endif

    return size;
}

static void file_close(void* ctx)
{
    file_io_ctx_t* fctx = ctx;
    if (fctx->should_close && fctx->fp)
        fclose(fctx->fp);
    free(fctx);
}

ziprand_io_t* ziprand_io_file(const char* path)
{
    FILE* fp = fopen(path, "rb");
    if (!fp)
        return NULL;

    file_io_ctx_t* fctx = malloc(sizeof(file_io_ctx_t));
    if (!fctx) {
        fclose(fp);
        return NULL;
    }

    fctx->fp = fp;
    fctx->should_close = 1;

    ziprand_io_t* io = malloc(sizeof(ziprand_io_t));
    if (!io) {
        fclose(fp);
        free(fctx);
        return NULL;
    }

    io->ctx = fctx;
    io->read = file_read;
    io->get_size = file_size;
    io->close = file_close;

    return io;
}

/* memory I/O implementation */
typedef struct {
    const uint8_t* data;
    size_t size;
} memory_io_ctx_t;

static int64_t memory_read(void* ctx, uint64_t offset, void* buffer, size_t size)
{
    memory_io_ctx_t* mctx = ctx;

    if (offset >= mctx->size)
        return 0;

    size_t remaining = mctx->size - offset;
    size_t to_read = size < remaining ? size : remaining;

    memcpy(buffer, mctx->data + offset, to_read);
    return to_read;
}

static int64_t memory_size(void* ctx)
{
    memory_io_ctx_t* mctx = ctx;
    return mctx->size;
}

static void memory_close(void* ctx)
{
    free(ctx);
}

ziprand_io_t* ziprand_io_memory(const void* data, size_t size)
{
    if (!data || size == 0)
        return NULL;

    memory_io_ctx_t* mctx = malloc(sizeof(memory_io_ctx_t));
    if (!mctx)
        return NULL;

    mctx->data = data;
    mctx->size = size;

    ziprand_io_t* io = malloc(sizeof(ziprand_io_t));
    if (!io) {
        free(mctx);
        return NULL;
    }

    io->ctx = mctx;
    io->read = memory_read;
    io->get_size = memory_size;
    io->close = memory_close;

    return io;
}

void ziprand_io_free(ziprand_io_t* io)
{
    if (!io)
        return;
    if (io->close)
        io->close(io->ctx);
    free(io);
}