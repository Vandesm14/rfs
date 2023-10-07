use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use thiserror::Error;

const TABLE_SIZE: usize = 256;

#[derive(Error, Debug)]
pub enum FileSystemError {
  #[error(transparent)]
  FileHeader(#[from] FileHeaderError),

  #[error("No more space in the table")]
  NoMoreSpaceInTable,

  #[error("No more space in the data")]
  NoMoreSpace,

  #[error(transparent)]
  IO(#[from] io::Error),
}

#[derive(Error, Debug)]
pub enum FileHeaderError {
  #[error("Could not read file header size")]
  FileHeaderSize(io::Error),

  #[error("Could not read data address")]
  DataAddress(io::Error),

  #[error("Could not read data length")]
  DataLength(io::Error),

  #[error("Could not read file name")]
  FileName(io::Error),

  #[error("File header size too small ({0})")]
  FileHeaderSizeTooSmall(u16),

  #[error("File header size too large ({0})")]
  FileHeaderSizeTooLarge(u16),

  #[error("File header size mismatch (expected {expected}, actual {actual})")]
  FileHeaderSizeMismatch { expected: u16, actual: u16 },

  #[error(transparent)]
  InvalidUTF8(#[from] std::string::FromUtf8Error),
}

/// File Header Spec:
/// - len of header (u16)
/// - addr of data (u16)
/// - len of data (u16)
/// - name (utf-8)
pub struct FileHeader {
  data_addr: u16,
  data_len: u16,
  name: String,
}

impl FileHeader {
  pub fn read(reader: &mut impl Read) -> Result<Self, FileHeaderError> {
    let mut size = [0u8; 2];
    reader
      .read_exact(&mut size)
      .map_err(FileHeaderError::FileHeaderSize)?;
    let size = u16::from_le_bytes(size);

    // If the total size is less than 7 bytes, then it's invalid
    // 7 is the minimum size of a file header (6 header bytes plus a single byte for the name)
    if size < 7 {
      return Err(FileHeaderError::FileHeaderSizeTooSmall(size));
    } else if size > TABLE_SIZE as u16 {
      return Err(FileHeaderError::FileHeaderSizeTooLarge(size));
    }

    let mut data_addr = [0u8; 2];
    reader
      .read_exact(&mut data_addr)
      .map_err(FileHeaderError::DataAddress)?;
    let data_addr = u16::from_le_bytes(data_addr);

    let mut data_len = [0u8; 2];
    reader
      .read_exact(&mut data_len)
      .map_err(FileHeaderError::DataLength)?;
    let data_len = u16::from_le_bytes(data_len);

    let mut name = vec![0u8; (size - 6) as usize];
    reader
      .read_exact(&mut name)
      .map_err(FileHeaderError::FileName)?;
    let name = String::from_utf8(name)?;

    // If the size doesn't match the actual size of the file header, then it's invalid
    if size != (6 + name.len()) as u16 {
      return Err(FileHeaderError::FileHeaderSizeMismatch {
        expected: size,
        actual: (6 + name.len()) as u16,
      });
    }

    Ok(Self {
      data_addr,
      data_len,
      name,
    })
  }

  pub fn write(&mut self, writer: &mut impl Write) -> io::Result<()> {
    let data_addr = self.data_addr.to_le_bytes();
    let data_len = self.data_len.to_le_bytes();
    let name_buf = self.name.as_bytes();

    let buf_len = (name_buf.len() as u16 + 6).to_le_bytes();

    writer.write_all(&buf_len)?;
    writer.write_all(&data_addr)?;
    writer.write_all(&data_len)?;
    writer.write_all(name_buf)?;

    Ok(())
  }

  /// Get the size of the file header
  pub fn len(&self) -> usize {
    self.name.len() + 6
  }

  pub fn is_empty(&self) -> bool {
    self.len() == 0
  }
}

#[derive(Debug)]
pub struct Filesystem {
  pub path: String,
  pub file: std::fs::File,
  pub table_size: usize,
  pub data_size: usize,
  pub memcache: Vec<u8>,
}

impl Filesystem {
  pub fn new(path: &str) -> Self {
    let file = std::fs::OpenOptions::new()
      .create(true)
      .write(true)
      .read(true)
      .open(path)
      .unwrap();

    Self {
      path: path.to_string(),
      file,
      table_size: TABLE_SIZE,
      data_size: TABLE_SIZE * 2,
      memcache: vec![],
    }
  }

  /// Flush the memory cache to the virtual disk
  pub fn flush(&mut self) {
    self.clear();
    let _ = self.file.write(&self.memcache);
  }

  /// Load the virtual disk into memory
  pub fn load(&mut self) {
    // Load the file into memory
    let mut buf = vec![0u8; self.table_size + self.data_size];
    let _ = self.file.read(&mut buf);
    self.memcache = buf;
  }

  /// Clear the filesystem
  pub fn clear(&mut self) {
    let buf = vec![0u8; self.table_size + self.data_size];
    self.file.seek(std::io::SeekFrom::Start(0)).unwrap();
    self.file.write_all(&buf).unwrap();
    self.file.seek(std::io::SeekFrom::Start(0)).unwrap();
  }

  /// Create a file in the filesystem
  pub fn create_file(
    &mut self,
    filename: String,
    content: String,
  ) -> Result<(), FileSystemError> {
    let content_buf = content.as_bytes();

    let table = self.memcache[0..self.table_size].to_vec();

    let mut last_table_addr = 0;
    let mut last_data_addr = 0;
    let mut seek_index = 0;
    loop {
      // If we've reached the end of the table, then we can't write anymore
      if (seek_index + 7) >= table.len() {
        return Err(FileSystemError::NoMoreSpaceInTable);
      }

      let size = u16::from_le_bytes([table[seek_index], table[seek_index + 1]]);

      // If we've hit an empty space, then we can write here
      if size == 0u16 {
        last_table_addr = seek_index;
        break;
      } else {
        // Otherwise, we need to skip over this file header
        let file_header = FileHeader::read(&mut &table[seek_index..]).unwrap();

        last_data_addr =
          file_header.data_addr as usize + file_header.data_len as usize;

        seek_index += size as usize;
      }
    }

    let mut file_header = FileHeader {
      data_addr: last_data_addr as u16,
      data_len: content_buf.len() as u16,
      name: filename,
    };

    // If the file header is too large, then we can't write it
    if (file_header.len() > TABLE_SIZE)
      || (file_header.len() + last_table_addr > TABLE_SIZE)
    {
      return Err(FileSystemError::NoMoreSpaceInTable);
    }

    if (last_data_addr + content_buf.len()) > self.data_size {
      return Err(FileSystemError::NoMoreSpace);
    }

    let mut cursor = Cursor::new(&mut self.memcache);
    cursor
      .seek(SeekFrom::Start(last_table_addr as u64))
      .unwrap();
    file_header.write(&mut cursor).unwrap();

    for (i, b) in content_buf.iter().enumerate() {
      self.memcache[self.table_size + last_data_addr + i] = *b;
    }

    self.flush();
    Ok(())
  }
}
