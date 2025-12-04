/* Enable POSIX extensions for pread */
#ifndef _MSC_VER
#define _POSIX_C_SOURCE 200809L
#endif

#include "ziprand.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#include <windows.h>
#else
#include <fcntl.h>
#include <unistd.h>
#include <sys/types.h>
#include <sys/stat.h>
#endif

/* File I/O using native handles for thread-safe pread */
typedef struct {
#ifdef _WIN32
    HANDLE handle;
#else
    int fd;
#endif
} file_io_ctx_t;

static int64_t file_read(void* ctx, uint64_t offset, void* buffer, size_t size)
{
    file_io_ctx_t* fctx = ctx;

#ifdef _WIN32
    OVERLAPPED overlapped = {0};
    overlapped.Offset = (DWORD)offset;
    overlapped.OffsetHigh = (DWORD)(offset >> 32);
    
    DWORD bytes_read;
    if (!ReadFile(fctx->handle, buffer, (DWORD)size, &bytes_read, &overlapped)) {
        return -1;
    }
    return (int64_t)bytes_read;
#else
    ssize_t bytes_read = pread(fctx->fd, buffer, size, offset);
    return bytes_read;
#endif
}

static int64_t file_size(void* ctx)
{
    file_io_ctx_t* fctx = ctx;

#ifdef _WIN32
    LARGE_INTEGER size;
    if (!GetFileSizeEx(fctx->handle, &size)) {
        return -1;
    }
    return (int64_t)size.QuadPart;
#else
    struct stat st;
    if (fstat(fctx->fd, &st) < 0) {
        return -1;
    }
    return (int64_t)st.st_size;
#endif
}

static void file_close(void* ctx)
{
    file_io_ctx_t* fctx = ctx;
#ifdef _WIN32
    if (fctx->handle != INVALID_HANDLE_VALUE) {
        CloseHandle(fctx->handle);
    }
#else
    if (fctx->fd >= 0) {
        close(fctx->fd);
    }
#endif
    free(fctx);
}

ziprand_io_t* ziprand_io_file(const char* path)
{
    if (!path)
        return NULL;

    file_io_ctx_t* fctx = malloc(sizeof(file_io_ctx_t));
    if (!fctx)
        return NULL;

#ifdef _WIN32
    fctx->handle = CreateFileA(
        path,
        GENERIC_READ,
        FILE_SHARE_READ,
        NULL,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        NULL
    );
    if (fctx->handle == INVALID_HANDLE_VALUE) {
        free(fctx);
        return NULL;
    }
#else
    fctx->fd = open(path, O_RDONLY);
    if (fctx->fd < 0) {
        free(fctx);
        return NULL;
    }
#endif

    ziprand_io_t* io = malloc(sizeof(ziprand_io_t));
    if (!io) {
#ifdef _WIN32
        CloseHandle(fctx->handle);
#else
        close(fctx->fd);
#endif
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
