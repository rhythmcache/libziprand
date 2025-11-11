//! # ziprand
//!
//! A lightweight library for random access reading of uncompressed (STORED) ZIP files.
//!
//! This library is designed for scenarios where you need to read specific files from
//! a ZIP archive without decompressing the entire archive. It supports both ZIP32 and
//! ZIP64 formats.
//!
//! ## Features
//!
//! - Random access to files within ZIP archives
//! - Support for both ZIP32 and ZIP64 formats
//! - Async I/O support (requires `async` feature)
//! - Only supports STORED (uncompressed) files
//! - Pluggable I/O backend via the `ZipIO` trait
//!
//! ## Example
//!
//! ```rust,no_run
//! use ziprand::{ZipReader, ZipIO};
//! use anyhow::Result;
//!
//! #[cfg(feature = "async")]
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     // Implement ZipIO for your I/O source
//!     let io = MyZipIO::new("archive.zip")?;
//!     let reader = ZipReader::new(io);
//!     
//!     // List all entries
//!     let entries = reader.list_entries().await?;
//!     
//!     // Find a specific file
//!     if let Some(entry) = reader.find_entry("data.txt").await? {
//!         // Get random access reader for the file
//!         let file_reader = reader.open_entry(&entry).await?;
//!         
//!         // Read at specific offset
//!         let mut buf = vec![0u8; 1024];
//!         file_reader.read_at(512, &mut buf).await?;
//!     }
//!     
//!     Ok(())
//! }
//! ```

use eyre::{Result, eyre};

#[cfg(feature = "async")]
use async_trait::async_trait;

// ============================================================================
// Constants
// ============================================================================

/// ZIP local file header signature (0x04034b50)
pub const LOCAL_FILE_HEADER_SIGNATURE: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];

/// ZIP central directory header signature (0x02014b50)
pub const CENTRAL_DIR_HEADER_SIGNATURE: [u8; 4] = [0x50, 0x4B, 0x01, 0x02];

/// End of Central Directory signature (0x06054b50)
pub const EOCD_SIGNATURE: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];

/// ZIP64 End of Central Directory signature (0x06064b50)
pub const ZIP64_EOCD_SIGNATURE: [u8; 4] = [0x50, 0x4B, 0x06, 0x06];

/// ZIP64 End of Central Directory Locator signature (0x07064b50)
pub const ZIP64_EOCD_LOCATOR_SIGNATURE: [u8; 4] = [0x50, 0x4B, 0x06, 0x07];

/// Compression method: no compression (stored)
pub const COMPRESSION_STORED: u16 = 0;

// ============================================================================
// I/O Trait
// ============================================================================

/// Abstract I/O trait for reading ZIP files from any source.
///
/// Implement this trait to provide ZIP reading capabilities for your
/// storage backend (e.g., files, network streams, in-memory buffers).
#[cfg(feature = "async")]
#[async_trait]
pub trait ZipIO: Send + Sync {
    /// Read exact number of bytes at given offset.
    ///
    /// # Arguments
    ///
    /// * `offset` - Byte offset from the start of the source
    /// * `buf` - Buffer to read into (must be completely filled)
    ///
    /// # Errors
    ///
    /// Returns an error if the read fails or if EOF is reached before
    /// filling the buffer.
    async fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<()>;

    /// Get total size of the source in bytes.
    async fn size(&self) -> Result<u64>;
}

#[cfg(not(feature = "async"))]
pub trait ZipIO: Send + Sync {
    /// Read exact number of bytes at given offset.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<()>;

    /// Get total size of the source in bytes.
    fn size(&self) -> Result<u64>;
}

// Implement ZipIO for references to ZipIO types (for both async and sync)
#[cfg(feature = "async")]
#[async_trait]
impl<T: ZipIO + ?Sized> ZipIO for &T {
    async fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<()> {
        (**self).read_at(offset, buf).await
    }

    async fn size(&self) -> Result<u64> {
        (**self).size().await
    }
}

#[cfg(not(feature = "async"))]
impl<T: ZipIO + ?Sized> ZipIO for &T {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<()> {
        (**self).read_at(offset, buf)
    }

    fn size(&self) -> Result<u64> {
        (**self).size()
    }
}

// ============================================================================
// Data Structures
// ============================================================================

