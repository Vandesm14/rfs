use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FileSystemError {
  #[error("No more space in the table")]
  NoMoreSpaceInTable,

  #[error("File name is larger than {} bytes", Filesystem::FILENAME_SIZE)]
  FileNameTooLarge,
}

#[derive(Debug)]
/// The header at the top of a virtual disk file
/// - headers (u8) how many file headers there are
/// - free_addr (u16) the address of the next free data space
pub struct FSHeader {
  headers: u8,
  free_addr: u16,
}

impl FSHeader {
  pub fn read(reader: &mut impl Read) -> io::Result<Self> {
    let mut headers = [0u8; 1];
    reader.read_exact(&mut headers)?;
    let headers = u8::from_le_bytes(headers);

    let mut free_addr = [0u8; 2];
    reader.read_exact(&mut free_addr)?;
    let free_addr = u16::from_le_bytes(free_addr);

    Ok(Self { headers, free_addr })
  }

  pub fn write(&mut self, writer: &mut impl Write) -> io::Result<()> {
    writer.write_all(&self.headers.to_le_bytes())?;
    writer.write_all(&self.free_addr.to_le_bytes())?;

    Ok(())
  }
}

#[derive(Error, Debug)]
pub enum FileHeaderError {
  #[error("Could not read data address")]
  DataAddress(io::Error),

  #[error("Could not read data length")]
  DataLength(io::Error),

  #[error("Could not read file name")]
  FileName(io::Error),

  #[error(transparent)]
  InvalidUTF8(#[from] std::string::FromUtf8Error),
}

/// File Header Spec:
/// - addr of data (u16)
/// - len of data (u16)
/// - len of name (u8) (max 16)
/// - name (char bytes; len = len of name)
pub struct FileHeader {
  data_addr: u16,
  data_len: u16,
  name: String,
}

impl FileHeader {
  pub fn read(reader: &mut impl Read) -> Result<Self, FileHeaderError> {
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

    let mut name_len = [0u8; 1];
    reader
      .read_exact(&mut name_len)
      .map_err(FileHeaderError::FileName)?;
    let name_len = u8::from_le_bytes(name_len);

    let mut name = vec![0u8; name_len as usize];
    reader
      .read_exact(&mut name)
      .map_err(FileHeaderError::FileName)?;
    let name = String::from_utf8(name)?;

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

    let name_len = (name_buf.len() as u8).to_le_bytes();

    writer.write_all(&data_addr)?;
    writer.write_all(&data_len)?;
    writer.write_all(&name_len)?;
    writer.write_all(name_buf)?;

    Ok(())
  }
}

#[derive(Debug)]
pub struct Filesystem {
  pub path: Option<String>,
  pub file: Option<std::fs::File>,
  pub memcache: Vec<u8>,
}

impl Filesystem {
  /// The size of the filesystem header (for storing state)
  pub const FS_HEADER_SIZE: usize = 16;

  // File Header Spec:
  // ```txt
  // |            bytes             |
  // | addr | len | name_len | name |
  // | 2    | 2   | 1        | 16   |
  // ```

  /// The max size in bytes of a file name
  pub const FILENAME_SIZE: usize = 16;

  /// The set size of a file header (alignment)
  pub const TABLE_ALIGN: usize = Self::FILENAME_SIZE + 5;

  /// The total number of file headers that can be stored
  pub const TOTAL_HEADERS: usize = 10;

  /// The total size of the virtual disk (excluding file data)
  pub const TABLE_SIZE: usize =
    Self::TABLE_ALIGN * Self::TOTAL_HEADERS + Self::FS_HEADER_SIZE;

  pub fn new(path: Option<&str>) -> Self {
    if let Some(path) = path {
      let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .open(path)
        .unwrap();

      Self {
        path: Some(path.to_string()),
        file: Some(file),
        memcache: vec![],
      }
    } else {
      Self {
        path: None,
        file: None,
        memcache: vec![],
      }
    }
  }

  /// Flush the memory cache to the virtual disk
  pub fn flush(&mut self) {
    if let Some(file) = &mut self.file {
      file.seek(std::io::SeekFrom::Start(0)).unwrap();
      let _ = file.write(&self.memcache);
      file.seek(std::io::SeekFrom::Start(0)).unwrap();
    }
  }

  /// Load the virtual disk into memory
  pub fn load(&mut self) {
    if let Some(file) = &mut self.file {
      let mut buf = vec![];
      // Load the file into memory
      let _ = file.read_to_end(&mut buf);
      self.memcache = buf;
    }

    self.init();
  }

