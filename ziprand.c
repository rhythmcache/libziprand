#include "ziprand.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ZIP signatures */
#define EOCD_SIGNATURE               0x06054b50
#define ZIP64_EOCD_SIGNATURE         0x06064b50
#define ZIP64_EOCD_LOCATOR_SIGNATURE 0x07064b50
#define CENTRAL_DIR_SIGNATURE        0x02014b50
#define LOCAL_HEADER_SIGNATURE       0x04034b50

/* internal structures */
struct ziprand_archive {
    ziprand_io_t io;
    ziprand_entry_t* entries;
    size_t entry_count;
    uint64_t total_size;
};

struct ziprand_file {
    ziprand_archive_t* archive;
    const ziprand_entry_t* entry;
    uint64_t position;
};

/* utility functions */
static inline uint16_t read_u16_le(const uint8_t* p)
{
    return (uint16_t)p[0] | ((uint16_t)p[1] << 8);
}

static inline uint32_t read_u32_le(const uint8_t* p)
{
    return (uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
}

static inline uint64_t read_u64_le(const uint8_t* p)
{
    return (uint64_t)p[0] | ((uint64_t)p[1] << 8) | ((uint64_t)p[2] << 16) |
           ((uint64_t)p[3] << 24) | ((uint64_t)p[4] << 32) | ((uint64_t)p[5] << 40) |
           ((uint64_t)p[6] << 48) | ((uint64_t)p[7] << 56);
}

/* find End of Central Directory record */
static ziprand_error_t
find_eocd(ziprand_archive_t* archive, uint64_t* eocd_offset, uint16_t* num_entries)
{
    uint8_t buffer[8192];
    uint64_t file_size = archive->total_size;
    uint64_t max_search = file_size < 65557 ? file_size : 65557;
    uint64_t search_pos = file_size;

    while (search_pos > file_size - max_search) {
        size_t chunk_size = search_pos - (file_size - max_search);
        if (chunk_size > sizeof(buffer))
            chunk_size = sizeof(buffer);

        uint64_t read_pos = search_pos - chunk_size;
        int64_t bytes_read = archive->io.read(archive->io.ctx, read_pos, buffer, chunk_size);

        if (bytes_read <= 0)
            return ZIPRAND_ERR_IO;

        for (int64_t i = bytes_read - 4; i >= 0; i--) {
            uint32_t sig = read_u32_le(&buffer[i]);
            if (sig == EOCD_SIGNATURE) {
                *eocd_offset = read_pos + i;
                if (i + 10 < bytes_read) {
                    *num_entries = read_u16_le(&buffer[i + 10]);
                } else {
                    uint8_t entry_buf[2];
                    if (archive->io.read(archive->io.ctx, *eocd_offset + 10, entry_buf, 2) != 2)
                        return ZIPRAND_ERR_IO;
                    *num_entries = read_u16_le(entry_buf);
                }
                return ZIPRAND_OK;
            }
        }

        search_pos = read_pos;
        if (search_pos < 3)
            break;
    }

    return ZIPRAND_ERR_INVALID_ZIP;
}

/* read ZIP64 EOCD */
static ziprand_error_t read_zip64_eocd(ziprand_archive_t* archive,
                                       uint64_t eocd_offset,
                                       uint64_t* cd_offset,
                                       uint64_t* num_entries)
{
    uint8_t buffer[56];
    uint64_t search_start = eocd_offset > 20 ? eocd_offset - 20 : 0;

    /* find ZIP64 EOCD locator */
    uint8_t search_buf[20];
    if (archive->io.read(archive->io.ctx, search_start, search_buf, 20) != 20)
        return ZIPRAND_ERR_IO;

    uint64_t zip64_eocd_offset = 0;
    for (int i = 0; i <= 16; i++) {
        if (read_u32_le(&search_buf[i]) == ZIP64_EOCD_LOCATOR_SIGNATURE) {
            zip64_eocd_offset = read_u64_le(&search_buf[i + 8]);
            break;
        }
    }

    if (zip64_eocd_offset == 0)
        return ZIPRAND_ERR_INVALID_ZIP;

    /* read ZIP64 EOCD */
    if (archive->io.read(archive->io.ctx, zip64_eocd_offset, buffer, 56) != 56)
        return ZIPRAND_ERR_IO;

    if (read_u32_le(buffer) != ZIP64_EOCD_SIGNATURE)
        return ZIPRAND_ERR_INVALID_ZIP;

    *cd_offset = read_u64_le(&buffer[48]);
    *num_entries = read_u64_le(&buffer[32]);

    return ZIPRAND_OK;
}

/* get central directory info */
static ziprand_error_t
get_cd_info(ziprand_archive_t* archive, uint64_t* cd_offset, uint64_t* num_entries)
{
    uint64_t eocd_offset;
    uint16_t entries_16;
    ziprand_error_t err = find_eocd(archive, &eocd_offset, &entries_16);
    if (err != ZIPRAND_OK)
        return err;

    uint8_t eocd_buf[22];
    if (archive->io.read(archive->io.ctx, eocd_offset, eocd_buf, 22) != 22)
        return ZIPRAND_ERR_IO;

    uint32_t cd_offset_32 = read_u32_le(&eocd_buf[16]);

    if (cd_offset_32 == 0xFFFFFFFF) {
        return read_zip64_eocd(archive, eocd_offset, cd_offset, num_entries);
    } else {
        *cd_offset = cd_offset_32;
        *num_entries = entries_16;
        return ZIPRAND_OK;
    }
}

/* read central directory entry */
static ziprand_error_t
read_cd_entry(ziprand_archive_t* archive, uint64_t* offset, ziprand_entry_t* entry)
{
    uint8_t header[46];
    if (archive->io.read(archive->io.ctx, *offset, header, 46) != 46)
        return ZIPRAND_ERR_IO;

    if (read_u32_le(header) != CENTRAL_DIR_SIGNATURE)
        return ZIPRAND_ERR_INVALID_ZIP;

    entry->compression_method = read_u16_le(&header[10]);
    uint16_t filename_len = read_u16_le(&header[28]);
    uint16_t extra_len = read_u16_le(&header[30]);
    uint16_t comment_len = read_u16_le(&header[32]);

    uint64_t compressed_size = read_u32_le(&header[20]);
    uint64_t uncompressed_size = read_u32_le(&header[24]);
    uint64_t local_offset = read_u32_le(&header[42]);

    /* Read filename */
    entry->name = malloc(filename_len + 1);
    if (!entry->name)
        return ZIPRAND_ERR_NOMEM;

    if (archive->io.read(archive->io.ctx, *offset + 46, entry->name, filename_len) !=
        filename_len) {
        free(entry->name);
        return ZIPRAND_ERR_IO;
    }
    entry->name[filename_len] = '\0';

    /* read extra field for ZIP64 */
    if (extra_len > 0) {
        uint8_t* extra = malloc(extra_len);
        if (!extra) {
            free(entry->name);
            return ZIPRAND_ERR_NOMEM;
        }

        if (archive->io.read(archive->io.ctx, *offset + 46 + filename_len, extra, extra_len) !=
            extra_len) {
            free(extra);
            free(entry->name);
            return ZIPRAND_ERR_IO;
        }

        /* parse ZIP64 extra field */
        if (uncompressed_size == 0xFFFFFFFF || compressed_size == 0xFFFFFFFF ||
            local_offset == 0xFFFFFFFF) {
            size_t pos = 0;
            while (pos + 4 <= extra_len) {
                uint16_t header_id = read_u16_le(&extra[pos]);
                uint16_t data_size = read_u16_le(&extra[pos + 2]);

                if (header_id == 0x0001) {
                    size_t field_pos = pos + 4;
                    if (uncompressed_size == 0xFFFFFFFF && field_pos + 8 <= pos + 4 + data_size) {
                        uncompressed_size = read_u64_le(&extra[field_pos]);
                        field_pos += 8;
                    }
                    if (compressed_size == 0xFFFFFFFF && field_pos + 8 <= pos + 4 + data_size) {
                        compressed_size = read_u64_le(&extra[field_pos]);
                        field_pos += 8;
                    }
                    if (local_offset == 0xFFFFFFFF && field_pos + 8 <= pos + 4 + data_size) {
                        local_offset = read_u64_le(&extra[field_pos]);
                    }
                    break;
                }
                pos += 4 + data_size;
            }
        }
        free(extra);
    }

    entry->compressed_size = compressed_size;
    entry->uncompressed_size = uncompressed_size;
    entry->offset = local_offset;
    entry->data_offset = 0; /* will be calculated later */

    *offset += 46 + filename_len + extra_len + comment_len;
    return ZIPRAND_OK;
}

/* calculate data offset for an entry */
static ziprand_error_t get_data_offset(ziprand_archive_t* archive, ziprand_entry_t* entry)
{
    uint8_t local_header[30];
    if (archive->io.read(archive->io.ctx, entry->offset, local_header, 30) != 30)
        return ZIPRAND_ERR_IO;

    if (read_u32_le(local_header) != LOCAL_HEADER_SIGNATURE)
        return ZIPRAND_ERR_INVALID_ZIP;

    uint16_t filename_len = read_u16_le(&local_header[26]);
    uint16_t extra_len = read_u16_le(&local_header[28]);

    entry->data_offset = entry->offset + 30 + filename_len + extra_len;
    return ZIPRAND_OK;
}

/* public API implementation */

ziprand_archive_t* ziprand_open(const ziprand_io_t* io)
{
    if (!io || !io->read || !io->get_size)
        return NULL;

    ziprand_archive_t* archive = calloc(1, sizeof(ziprand_archive_t));
    if (!archive)
        return NULL;

    archive->io = *io;

    int64_t size = archive->io.get_size(archive->io.ctx);
    if (size < 0) {
        free(archive);
        return NULL;
    }
    archive->total_size = size;

    uint64_t cd_offset, num_entries;
    if (get_cd_info(archive, &cd_offset, &num_entries) != ZIPRAND_OK) {
        free(archive);
        return NULL;
    }

    archive->entries = calloc(num_entries, sizeof(ziprand_entry_t));
    if (!archive->entries) {
        free(archive);
        return NULL;
    }

    uint64_t offset = cd_offset;
    for (size_t i = 0; i < num_entries; i++) {
        if (read_cd_entry(archive, &offset, &archive->entries[i]) != ZIPRAND_OK) {
            for (size_t j = 0; j < i; j++)
                free(archive->entries[j].name);
            free(archive->entries);
            free(archive);
            return NULL;
        }
    }

    archive->entry_count = num_entries;
    return archive;
}

void ziprand_close(ziprand_archive_t* archive)
{
    if (!archive)
        return;

    if (archive->io.close)
        archive->io.close(archive->io.ctx);

    for (size_t i = 0; i < archive->entry_count; i++)
        free(archive->entries[i].name);

    free(archive->entries);
    free(archive);
}

int64_t ziprand_get_entry_count(ziprand_archive_t* archive)
{
    return archive ? (int64_t)archive->entry_count : -1;
}

const ziprand_entry_t* ziprand_get_entry_by_index(ziprand_archive_t* archive, size_t index)
{
    if (!archive || index >= archive->entry_count)
        return NULL;
    return &archive->entries[index];
}

const ziprand_entry_t* ziprand_find_entry(ziprand_archive_t* archive, const char* name)
{
    if (!archive || !name)
        return NULL;

    for (size_t i = 0; i < archive->entry_count; i++) {
        if (strcmp(archive->entries[i].name, name) == 0)
            return &archive->entries[i];
    }
    return NULL;
}

ziprand_file_t* ziprand_fopen(ziprand_archive_t* archive, const ziprand_entry_t* entry)
{
    if (!archive || !entry)
        return NULL;

    if (entry->compression_method != 0)
        return NULL;

    /* calculate data offset if not already done */
    ziprand_entry_t* mutable_entry = (ziprand_entry_t*)entry;
    if (mutable_entry->data_offset == 0) {
        if (get_data_offset(archive, mutable_entry) != ZIPRAND_OK)
            return NULL;
    }

    ziprand_file_t* file = malloc(sizeof(ziprand_file_t));
    if (!file)
        return NULL;

    file->archive = archive;
    file->entry = entry;
    file->position = 0;

    return file;
}

ziprand_file_t* ziprand_fopen_by_name(ziprand_archive_t* archive, const char* name)
{
    const ziprand_entry_t* entry = ziprand_find_entry(archive, name);
    if (!entry)
        return NULL;
    return ziprand_fopen(archive, entry);
}

int64_t ziprand_fread(ziprand_file_t* file, void* buffer, size_t size)
{
    if (!file)
        return -1;
    int64_t result = ziprand_fread_at(file, file->position, buffer, size);
    if (result > 0)
        file->position += result;
    return result;
}

int64_t ziprand_fread_at(ziprand_file_t* file, uint64_t offset, void* buffer, size_t size)
{
    if (!file || !buffer)
        return -1;

    if (offset >= file->entry->uncompressed_size)
        return 0;

    uint64_t remaining = file->entry->uncompressed_size - offset;
    size_t to_read = size < remaining ? size : remaining;

    return file->archive->io.read(
        file->archive->io.ctx, file->entry->data_offset + offset, buffer, to_read);
}

int64_t ziprand_fseek(ziprand_file_t* file, int64_t offset, int whence)
{
    if (!file)
        return -1;

    uint64_t new_pos;
    switch (whence) {
    case SEEK_SET:
        new_pos = offset;
        break;
    case SEEK_CUR:
        if (offset >= 0) {
            new_pos = file->position + offset;
        } else {
            if ((uint64_t)(-offset) > file->position)
                new_pos = 0;
            else
                new_pos = file->position - (-offset);
        }
        break;
    case SEEK_END:
        if (offset >= 0) {
            new_pos = file->entry->uncompressed_size + offset;
        } else {
            if ((uint64_t)(-offset) > file->entry->uncompressed_size)
                new_pos = 0;
            else
                new_pos = file->entry->uncompressed_size - (-offset);
        }
        break;
    default:
        return -1;
    }

    if (new_pos > file->entry->uncompressed_size)
        return -1;

    file->position = new_pos;
    return file->position;
}

int64_t ziprand_ftell(ziprand_file_t* file)
{
    return file ? (int64_t)file->position : -1;
}

int64_t ziprand_fsize(ziprand_file_t* file)
{
    return file ? (int64_t)file->entry->uncompressed_size : -1;
}

void ziprand_fclose(ziprand_file_t* file)
{
    free(file);
}

const char* ziprand_strerror(ziprand_error_t error)
{
    switch (error) {
    case ZIPRAND_OK:
        return "Success";
    case ZIPRAND_ERR_IO:
        return "I/O error";
    case ZIPRAND_ERR_INVALID_ZIP:
        return "Invalid ZIP file";
    case ZIPRAND_ERR_NOT_FOUND:
        return "Entry not found";
    case ZIPRAND_ERR_COMPRESSED:
        return "Entry is compressed";
    case ZIPRAND_ERR_NOMEM:
        return "Out of memory";
    case ZIPRAND_ERR_INVALID_PARAM:
        return "Invalid parameter";
    case ZIPRAND_ERR_SEEK_BEYOND_END:
        return "Seek beyond end of file";
    default:
        return "Unknown error";
    }
}