/// Represents a single entry (file or directory) in a ZIP archive.
#[derive(Debug, Clone)]
pub struct ZipEntry {
    /// File name (path) within the ZIP archive
    pub name: String,

    /// Uncompressed size in bytes
    pub uncompressed_size: u64,

    /// Offset of the local file header in the ZIP file
    pub offset: u64,

    /// Compression method (0 = stored/uncompressed)
    pub compression_method: u16,
}

impl ZipEntry {
    /// Check if this entry is a directory.
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }

    /// Check if this entry uses STORED (uncompressed) compression.
    pub fn is_stored(&self) -> bool {
        self.compression_method == COMPRESSION_STORED
    }
}

/// Random access reader for a specific file within a ZIP archive.
pub struct ZipFileReader<I: ZipIO> {
    io: I,
    data_offset: u64,
    size: u64,
}

impl<I: ZipIO> ZipFileReader<I> {
    /// Get the size of the file in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Read bytes at a specific offset within the file.
    ///
    /// # Arguments
    ///
    /// * `offset` - Offset within the file (not the ZIP archive)
    /// * `buf` - Buffer to read into
    ///
    /// # Errors
    ///
    /// Returns an error if offset + buf.len() exceeds file size or if read fails.
    #[cfg(feature = "async")]
    pub async fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<()> {
        if offset + buf.len() as u64 > self.size {
            return Err(eyre!("Read beyond end of file"));
        }

        self.io.read_at(self.data_offset + offset, buf).await
    }

    #[cfg(not(feature = "async"))]
    pub fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<()> {
        if offset + buf.len() as u64 > self.size {
            return Err(eyre!("Read beyond end of file"));
        }

        self.io.read_at(self.data_offset + offset, buf)
    }

    /// Read the entire file contents into a vector.
    #[cfg(feature = "async")]
    pub async fn read_all(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; self.size as usize];
        self.read_at(0, &mut buf).await?;
        Ok(buf)
    }

    #[cfg(not(feature = "async"))]
    pub fn read_all(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; self.size as usize];
        self.read_at(0, &mut buf)?;
        Ok(buf)
    }
}

// ============================================================================
// ZIP Reader
// ============================================================================

/// Main ZIP archive reader supporting random access to uncompressed files.
pub struct ZipReader<I: ZipIO> {
    io: I,
}

impl<I: ZipIO> ZipReader<I> {
    /// Create a new ZIP reader from an I/O source.
    pub fn new(io: I) -> Self {
        Self { io }
    }

    /// List all entries in the ZIP archive.
    ///
    /// # Returns
    ///
    /// A vector of all entries found in the central directory.
    #[cfg(feature = "async")]
    pub async fn list_entries(&self) -> Result<Vec<ZipEntry>> {
        let (cd_offset, num_entries) = self.get_central_directory_info().await?;
        let mut entries = Vec::with_capacity(num_entries);
        let mut current_offset = cd_offset;

        for _ in 0..num_entries {
            let (entry, next_offset) = self.read_central_directory_entry(current_offset).await?;
            entries.push(entry);
            current_offset = next_offset;
        }

        Ok(entries)
    }

    #[cfg(not(feature = "async"))]
    pub fn list_entries(&self) -> Result<Vec<ZipEntry>> {
        let (cd_offset, num_entries) = self.get_central_directory_info()?;
        let mut entries = Vec::with_capacity(num_entries);
        let mut current_offset = cd_offset;

        for _ in 0..num_entries {
            let (entry, next_offset) = self.read_central_directory_entry(current_offset)?;
            entries.push(entry);
            current_offset = next_offset;
        }

        Ok(entries)
    }

    /// Find a specific entry by name.
    ///
    /// # Arguments
    ///
    /// * `name` - File name to search for (case-sensitive)
    ///
    /// # Returns
    ///
    /// The entry if found, or None if not found.
    #[cfg(feature = "async")]
    pub async fn find_entry(&self, name: &str) -> Result<Option<ZipEntry>> {
        let entries = self.list_entries().await?;
        Ok(entries.into_iter().find(|e| e.name == name))
    }

    #[cfg(not(feature = "async"))]
    pub fn find_entry(&self, name: &str) -> Result<Option<ZipEntry>> {
        let entries = self.list_entries()?;
        Ok(entries.into_iter().find(|e| e.name == name))
    }