  /// Initialize the virtual disk
  fn init(&mut self) {
    if !self.memcache.is_empty() {
      return;
    }

    // Write zeros for the filesystem header and file headers
    let buf = vec![0u8; Self::TABLE_SIZE];

    self.memcache = buf;
    self.flush();
  }

  /// Create a file in the filesystem
  pub fn create_file(
    &mut self,
    filename: String,
    content: String,
  ) -> Result<(), FileSystemError> {
    let mut cursor = Cursor::new(&mut self.memcache);

    if filename.len() > Self::FILENAME_SIZE {
      return Err(FileSystemError::FileNameTooLarge);
    }

    // Read the filesystem header
    cursor.seek(SeekFrom::Start(0)).unwrap();
    let mut fs_header = FSHeader::read(&mut cursor).unwrap();

    // Check if we have reached max headers
    if fs_header.headers >= Filesystem::TOTAL_HEADERS as u8 {
      return Err(FileSystemError::NoMoreSpaceInTable);
    }

    // Calculate the address we will write the header to
    let header_addr = fs_header.headers as usize * Filesystem::TABLE_ALIGN
      + Filesystem::FS_HEADER_SIZE;

    // Calculate the address we will write the data to
    let data_addr = fs_header.free_addr as usize;

    // Calculate the start of the data blocks
    let data_offset = Filesystem::TABLE_SIZE;

    // Create the file header
    let mut file_header = FileHeader {
      data_addr: data_addr as u16,
      data_len: content.len() as u16,
      name: filename,
    };

    // Write the header
    cursor.seek(SeekFrom::Start(header_addr as u64)).unwrap();
    file_header.write(&mut cursor).unwrap();

    // Write the data
    cursor
      .seek(SeekFrom::Start((data_addr + data_offset) as u64))
      .unwrap();
    cursor.write_all(content.as_bytes()).unwrap();

    // Update the filesystem header
    fs_header.headers += 1;
    fs_header.free_addr = data_addr as u16 + content.len() as u16;

    cursor.seek(SeekFrom::Start(0)).unwrap();
    fs_header.write(&mut cursor).unwrap();

    self.flush();
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use crate::{FileSystemError, Filesystem};

  #[test]
  fn test_create_file() {
    let mut filesystem = Filesystem::new(None);

    let title = "test.txt";
    let content = "This is a test.";

    filesystem.load();
    filesystem
      .create_file(title.to_string(), content.to_string())
      .unwrap();

    // The filesystem should contain space for all file headers, the filesystem header itself, and the data
    assert_eq!(
      filesystem.memcache.len(),
      Filesystem::TABLE_SIZE + content.len()
    );
  }

  #[test]
  fn test_create_different_files() {
    let mut filesystem = Filesystem::new(None);

    let title = "test.txt";
    let content = "This is a test.";

    let title2 = "test2.txt";
    let content2 = "This is another test.";

    filesystem.load();
    filesystem
      .create_file(title.to_string(), content.to_string())
      .unwrap();
    filesystem
      .create_file(title2.to_string(), content2.to_string())
      .unwrap();

    // The filesystem should contain space for all file headers, the filesystem header itself, and the data
    assert_eq!(
      filesystem.memcache.len(),
      Filesystem::TABLE_SIZE + content.len() + content2.len()
    );
  }

  #[test]
  fn test_too_many_headers() {
    let mut filesystem = Filesystem::new(None);

    let title = "test.txt";
    let content = "This is a test.";

    filesystem.load();

    // Create the maximum number of files
    for _ in 0..Filesystem::TOTAL_HEADERS {
      filesystem
        .create_file(title.to_string(), content.to_string())
        .unwrap();
    }

    // Try to create another file
    let result = filesystem.create_file(title.to_string(), content.to_string());

    // The filesystem should return an error
    assert!(matches!(result, Err(FileSystemError::NoMoreSpaceInTable)));
  }

  #[test]
  fn test_file_name_too_large() {
    let mut filesystem = Filesystem::new(None);

    let title = "this is a file with a name that is too large.txt";
    let content = "This is a test.";

    filesystem.load();

    // Create a file with a name that is too large
    let result = filesystem.create_file(title.to_string(), content.to_string());

    // The filesystem should return an error
    assert!(matches!(result, Err(FileSystemError::FileNameTooLarge)));
  }
}
