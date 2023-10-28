use std::io::{self, copy, Cursor, Read, Seek, SeekFrom, Write};
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
  pub memcache: Cursor<Vec<u8>>,
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
        memcache: Cursor::new(vec![]),
      }
    } else {
      Self {
        path: None,
        file: None,
        memcache: Cursor::new(vec![]),
      }
    }
  }

  /// Flush the memory cache to the virtual disk
  pub fn flush(&mut self) {
    if let Some(file) = &mut self.file {
      file.seek(std::io::SeekFrom::Start(0)).unwrap();

      self.memcache.set_position(0);
      let _ = copy(&mut self.memcache, file);
      file.seek(std::io::SeekFrom::Start(0)).unwrap();
    }
  }

  /// Load the virtual disk into memory
  pub fn load(&mut self) {
    if let Some(file) = &mut self.file {
      let _ = copy(file, &mut self.memcache);
    }

    self.init();
  }

  /// Initialize the virtual disk
  fn init(&mut self) {
    if stream_len(&mut self.memcache).unwrap() > 0 {
      return;
    }

    // Write zeros for the filesystem header and file headers
    let buf = vec![0u8; Self::TABLE_SIZE];
    self.memcache.set_position(0);
    self.memcache.write_all(&buf).unwrap();

    self.flush();
  }

  /// Scans the header table into memory
  fn scan_headers(&mut self) -> Result<Vec<FileHeader>, FileHeaderError> {
    let mut headers: Vec<FileHeader> = vec![];

    // Skip the filesystem header
    self.memcache.set_position(Self::FS_HEADER_SIZE as u64);

    for i in 0..Self::TOTAL_HEADERS {
      // Set the cursor position to the start of the header
      self.memcache.set_position(
        (Self::FS_HEADER_SIZE as u64) + (Self::TABLE_ALIGN as u64) * (i as u64),
      );
      let header = FileHeader::read(&mut self.memcache);

      headers.push(header?);
    }

    Ok(headers)
  }

  /// Gets a file header from the filesystem
  fn get_file_header(
    &mut self,
    filename: String,
  ) -> Result<Option<FileHeader>, FileHeaderError> {
    let headers = self.scan_headers()?;

    for header in headers {
      if header.name == filename {
        return Ok(Some(header));
      }
    }

    Ok(None)
  }

  /// Gets the address of a file header from the filesystem
  fn get_file_header_addr(
    &mut self,
    filename: String,
  ) -> Result<Option<usize>, FileHeaderError> {
    let headers = self.scan_headers()?;

    for (i, header) in headers.iter().enumerate() {
      if header.name == filename {
        return Ok(Some(i * Self::TABLE_ALIGN + Self::FS_HEADER_SIZE));
      }
    }

    Ok(None)
  }

  /// Reads the data of a file given a file header
  fn get_file_data(
    &mut self,
    header: FileHeader,
  ) -> Result<String, Box<dyn std::error::Error>> {
    let mut data = vec![0u8; header.data_len as usize];
    self
      .memcache
      .set_position(header.data_addr as u64 + Filesystem::TABLE_SIZE as u64);
    self.memcache.read_exact(&mut data)?;

    Ok(String::from_utf8(data)?)
  }

  /// Create a file in the filesystem
  pub fn create_file(
    &mut self,
    filename: String,
    content: String,
  ) -> Result<FileHeader, FileSystemError> {
    if filename.len() > Self::FILENAME_SIZE {
      return Err(FileSystemError::FileNameTooLarge);
    }

    // Read the filesystem header
    self.memcache.set_position(0);
    let mut fs_header = FSHeader::read(&mut self.memcache).unwrap();

    // Check if we have reached max headers
    if fs_header.headers >= Filesystem::TOTAL_HEADERS as u8 {
      return Err(FileSystemError::NoMoreSpaceInTable);
    }

    // Calculate the address we will write the header to
    let mut header_addr = fs_header.headers as usize * Filesystem::TABLE_ALIGN
      + Filesystem::FS_HEADER_SIZE;

    // Calculate the address we will write the data to
    let data_addr = fs_header.free_addr as usize;

    // Calculate the start of the data blocks
    let data_offset = Filesystem::TABLE_SIZE;

    // Check if the file already exists
    let existing_header_addr =
      self.get_file_header_addr(filename.clone()).unwrap();
    if let Some(addr) = existing_header_addr {
      header_addr = addr;
    }

    // Create the file header
    let mut file_header = FileHeader {
      data_addr: data_addr as u16,
      data_len: content.len() as u16,
      name: filename,
    };

    // Write the header
    self.memcache.set_position(header_addr as u64);
    file_header.write(&mut self.memcache).unwrap();

    // Write the data
    self.memcache.set_position((data_addr + data_offset) as u64);
    self.memcache.write_all(content.as_bytes()).unwrap();

    // Update the filesystem header
    if existing_header_addr.is_none() {
      // If we updated the header, we don't need to increment the header count
      fs_header.headers += 1;
    }
    fs_header.free_addr = data_addr as u16 + content.len() as u16;

    self.memcache.seek(SeekFrom::Start(0)).unwrap();
    fs_header.write(&mut self.memcache).unwrap();

    self.flush();
    Ok(file_header)
  }

  /// Read a file from the filesystem
  pub fn read_file() {
    todo!();
  }
}