    /// Find all entries matching a predicate.
    #[cfg(feature = "async")]
    pub async fn find_entries<F>(&self, predicate: F) -> Result<Vec<ZipEntry>>
    where
        F: Fn(&ZipEntry) -> bool,
    {
        let entries = self.list_entries().await?;
        Ok(entries.into_iter().filter(predicate).collect())
    }

    #[cfg(not(feature = "async"))]
    pub fn find_entries<F>(&self, predicate: F) -> Result<Vec<ZipEntry>>
    where
        F: Fn(&ZipEntry) -> bool,
    {
        let entries = self.list_entries()?;
        Ok(entries.into_iter().filter(predicate).collect())
    }

    /// Open an entry for random access reading.
    ///
    /// # Arguments
    ///
    /// * `entry` - The ZIP entry to open
    ///
    /// # Errors
    ///
    /// Returns an error if the entry is compressed or if the local header is invalid.
    #[cfg(feature = "async")]
    pub async fn open_entry(&self, entry: &ZipEntry) -> Result<ZipFileReader<&I>> {
        if !entry.is_stored() {
            return Err(eyre!(
                "Cannot open compressed entry '{}' (compression method {}). Only STORED (0) is supported.",
                entry.name,
                entry.compression_method
            ));
        }

        let data_offset = self.get_data_offset(entry).await?;

        Ok(ZipFileReader {
            io: &self.io,
            data_offset,
            size: entry.uncompressed_size,
        })
    }

    #[cfg(not(feature = "async"))]
    pub fn open_entry(&self, entry: &ZipEntry) -> Result<ZipFileReader<&I>> {
        if !entry.is_stored() {
            return Err(eyre!(
                "Cannot open compressed entry '{}' (compression method {}). Only STORED (0) is supported.",
                entry.name,
                entry.compression_method
            ));
        }

        let data_offset = self.get_data_offset(entry)?;

        Ok(ZipFileReader {
            io: &self.io,
            data_offset,
            size: entry.uncompressed_size,
        })
    }

    // Internal methods

    /// Find the End of Central Directory record.
    #[cfg(feature = "async")]
    async fn find_eocd(&self) -> Result<(u64, u16)> {
        let file_size = self.io.size().await?;

        let max_comment_size = 65535;
        let eocd_min_size = 22;
        let max_search = std::cmp::min(file_size, (max_comment_size + eocd_min_size) as u64);
        let chunk_size = 8192;
        let mut current_pos = file_size;
        let mut eocd_pos = None;
        let mut buffer = vec![0u8; chunk_size];

        while current_pos > file_size.saturating_sub(max_search) && eocd_pos.is_none() {
            let read_size = std::cmp::min(
                chunk_size,
                (current_pos - file_size.saturating_sub(max_search)) as usize,
            );
            let read_pos = current_pos.saturating_sub(read_size as u64);

            self.io.read_at(read_pos, &mut buffer[..read_size]).await?;

            if read_size >= 4 {
                for i in (0..=read_size - 4).rev() {
                    if buffer[i..i + 4] == EOCD_SIGNATURE {
                        eocd_pos = Some(read_pos + i as u64);
                        break;
                    }
                }
            }

            current_pos = read_pos;
            if current_pos > 3 {
                current_pos -= 3;
            }
        }

        let eocd_offset =
            eocd_pos.ok_or_else(|| eyre!("Could not find End of Central Directory record"))?;

        // Read number of entries
        let mut num_entries_buf = [0u8; 2];
        self.io
            .read_at(eocd_offset + 10, &mut num_entries_buf)
            .await?;
        let num_entries = u16::from_le_bytes(num_entries_buf);

        Ok((eocd_offset, num_entries))
    }

