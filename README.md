# libziprand

A lightweight, callback-based C library for random access to uncompressed files within ZIP archives.

> Core concept adapted from [rhythmcache/payload-dumper-rust](https://github.com/rhythmcache/payload-dumper-rust)  
> Redesigned as a general-purpose C library with pluggable I/O backends.

## Key Features

 **Source-agnostic I/O** - Works with any data source: files, HTTP, memory, cloud storage, custom protocols  
 **True random access** - Seek and read from any position without decompression   
 **ZIP64 support** - Handle files and archives > 4GB  
 **Zero core dependencies** - Only stdlib for core library
 **Thread-safe** - Multiple archive handles can be used concurrently  

## Limitations

- **Uncompressed files only** - Supports compression method 0 (stored) only
- **Read-only** - No write or modification support
- **No streaming decompression** - DEFLATE and other compression methods not supported

---

## Table of Contents

1. [Quick Start](#quick-start)
2. [I/O Callbacks](#io-callbacks)
3. [API Reference](#api-reference)
4. [Usage Examples](#usage-examples)
5. [HTTP Support](#http-support)
6. [Error Handling](#error-handling)
7. [Performance Tips](#performance-tips)

---

## Quick Start

### Basic Usage (Local File)

```c
#include "ziprand.h"

int main() {
    // open zip file using built-in helper
    ziprand_io_t *io = ziprand_io_file("archive.zip");
    ziprand_archive_t *archive = ziprand_open(io);
    
    // find and open a file
    const ziprand_entry_t *entry = ziprand_find_entry(archive, "data.txt");
    ziprand_file_t *file = ziprand_fopen(archive, entry);
    
    // read data
    char buffer[1024];
    int64_t bytes = ziprand_fread(file, buffer, sizeof(buffer));
    
    // random access
    ziprand_fseek(file, 100, SEEK_SET);
    ziprand_fread(file, buffer, 50);
    
    // cleanup
    ziprand_fclose(file);
    ziprand_close(archive);
    ziprand_io_free(io);
    
    return 0;
}
```
---

## I/O Callbacks

### Callback Interface

```c
typedef struct ziprand_io {
    void *ctx;                          // context pointer
    ziprand_read_fn read;               // read function
    ziprand_size_fn get_size;           // get size function
    ziprand_close_fn close;             // optional cleanup
} ziprand_io_t;
```

### Callback Functions

```c
// read data from source
typedef int64_t (*ziprand_read_fn)(
    void *io_ctx,           // your context
    uint64_t offset,        // absolute offset to read from
    void *buffer,           // buffer to fill
    size_t size             // bytes to read
);

// get total size of source
typedef int64_t (*ziprand_size_fn)(void *io_ctx);

// optional cleanup
typedef void (*ziprand_close_fn)(void *io_ctx);
```

### Example: Custom I/O Backend

```c
typedef struct {
    FILE *fp;
} my_io_ctx_t;

int64_t my_read(void *ctx, uint64_t offset, void *buffer, size_t size) {
    my_io_ctx_t *io = ctx;
    fseek(io->fp, offset, SEEK_SET);
    return fread(buffer, 1, size, io->fp);
}

int64_t my_size(void *ctx) {
    my_io_ctx_t *io = ctx;
    fseek(io->fp, 0, SEEK_END);
    return ftell(io->fp);
}

void my_close(void *ctx) {
    my_io_ctx_t *io = ctx;
    fclose(io->fp);
    free(io);
}

// use it
my_io_ctx_t *ctx = malloc(sizeof(my_io_ctx_t));
ctx->fp = fopen("file.zip", "rb");

ziprand_io_t io = {
    .ctx = ctx,
    .read = my_read,
    .get_size = my_size,
    .close = my_close
};

ziprand_archive_t *archive = ziprand_open(&io);
```

---

## API Reference

### Error Codes

```c
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
```

### Entry Structure

```c
typedef struct {
    char *name;                     // entry name (null-terminated)
    uint64_t compressed_size;       // compressed size
    uint64_t uncompressed_size;     // uncompressed size
    uint64_t offset;                // local header offset
    uint64_t data_offset;           // actual data offset
    uint16_t compression_method;    // 0 = stored, 8 = deflate
} ziprand_entry_t;
```

### Archive Functions

#### `ziprand_open`
```c
ziprand_archive_t *ziprand_open(const ziprand_io_t *io);
```
Opens a ZIP archive using provided I/O callbacks.

**Returns:** Archive handle or NULL on error

---

#### `ziprand_close`
```c
void ziprand_close(ziprand_archive_t *archive);
```
Closes archive and frees resources. Also calls `io->close()` if provided.

---

#### `ziprand_get_entry_count`
```c
int64_t ziprand_get_entry_count(ziprand_archive_t *archive);
```
Returns number of entries in the archive.

---

#### `ziprand_get_entry_by_index`
```c
const ziprand_entry_t *ziprand_get_entry_by_index(
    ziprand_archive_t *archive, 
    size_t index
);
```
Gets entry by index (0-based). **Do not free** - owned by archive.

---

#### `ziprand_find_entry`
```c
const ziprand_entry_t *ziprand_find_entry(
    ziprand_archive_t *archive,
    const char *name
);
```
Finds entry by name (case-sensitive). **Do not free** - owned by archive.

---

### File Functions

#### `ziprand_fopen`
```c
ziprand_file_t *ziprand_fopen(
    ziprand_archive_t *archive,
    const ziprand_entry_t *entry
);
```
Opens a file for reading. Only works with uncompressed entries.

---

#### `ziprand_fopen_by_name`
```c
ziprand_file_t *ziprand_fopen_by_name(
    ziprand_archive_t *archive,
    const char *name
);
```
Opens a file by name (convenience wrapper).

---

#### `ziprand_fclose`
```c
void ziprand_fclose(ziprand_file_t *file);
```
Closes file handle.

---

#### `ziprand_fread`
```c
int64_t ziprand_fread(
    ziprand_file_t *file,
    void *buffer,
    size_t size
);
```
Reads from current position. Advances position.

**Returns:** Bytes read, or -1 on error

---

#### `ziprand_fread_at`
```c
int64_t ziprand_fread_at(
    ziprand_file_t *file,
    uint64_t offset,
    void *buffer,
    size_t size
);
```
Reads from specific offset. **Does not** change position.

**Returns:** Bytes read, or -1 on error

---

#### `ziprand_fseek`
```c
int64_t ziprand_fseek(
    ziprand_file_t *file,
    int64_t offset,
    int whence
);
```
Seeks to position. `whence` is `SEEK_SET`, `SEEK_CUR`, or `SEEK_END`.

**Returns:** New position, or -1 on error

---

#### `ziprand_ftell`
```c
int64_t ziprand_ftell(ziprand_file_t *file);
```
Returns current position.

---

#### `ziprand_fsize`
```c
int64_t ziprand_fsize(ziprand_file_t *file);
```
Returns file size.

---

### Helper Functions

#### `ziprand_io_file`
```c
ziprand_io_t *ziprand_io_file(const char *path);
```
Creates I/O interface for local file. Must free with `ziprand_io_free()`.

---

#### `ziprand_io_memory`
```c
ziprand_io_t *ziprand_io_memory(const void *data, size_t size);
```
Creates I/O interface for memory buffer. Must free with `ziprand_io_free()`.

---

#### `ziprand_io_free`
```c
void ziprand_io_free(ziprand_io_t *io);
```
Frees I/O interface created by helpers.

---

#### `ziprand_strerror`
```c
const char *ziprand_strerror(ziprand_error_t error);
```
Converts error code to string.

---

## Usage Examples

### Example 1: List All Files

```c
#include "ziprand.h"
#include <stdio.h>

int main() {
    ziprand_io_t *io = ziprand_io_file("archive.zip");
    ziprand_archive_t *archive = ziprand_open(io);
    
    if (!archive) {
        fprintf(stderr, "Failed to open ZIP\n");
        return 1;
    }
    
    int64_t count = ziprand_get_entry_count(archive);
    printf("Archive contains %lld entries:\n\n", (long long)count);
    
    for (size_t i = 0; i < count; i++) {
        const ziprand_entry_t *entry = ziprand_get_entry_by_index(archive, i);
        printf("[%zu] %s\n", i, entry->name);
        printf("     Size: %llu bytes\n", 
               (unsigned long long)entry->uncompressed_size);
        printf("     Compression: %s\n", 
               entry->compression_method == 0 ? "none" : "compressed");
    }
    
    ziprand_close(archive);
    ziprand_io_free(io);
    return 0;
}
```

### Example 2: Extract Specific File

```c
#include "ziprand.h"
#include <stdio.h>

int main() {
    ziprand_io_t *io = ziprand_io_file("archive.zip");
    ziprand_archive_t *archive = ziprand_open(io);
    
    ziprand_file_t *file = ziprand_fopen_by_name(archive, "readme.txt");
    if (!file) {
        fprintf(stderr, "File not found or compressed\n");
        ziprand_close(archive);
        ziprand_io_free(io);
        return 1;
    }
    
    // extract to disk
    FILE *out = fopen("readme.txt", "wb");
    char buffer[8192];
    
    while (1) {
        int64_t bytes = ziprand_fread(file, buffer, sizeof(buffer));
        if (bytes <= 0) break;
        fwrite(buffer, 1, bytes, out);
    }
    
    fclose(out);
    ziprand_fclose(file);
    ziprand_close(archive);
    ziprand_io_free(io);
    
    printf("Extracted successfully\n");
    return 0;
}
```

### Example 3: Random Access Reading

```c
#include "ziprand.h"
#include <stdio.h>

int main() {
    ziprand_io_t *io = ziprand_io_file("archive.zip");
    ziprand_archive_t *archive = ziprand_open(io);
    ziprand_file_t *file = ziprand_fopen_by_name(archive, "data.bin");
    
    if (!file) {
        fprintf(stderr, "File not found\n");
        return 1;
    }
    
    int64_t size = ziprand_fsize(file);
    printf("File size: %lld bytes\n", (long long)size);
    
    // read first 4 bytes
    char magic[4];
    ziprand_fread(file, magic, 4);
    printf("Magic: %.4s\n", magic);
    
    // jump to middle
    ziprand_fseek(file, size / 2, SEEK_SET);
    char middle[100];
    ziprand_fread(file, middle, 100);
    printf("Read 100 bytes from middle\n");
    
    // jump to specific offset (no position change)
    char chunk[50];
    ziprand_fread_at(file, 1000, chunk, 50);
    printf("Read 50 bytes from offset 1000\n");
    printf("Current position: %lld\n", (long long)ziprand_ftell(file));
    
    ziprand_fclose(file);
    ziprand_close(archive);
    ziprand_io_free(io);
    
    return 0;
}
```

### Example 4: Memory-Mapped ZIP

```c
#include "ziprand.h"
#include <sys/mman.h>
#include <sys/stat.h>
#include <fcntl.h>

int main() {
    // memory-map the ZIP file
    int fd = open("archive.zip", O_RDONLY);
    struct stat st;
    fstat(fd, &st);
    
    void *mapped = mmap(NULL, st.st_size, PROT_READ, MAP_PRIVATE, fd, 0);
    close(fd);
    
    // use memory I/O
    ziprand_io_t *io = ziprand_io_memory(mapped, st.st_size);
    ziprand_archive_t *archive = ziprand_open(io);
    
    // access files...
    ziprand_file_t *file = ziprand_fopen_by_name(archive, "data.bin");
    // ...
    
    ziprand_fclose(file);
    ziprand_close(archive);
    ziprand_io_free(io);
    
    munmap(mapped, st.st_size);
    return 0;
}
```

---

## Error Handling

### Checking Errors

```c
ziprand_archive_t *archive = ziprand_open(io);
if (!archive) {
    fprintf(stderr, "Failed to open ZIP\n");
    ziprand_io_free(io);
    return 1;
}

ziprand_file_t *file = ziprand_fopen_by_name(archive, "data.bin");
if (!file) {
    // file not found or compressed
    ziprand_close(archive);
    ziprand_io_free(io);
    return 1;
}

int64_t bytes = ziprand_fread(file, buffer, size);
if (bytes < 0) {
    fprintf(stderr, "Read error\n");
}
```

### Common Errors

| Error | Cause | Solution |
|-------|-------|----------|
| `ZIPRAND_ERR_IO` | I/O operation failed | Check I/O callbacks |
| `ZIPRAND_ERR_INVALID_ZIP` | Invalid ZIP format | Verify file is valid ZIP |
| `ZIPRAND_ERR_NOT_FOUND` | Entry doesn't exist | Check entry name |
| `ZIPRAND_ERR_COMPRESSED` | File is compressed | Only stored files supported |
| `ZIPRAND_ERR_NOMEM` | Out of memory | Check available memory |

---

## Performance Tips

### 1. Reuse Archive Handles

```c
// good -> open once
ziprand_archive_t *archive = ziprand_open(io);
for (int i = 0; i < 100; i++) {
    ziprand_file_t *file = ziprand_fopen_by_name(archive, files[i]);
    // ... read ...
    ziprand_fclose(file);
}
ziprand_close(archive);

// bad -> reopen every time
for (int i = 0; i < 100; i++) {
    ziprand_io_t *io = ziprand_io_file("file.zip");  // Slow!
    ziprand_archive_t *archive = ziprand_open(io);
    // ...
    ziprand_close(archive);
    ziprand_io_free(io);
}
```

### 2. Use `fread_at` for Random Access

```c
// good -> no position tracking needed
ziprand_fread_at(file, 1000, buf1, 100);
ziprand_fread_at(file, 5000, buf2, 100);

// less efficient -> seek + read
ziprand_fseek(file, 1000, SEEK_SET);
ziprand_fread(file, buf1, 100);
ziprand_fseek(file, 5000, SEEK_SET);
ziprand_fread(file, buf2, 100);
```

### 3. Thread Safety

Each thread needs its own archive handle:

```c
void* worker(void *arg) {
    // each thread opens independently
    ziprand_io_t *io = ziprand_io_file("shared.zip");
    ziprand_archive_t *archive = ziprand_open(io);
    
    // work with archive...
    
    ziprand_close(archive);
    ziprand_io_free(io);
    return NULL;
}
```

### 4. Buffer Sizes

```c
// sequential reading -> use larger buffers
char buffer[1024 * 1024];  // 1 MB

// random access -> smaller is fine
char buffer[4096];  // 4 KB
```

---

## Advanced Examples

### Custom I/O: XOR Encryption

```c
typedef struct {
    FILE *fp;
    uint8_t key;
} xor_io_ctx_t;

int64_t xor_read(void *ctx, uint64_t offset, void *buffer, size_t size) {
    xor_io_ctx_t *xor = ctx;
    fseek(xor->fp, offset, SEEK_SET);
    int64_t bytes = fread(buffer, 1, size, xor->fp);
    
    // Decrypt
    uint8_t *buf = buffer;
    for (int64_t i = 0; i < bytes; i++) {
        buf[i] ^= xor->key;
    }
    
    return bytes;
}

// use it
xor_io_ctx_t xor_ctx = { .fp = fopen("encrypted.zip", "rb"), .key = 0x42 };
ziprand_io_t io = { .ctx = &xor_ctx, .read = xor_read, ... };
ziprand_archive_t *archive = ziprand_open(&io);
```

### Nested ZIP (ZIP inside ZIP)

```c
// open outer ZIP
ziprand_io_t *outer_io = ziprand_io_file("outer.zip");
ziprand_archive_t *outer = ziprand_open(outer_io);

// get inner.zip as a file
ziprand_file_t *inner_zip_file = ziprand_fopen_by_name(outer, "inner.zip");

// create I/O that reads from inner_zip_file
typedef struct {
    ziprand_file_t *file;
} nested_ctx_t;

int64_t nested_read(void *ctx, uint64_t offset, void *buffer, size_t size) {
    nested_ctx_t *n = ctx;
    return ziprand_fread_at(n->file, offset, buffer, size);
}

nested_ctx_t nested = { .file = inner_zip_file };
ziprand_io_t inner_io = { .ctx = &nested, .read = nested_read, ... };

// open inner ZIP
ziprand_archive_t *inner = ziprand_open(&inner_io);

// read from inner ZIP!
ziprand_file_t *file = ziprand_fopen_by_name(inner, "data.txt");
```
---

## FAQ

**Q: Can I use this with compressed (DEFLATE) files?**  
A: No. Random seeking is impossible with streaming compression. You must decompress sequentially, which defeats the purpose.

**Q: Can multiple threads read the same file?**  
A: Each thread needs its own `ziprand_file_t` handle. You can open the same entry multiple times.

**Q: Does it support encrypted ZIPs?**  
A: No, but you can implement encryption in your I/O callbacks.

**Q: Can I write/modify ZIPs?**  
A: No, this is read-only. Use libzip for write support.

**Q: What about ZIP64?**  
A: Fully supported for archives and files > 4GB.

**Q: How do I create uncompressed ZIPs?**  
```bash
# using zip command
zip -0 archive.zip file1.txt file2.txt

# using Python
import zipfile
with zipfile.ZipFile('archive.zip', 'w', zipfile.ZIP_STORED) as zf:
    zf.write('file1.txt')
```
---

## Building

### Core Library

```bash
meson setup build
ninja -C build
```

## License

Apache-2

## Contributing

Issues and pull requests welcome!