fn stream_len(cursor: &mut Cursor<Vec<u8>>) -> io::Result<u64> {
  let old_pos = cursor.stream_position()?;
  let len = cursor.seek(SeekFrom::End(0))?;

  // Avoid seeking a third time when we were already at the end of the
  // stream. The branch is usually way cheaper than a seek operation.
  if old_pos != len {
    cursor.seek(SeekFrom::Start(old_pos))?;
  }

  cursor.set_position(0);

  Ok(len)
}

#[cfg(test)]
mod tests {
  use crate::{stream_len, FileSystemError, Filesystem};

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
      stream_len(&mut filesystem.memcache).unwrap() as usize,
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
    let header = filesystem
      .create_file(title.to_string(), content.to_string())
      .unwrap();
    let header2 = filesystem
      .create_file(title2.to_string(), content2.to_string())
      .unwrap();

    // The filesystem should contain space for all file headers, the filesystem header itself, and the data
    assert_eq!(
      stream_len(&mut filesystem.memcache).unwrap() as usize,
      Filesystem::TABLE_SIZE + content.len() + content2.len()
    );

    // The first header should contain the first data
    let data = filesystem.get_file_data(header).unwrap();
    assert_eq!(data, content);

    // The second header should contain the second data
    let data2 = filesystem.get_file_data(header2).unwrap();
    assert_eq!(data2, content2);
  }

  #[test]
  fn test_too_many_headers() {
    let mut filesystem = Filesystem::new(None);

    let title = "test.txt";
    let content = "This is a test.";

    filesystem.load();

    // Create the maximum number of files
    for i in 0..Filesystem::TOTAL_HEADERS {
      filesystem
        .create_file(format!("{title}{i}"), content.to_string())
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

  #[test]
  fn test_overwrite_with_same_name() {
    let mut filesystem = Filesystem::new(None);

    let title = "test.txt";
    let content = "This is a test.";
    let content2 = "This is another test.";

    filesystem.load();
    let header = filesystem
      .create_file(title.to_string(), content.to_string())
      .unwrap();
    let header2 = filesystem
      .create_file(title.to_string(), content2.to_string())
      .unwrap();

    // The filesystem should contain the new and old data
    assert_eq!(
      stream_len(&mut filesystem.memcache).unwrap() as usize,
      Filesystem::TABLE_SIZE + content.len() + content2.len()
    );

    // The first header should contain the first data
    let data = filesystem.get_file_data(header).unwrap();
    assert_eq!(data, content);

    // The second header should contain the second data
    let data2 = filesystem.get_file_data(header2).unwrap();
    assert_eq!(data2, content2);
  }
}