    #[cfg(not(feature = "async"))]
    fn find_eocd(&self) -> Result<(u64, u16)> {
        let file_size = self.io.size()?;

        let max_comment_size = 65535;
        let eocd_min_size = 22;
        let max_search = std::cmp::min(file_size, (max_comment_size + eocd_min_size) as u64);
        let chunk_size = 8192;
        let mut current_pos = file_size;
        let mut eocd_pos = None;
        let mut buffer = vec![0u8; chunk_size];

        while current_pos > file_size.saturating_sub(max_search) && eocd_pos.is_none() {
            let read_size = std::cmp::min(
                chunk_size,
                (current_pos - file_size.saturating_sub(max_search)) as usize,
            );
            let read_pos = current_pos.saturating_sub(read_size as u64);

            self.io.read_at(read_pos, &mut buffer[..read_size])?;

            if read_size >= 4 {
                for i in (0..=read_size - 4).rev() {
                    if buffer[i..i + 4] == EOCD_SIGNATURE {
                        eocd_pos = Some(read_pos + i as u64);
                        break;
                    }
                }
            }

            current_pos = read_pos;
            if current_pos > 3 {
                current_pos -= 3;
            }
        }

        let eocd_offset =
            eocd_pos.ok_or_else(|| eyre!("Could not find End of Central Directory record"))?;

        let mut num_entries_buf = [0u8; 2];
        self.io.read_at(eocd_offset + 10, &mut num_entries_buf)?;
        let num_entries = u16::from_le_bytes(num_entries_buf);

        Ok((eocd_offset, num_entries))
    }

    /// Read ZIP64 End of Central Directory information.
    #[cfg(feature = "async")]
    async fn read_zip64_eocd(&self, eocd_offset: u64) -> Result<(u64, u64)> {
        if eocd_offset < 20 {
            return Err(eyre!("Invalid ZIP64 structure"));
        }

        let search_start = eocd_offset.saturating_sub(20);
        let mut buffer = vec![0u8; (eocd_offset - search_start) as usize];
        self.io.read_at(search_start, &mut buffer).await?;

        let mut zip64_eocd_offset = 0u64;
        let mut found_locator = false;

        if buffer.len() >= 4 {
            for i in (0..=buffer.len() - 4).rev() {
                if buffer[i..i + 4] == ZIP64_EOCD_LOCATOR_SIGNATURE {
                    found_locator = true;
                    if i + 16 <= buffer.len() {
                        zip64_eocd_offset = u64::from_le_bytes([
                            buffer[i + 8],
                            buffer[i + 9],
                            buffer[i + 10],
                            buffer[i + 11],
                            buffer[i + 12],
                            buffer[i + 13],
                            buffer[i + 14],
                            buffer[i + 15],
                        ]);
                    }
                    break;
                }
            }
        }

        if !found_locator {
            return Err(eyre!(
                "ZIP64 format indicated but ZIP64 EOCD locator not found"
            ));
        }

        let mut zip64_eocd = [0u8; 56];
        self.io.read_at(zip64_eocd_offset, &mut zip64_eocd).await?;

        if zip64_eocd[0..4] != ZIP64_EOCD_SIGNATURE {
            return Err(eyre!("Invalid ZIP64 EOCD signature"));
        }

        let cd_offset = u64::from_le_bytes([
            zip64_eocd[48],
            zip64_eocd[49],
            zip64_eocd[50],
            zip64_eocd[51],
            zip64_eocd[52],
            zip64_eocd[53],
            zip64_eocd[54],
            zip64_eocd[55],
        ]);

        let num_entries = u64::from_le_bytes([
            zip64_eocd[32],
            zip64_eocd[33],
            zip64_eocd[34],
            zip64_eocd[35],
            zip64_eocd[36],
            zip64_eocd[37],
            zip64_eocd[38],
            zip64_eocd[39],
        ]);

        Ok((cd_offset, num_entries))
    }

