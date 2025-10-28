#ifndef ZIPRAND_H
#define ZIPRAND_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Return codes */
typedef enum {
    ZIPRAND_OK = 0,
    ZIPRAND_ERR_IO = -1,
    ZIPRAND_ERR_INVALID_ZIP = -2,
    ZIPRAND_ERR_NOT_FOUND = -3,
    ZIPRAND_ERR_COMPRESSED = -4,
    ZIPRAND_ERR_NOMEM = -5,
    ZIPRAND_ERR_INVALID_PARAM = -6,
    ZIPRAND_ERR_SEEK_BEYOND_END = -7
} ziprand_error_t;

/* I/O callback function types */
typedef struct ziprand_io ziprand_io_t;

/**
 * Read callback - reads data from the source
 * @param io_ctx User-provided context
 * @param offset Absolute offset to read from
 * @param buffer Buffer to read into
 * @param size Number of bytes to read
 * @return Number of bytes read, or -1 on error
 */
typedef int64_t (*ziprand_read_fn)(void* io_ctx, uint64_t offset, void* buffer, size_t size);

/**
 * Get size callback - returns total size of the source
 * @param io_ctx User-provided context
 * @return Total size in bytes, or -1 on error
 */
typedef int64_t (*ziprand_size_fn)(void* io_ctx);

/**
 * Optional close callback - called when ziprand_close() is invoked
 * @param io_ctx User-provided context
 */
typedef void (*ziprand_close_fn)(void* io_ctx);

/* I/O interface structure */
struct ziprand_io {
    void* ctx;                /* User-provided context pointer */
    ziprand_read_fn read;     /* Read function */
    ziprand_size_fn get_size; /* Get size function */
    ziprand_close_fn close;   /* Optional close function (can be NULL) */
};

/* ZIP entry information */
typedef struct {
    char* name;                  /* Entry name (null-terminated) */
    uint64_t compressed_size;    /* Compressed size in bytes */
    uint64_t uncompressed_size;  /* Uncompressed size in bytes */
    uint64_t offset;             /* Offset of local header */
    uint64_t data_offset;        /* Offset of actual data */
    uint16_t compression_method; /* 0 = stored, 8 = deflate, etc. */
} ziprand_entry_t;

/* Main ZIP archive handle */
typedef struct ziprand_archive ziprand_archive_t;

/* ZIP file reader handle */
typedef struct ziprand_file ziprand_file_t;

/**
 * Open a ZIP archive using provided I/O callbacks
 * @param io I/O interface (copied internally)
 * @return Archive handle or NULL on error
 */
ziprand_archive_t* ziprand_open(const ziprand_io_t* io);

/**
 * Close the archive and free all resources
 * @param archive Archive handle
 */
void ziprand_close(ziprand_archive_t* archive);

/**
 * Get number of entries in the archive
 * @param archive Archive handle
 * @return Number of entries, or -1 on error
 */
int64_t ziprand_get_entry_count(ziprand_archive_t* archive);

/**
 * Get entry by index
 * @param archive Archive handle
 * @param index Entry index (0-based)
 * @return Entry information or NULL on error (do not free, owned by archive)
 */
const ziprand_entry_t* ziprand_get_entry_by_index(ziprand_archive_t* archive, size_t index);

/**
 * Find entry by name
 * @param archive Archive handle
 * @param name Entry name to find
 * @return Entry information or NULL if not found (do not free, owned by archive)
 */
const ziprand_entry_t* ziprand_find_entry(ziprand_archive_t* archive, const char* name);

/**
 * Open a file within the archive for reading (only uncompressed files supported)
 * @param archive Archive handle
 * @param entry Entry to open
 * @return File handle or NULL on error
 */
ziprand_file_t* ziprand_fopen(ziprand_archive_t* archive, const ziprand_entry_t* entry);

/**
 * Open a file by name
 * @param archive Archive handle
 * @param name Entry name
 * @return File handle or NULL on error
 */
ziprand_file_t* ziprand_fopen_by_name(ziprand_archive_t* archive, const char* name);

/**
 * Read from current position in file
 * @param file File handle
 * @param buffer Buffer to read into
 * @param size Number of bytes to read
 * @return Number of bytes read, or -1 on error
 */
int64_t ziprand_fread(ziprand_file_t* file, void* buffer, size_t size);

/**
 * Read from specific offset (random access)
 * @param file File handle
 * @param offset Offset within the file
 * @param buffer Buffer to read into
 * @param size Number of bytes to read
 * @return Number of bytes read, or -1 on error
 */
int64_t ziprand_fread_at(ziprand_file_t* file, uint64_t offset, void* buffer, size_t size);

/**
 * Seek to position in file
 * @param file File handle
 * @param offset Offset to seek to
 * @param whence SEEK_SET, SEEK_CUR, or SEEK_END
 * @return New position, or -1 on error
 */
int64_t ziprand_fseek(ziprand_file_t* file, int64_t offset, int whence);

/**
 * Get current position in file
 * @param file File handle
 * @return Current position, or -1 on error
 */
int64_t ziprand_ftell(ziprand_file_t* file);

/**
 * Get size of file
 * @param file File handle
 * @return Size in bytes, or -1 on error
 */
int64_t ziprand_fsize(ziprand_file_t* file);

/**
 * Close file handle
 * @param file File handle
 */
void ziprand_fclose(ziprand_file_t* file);

/**
 * Get last error message
 * @return Error message string (do not free)
 */
const char* ziprand_strerror(ziprand_error_t error);

/* Helper functions for common I/O sources */

/**
 * Create I/O interface for standard file
 * @param path File path
 * @return Allocated I/O interface (must be freed with ziprand_io_free)
 */
ziprand_io_t* ziprand_io_file(const char* path);

/**
 * Create I/O interface from memory buffer
 * @param data Buffer pointer
 * @param size Buffer size
 * @return Allocated I/O interface (must be freed with ziprand_io_free)
 */
ziprand_io_t* ziprand_io_memory(const void* data, size_t size);

/**
 * Free I/O interface created by helper functions
 * @param io I/O interface
 */
void ziprand_io_free(ziprand_io_t* io);

#ifdef __cplusplus
}
#endif

#endif /* ZIPRAND_H */