    #[cfg(not(feature = "async"))]
    fn read_zip64_eocd(&self, eocd_offset: u64) -> Result<(u64, u64)> {
        if eocd_offset < 20 {
            return Err(eyre!("Invalid ZIP64 structure"));
        }

        let search_start = eocd_offset.saturating_sub(20);
        let mut buffer = vec![0u8; (eocd_offset - search_start) as usize];
        self.io.read_at(search_start, &mut buffer)?;

        let mut zip64_eocd_offset = 0u64;
        let mut found_locator = false;

        if buffer.len() >= 4 {
            for i in (0..=buffer.len() - 4).rev() {
                if buffer[i..i + 4] == ZIP64_EOCD_LOCATOR_SIGNATURE {
                    found_locator = true;
                    if i + 16 <= buffer.len() {
                        zip64_eocd_offset = u64::from_le_bytes([
                            buffer[i + 8],
                            buffer[i + 9],
                            buffer[i + 10],
                            buffer[i + 11],
                            buffer[i + 12],
                            buffer[i + 13],
                            buffer[i + 14],
                            buffer[i + 15],
                        ]);
                    }
                    break;
                }
            }
        }

        if !found_locator {
            return Err(eyre!(
                "ZIP64 format indicated but ZIP64 EOCD locator not found"
            ));
        }

        let mut zip64_eocd = [0u8; 56];
        self.io.read_at(zip64_eocd_offset, &mut zip64_eocd)?;

        if zip64_eocd[0..4] != ZIP64_EOCD_SIGNATURE {
            return Err(eyre!("Invalid ZIP64 EOCD signature"));
        }

        let cd_offset = u64::from_le_bytes([
            zip64_eocd[48],
            zip64_eocd[49],
            zip64_eocd[50],
            zip64_eocd[51],
            zip64_eocd[52],
            zip64_eocd[53],
            zip64_eocd[54],
            zip64_eocd[55],
        ]);

        let num_entries = u64::from_le_bytes([
            zip64_eocd[32],
            zip64_eocd[33],
            zip64_eocd[34],
            zip64_eocd[35],
            zip64_eocd[36],
            zip64_eocd[37],
            zip64_eocd[38],
            zip64_eocd[39],
        ]);

        Ok((cd_offset, num_entries))
    }

    /// Get central directory offset and number of entries.
    #[cfg(feature = "async")]
    async fn get_central_directory_info(&self) -> Result<(u64, usize)> {
        let (eocd_offset, num_entries) = self.find_eocd().await?;

        let mut cd_offset_buf = [0u8; 4];
        self.io
            .read_at(eocd_offset + 16, &mut cd_offset_buf)
            .await?;
        let cd_offset = u32::from_le_bytes(cd_offset_buf) as u64;

        if cd_offset == 0xFFFFFFFF {
            let (real_cd_offset, real_num_entries) = self.read_zip64_eocd(eocd_offset).await?;
            Ok((real_cd_offset, real_num_entries as usize))
        } else {
            Ok((cd_offset, num_entries as usize))
        }
    }

    #[cfg(not(feature = "async"))]
    fn get_central_directory_info(&self) -> Result<(u64, usize)> {
        let (eocd_offset, num_entries) = self.find_eocd()?;

        let mut cd_offset_buf = [0u8; 4];
        self.io.read_at(eocd_offset + 16, &mut cd_offset_buf)?;
        let cd_offset = u32::from_le_bytes(cd_offset_buf) as u64;

        if cd_offset == 0xFFFFFFFF {
            let (real_cd_offset, real_num_entries) = self.read_zip64_eocd(eocd_offset)?;
            Ok((real_cd_offset, real_num_entries as usize))
        } else {
            Ok((cd_offset, num_entries as usize))
        }
    }

    /// Read a single central directory entry.
    #[cfg(feature = "async")]
    async fn read_central_directory_entry(&self, offset: u64) -> Result<(ZipEntry, u64)> {
        let mut entry_header = [0u8; 46];
        self.io.read_at(offset, &mut entry_header).await?;

        if entry_header[0..4] != CENTRAL_DIR_HEADER_SIGNATURE {
            return Err(eyre!("Invalid central directory header signature"));
        }

        let compression_method = u16::from_le_bytes([entry_header[10], entry_header[11]]);
        let filename_len = u16::from_le_bytes([entry_header[28], entry_header[29]]) as usize;
        let extra_len = u16::from_le_bytes([entry_header[30], entry_header[31]]) as usize;
        let comment_len = u16::from_le_bytes([entry_header[32], entry_header[33]]) as usize;

        let mut local_header_offset = u32::from_le_bytes([
            entry_header[42],
            entry_header[43],
            entry_header[44],
            entry_header[45],
        ]) as u64;

        let mut uncompressed_size = u32::from_le_bytes([
            entry_header[24],
            entry_header[25],
            entry_header[26],
            entry_header[27],
        ]) as u64;

        // Read filename
        let mut filename = vec![0u8; filename_len];
        self.io.read_at(offset + 46, &mut filename).await?;

        // Read extra data
        let mut extra_data = vec![0u8; extra_len];
        self.io
            .read_at(offset + 46 + filename_len as u64, &mut extra_data)
            .await?;

        // Handle ZIP64 extra fields
        if local_header_offset == 0xFFFFFFFF || uncompressed_size == 0xFFFFFFFF {
            let mut pos = 0;
            while pos + 4 <= extra_data.len() {
                let header_id = u16::from_le_bytes([extra_data[pos], extra_data[pos + 1]]);
                let data_size =
                    u16::from_le_bytes([extra_data[pos + 2], extra_data[pos + 3]]) as usize;

                if header_id == 0x0001 && pos + 4 + data_size <= extra_data.len() {
                    let mut field_pos = pos + 4;

                    if uncompressed_size == 0xFFFFFFFF && field_pos + 8 <= pos + 4 + data_size {
                        uncompressed_size = u64::from_le_bytes([
                            extra_data[field_pos],
                            extra_data[field_pos + 1],
                            extra_data[field_pos + 2],
                            extra_data[field_pos + 3],
                            extra_data[field_pos + 4],
                            extra_data[field_pos + 5],
                            extra_data[field_pos + 6],
                            extra_data[field_pos + 7],
                        ]);
                        field_pos += 8;
                    }

                    if local_header_offset == 0xFFFFFFFF && field_pos + 8 <= pos + 4 + data_size {
                        local_header_offset = u64::from_le_bytes([
                            extra_data[field_pos],
                            extra_data[field_pos + 1],
                            extra_data[field_pos + 2],
                            extra_data[field_pos + 3],
                            extra_data[field_pos + 4],
                            extra_data[field_pos + 5],
                            extra_data[field_pos + 6],
                            extra_data[field_pos + 7],
                        ]);
                    }
                    break;
                }
                pos += 4 + data_size;
            }
        }

        let next_offset = offset + 46 + filename_len as u64 + extra_len as u64 + comment_len as u64;

        Ok((
            ZipEntry {
                name: String::from_utf8_lossy(&filename).into_owned(),
                uncompressed_size,
                offset: local_header_offset,
                compression_method,
            },
            next_offset,
        ))
    }

    #[cfg(not(feature = "async"))]
    fn read_central_directory_entry(&self, offset: u64) -> Result<(ZipEntry, u64)> {
        let mut entry_header = [0u8; 46];
        self.io.read_at(offset, &mut entry_header)?;

        if entry_header[0..4] != CENTRAL_DIR_HEADER_SIGNATURE {
            return Err(eyre!("Invalid central directory header signature"));
        }

        let compression_method = u16::from_le_bytes([entry_header[10], entry_header[11]]);
        let filename_len = u16::from_le_bytes([entry_header[28], entry_header[29]]) as usize;
        let extra_len = u16::from_le_bytes([entry_header[30], entry_header[31]]) as usize;
        let comment_len = u16::from_le_bytes([entry_header[32], entry_header[33]]) as usize;

        let mut local_header_offset = u32::from_le_bytes([
            entry_header[42],
            entry_header[43],
            entry_header[44],
            entry_header[45],
        ]) as u64;

        let mut uncompressed_size = u32::from_le_bytes([
            entry_header[24],
            entry_header[25],
            entry_header[26],
            entry_header[27],
        ]) as u64;

        let mut filename = vec![0u8; filename_len];
        self.io.read_at(offset + 46, &mut filename)?;

        let mut extra_data = vec![0u8; extra_len];
        self.io
            .read_at(offset + 46 + filename_len as u64, &mut extra_data)?;

        // Handle ZIP64 extra fields
        if local_header_offset == 0xFFFFFFFF || uncompressed_size == 0xFFFFFFFF {
            let mut pos = 0;
            while pos + 4 <= extra_data.len() {
                let header_id = u16::from_le_bytes([extra_data[pos], extra_data[pos + 1]]);
                let data_size =
                    u16::from_le_bytes([extra_data[pos + 2], extra_data[pos + 3]]) as usize;

                if header_id == 0x0001 && pos + 4 + data_size <= extra_data.len() {
                    let mut field_pos = pos + 4;

                    if uncompressed_size == 0xFFFFFFFF && field_pos + 8 <= pos + 4 + data_size {
                        uncompressed_size = u64::from_le_bytes([
                            extra_data[field_pos],
                            extra_data[field_pos + 1],
                            extra_data[field_pos + 2],
                            extra_data[field_pos + 3],
                            extra_data[field_pos + 4],
                            extra_data[field_pos + 5],
                            extra_data[field_pos + 6],
                            extra_data[field_pos + 7],
                        ]);
                        field_pos += 8;
                    }

                    if local_header_offset == 0xFFFFFFFF && field_pos + 8 <= pos + 4 + data_size {
                        local_header_offset = u64::from_le_bytes([
                            extra_data[field_pos],
                            extra_data[field_pos + 1],
                            extra_data[field_pos + 2],
                            extra_data[field_pos + 3],
                            extra_data[field_pos + 4],
                            extra_data[field_pos + 5],
                            extra_data[field_pos + 6],
                            extra_data[field_pos + 7],
                        ]);
                    }
                    break;
                }
                pos += 4 + data_size;
            }
        }

        let next_offset = offset + 46 + filename_len as u64 + extra_len as u64 + comment_len as u64;

        Ok((
            ZipEntry {
                name: String::from_utf8_lossy(&filename).into_owned(),
                uncompressed_size,
                offset: local_header_offset,
                compression_method,
            },
            next_offset,
        ))
    }

    /// Calculate the actual data offset for a ZIP entry (after local header).
    #[cfg(feature = "async")]
    async fn get_data_offset(&self, entry: &ZipEntry) -> Result<u64> {
        let mut local_header = [0u8; 30];
        self.io.read_at(entry.offset, &mut local_header).await?;

        if local_header[0..4] != LOCAL_FILE_HEADER_SIGNATURE {
            return Err(eyre!("Invalid local file header signature"));
        }

        // Verify compression method in local header
        let local_compression = u16::from_le_bytes([local_header[8], local_header[9]]);
        if local_compression != COMPRESSION_STORED {
            return Err(eyre!(
                "Entry '{}' is compressed (method {}), expected STORED (0)",
                entry.name,
                local_compression
            ));
        }

        let local_filename_len = u16::from_le_bytes([local_header[26], local_header[27]]) as u64;
        let local_extra_len = u16::from_le_bytes([local_header[28], local_header[29]]) as u64;

        let data_offset = entry.offset + 30 + local_filename_len + local_extra_len;
        Ok(data_offset)
    }

    #[cfg(not(feature = "async"))]
    fn get_data_offset(&self, entry: &ZipEntry) -> Result<u64> {
        let mut local_header = [0u8; 30];
        self.io.read_at(entry.offset, &mut local_header)?;

        if local_header[0..4] != LOCAL_FILE_HEADER_SIGNATURE {
            return Err(eyre!("Invalid local file header signature"));
        }

        let local_compression = u16::from_le_bytes([local_header[8], local_header[9]]);
        if local_compression != COMPRESSION_STORED {
            return Err(eyre!(
                "Entry '{}' is compressed (method {}), expected STORED (0)",
                entry.name,
                local_compression
            ));
        }

        let local_filename_len = u16::from_le_bytes([local_header[26], local_header[27]]) as u64;
        let local_extra_len = u16::from_le_bytes([local_header[28], local_header[29]]) as u64;

        let data_offset = entry.offset + 30 + local_filename_len + local_extra_len;
        Ok(data_offset)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "async")]
    #[async_trait]
    impl ZipIO for Vec<u8> {
        async fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<()> {
            let start = offset as usize;
            let end = start + buf.len();

            if end > self.len() {
                return Err(eyre!("Read beyond end of buffer"));
            }

            buf.copy_from_slice(&self[start..end]);
            Ok(())
        }

        async fn size(&self) -> Result<u64> {
            Ok(self.len() as u64)
        }
    }

    #[cfg(not(feature = "async"))]
    impl ZipIO for Vec<u8> {
        fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<()> {
            let start = offset as usize;
            let end = start + buf.len();

            if end > self.len() {
                return Err(eyre!("Read beyond end of buffer"));
            }

            buf.copy_from_slice(&self[start..end]);
            Ok(())
        }

        fn size(&self) -> Result<u64> {
            Ok(self.len() as u64)
        }
    }
}
