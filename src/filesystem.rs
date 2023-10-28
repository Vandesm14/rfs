use std::io::{self, copy, Cursor, Read, Seek, SeekFrom, Write};
use thiserror::Error;

use crate::{FileHeader, FileHeaderError};

#[derive(Error, Debug)]
pub enum CreateFileError {
  #[error("No more space in the table")]
  NoMoreSpaceInTable,

  #[error("File name is larger than {} bytes", Filesystem::FILENAME_SIZE)]
  FileNameTooLarge,
}

#[derive(Error, Debug)]
pub enum ReadFileError {
  #[error("File not found")]
  FileNotFound,

  #[error("Error reading data")]
  DataReadError,

  #[error("Error reading file header")]
  FileHeaderReadError,
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
      let addr = Self::FS_HEADER_SIZE + Self::TABLE_ALIGN * i;
      self.memcache.set_position(addr as u64);
      let header = FileHeader::read(&mut self.memcache, addr as u16);

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
  pub fn write_file(
    &mut self,
    filename: String,
    content: String,
  ) -> Result<FileHeader, CreateFileError> {
    if filename.len() > Self::FILENAME_SIZE {
      return Err(CreateFileError::FileNameTooLarge);
    }

    // Read the filesystem header
    self.memcache.set_position(0);
    let mut fs_header = FSHeader::read(&mut self.memcache).unwrap();

    // Check if we have reached max headers
    if fs_header.headers >= Filesystem::TOTAL_HEADERS as u8 {
      return Err(CreateFileError::NoMoreSpaceInTable);
    }

    // Calculate the address we will write the header to
    let mut header_addr = fs_header.headers as usize * Filesystem::TABLE_ALIGN
      + Filesystem::FS_HEADER_SIZE;

    // Calculate the address we will write the data to
    let data_addr = fs_header.free_addr as usize;

    // Calculate the start of the data blocks
    let data_offset = Filesystem::TABLE_SIZE;

    // Check if the file already exists
    let existing_header = self.get_file_header(filename.clone()).unwrap();
    if let Some(header) = &existing_header {
      header_addr = header.addr as usize;
    }

    // Create the file header
    let mut file_header = FileHeader {
      data_addr: data_addr as u16,
      data_len: content.len() as u16,
      name: filename,
      addr: header_addr as u16,
    };

    // Write the header
    self.memcache.set_position(header_addr as u64);
    file_header.write(&mut self.memcache).unwrap();

    // Write the data
    self.memcache.set_position((data_addr + data_offset) as u64);
    self.memcache.write_all(content.as_bytes()).unwrap();

    // Update the filesystem header
    if existing_header.is_none() {
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
  pub fn read_file(
    &mut self,
    filename: String,
  ) -> Result<String, ReadFileError> {
    let header = Self::get_file_header(self, filename);

    // Check if the read was successful
    match header {
      // If there was an error reading the header, return an error
      Err(_) => Err(ReadFileError::FileHeaderReadError),

      // Else, check if the header exists
      Ok(header) => match header {
        // If the header does not exist, return an error
        None => Err(ReadFileError::FileNotFound),

        // If the header exists, read the file data
        Some(header) => match Self::get_file_data(self, header) {
          // If there was an error reading the data, return an error
          Err(_) => Err(ReadFileError::DataReadError),

          // Else, return the data
          Ok(data) => Ok(data),
        },
      },
    }
  }
}

pub fn stream_len(cursor: &mut Cursor<Vec<u8>>) -> io::Result<u64> {
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
  use crate::{stream_len, CreateFileError, Filesystem};

  #[test]
  fn test_create_file() {
    let mut filesystem = Filesystem::new(None);

    let title = "test.txt";
    let content = "This is a test.";

    filesystem.load();
    filesystem
      .write_file(title.to_string(), content.to_string())
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
      .write_file(title.to_string(), content.to_string())
      .unwrap();
    let header2 = filesystem
      .write_file(title2.to_string(), content2.to_string())
      .unwrap();

    // The filesystem should contain space for all file headers, the filesystem header itself, and the data
    assert_eq!(
      stream_len(&mut filesystem.memcache).unwrap() as usize,
      Filesystem::TABLE_SIZE + content.len() + content2.len()
    );

    // The first header should contain the first data
    let data = filesystem.get_file_data(header.clone()).unwrap();
    assert_eq!(data, content);

    // The second header should contain the second data
    let data2 = filesystem.get_file_data(header2.clone()).unwrap();
    assert_eq!(data2, content2);

    // The first header should be in the first header position
    assert_eq!(header.addr, Filesystem::FS_HEADER_SIZE as u16);

    // The second header should be in the second header position
    assert_eq!(
      header2.addr,
      (Filesystem::FS_HEADER_SIZE + Filesystem::TABLE_ALIGN) as u16
    );
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
        .write_file(format!("{title}{i}"), content.to_string())
        .unwrap();
    }

    // Try to create another file
    let result = filesystem.write_file(title.to_string(), content.to_string());

    // The filesystem should return an error
    assert!(matches!(result, Err(CreateFileError::NoMoreSpaceInTable)));
  }

  #[test]
  fn test_file_name_too_large() {
    let mut filesystem = Filesystem::new(None);

    let title = "this is a file with a name that is too large.txt";
    let content = "This is a test.";

    filesystem.load();

    // Create a file with a name that is too large
    let result = filesystem.write_file(title.to_string(), content.to_string());

    // The filesystem should return an error
    assert!(matches!(result, Err(CreateFileError::FileNameTooLarge)));
  }

  #[test]
  fn test_overwrite_with_same_name() {
    let mut filesystem = Filesystem::new(None);

    let title = "test.txt";
    let content = "This is a test.";
    let content2 = "This is another test.";

    filesystem.load();
    let header = filesystem
      .write_file(title.to_string(), content.to_string())
      .unwrap();
    let header2 = filesystem
      .write_file(title.to_string(), content2.to_string())
      .unwrap();

    // The filesystem should contain the new and old data
    assert_eq!(
      stream_len(&mut filesystem.memcache).unwrap() as usize,
      Filesystem::TABLE_SIZE + content.len() + content2.len()
    );

    // The first header should contain the first data
    let data = filesystem.get_file_data(header.clone()).unwrap();
    assert_eq!(data, content);

    // The second header should contain the second data
    let data2 = filesystem.get_file_data(header2.clone()).unwrap();
    assert_eq!(data2, content2);

    // The first header should be at the first header position
    assert_eq!(header.addr, Filesystem::FS_HEADER_SIZE as u16);

    // The second header should be at the same position
    assert_eq!(header2.addr, Filesystem::FS_HEADER_SIZE as u16);
  }

  #[test]
  fn test_read_file_data() {
    let mut filesystem = Filesystem::new(None);

    let title = "test.txt";
    let content = "This is a test.";

    filesystem.load();
    filesystem
      .write_file(title.to_string(), content.to_string())
      .unwrap();

    // The file should contain the data
    let data = filesystem.read_file(title.to_string()).unwrap();
    assert_eq!(data, content);

    // Overwrite the file contents with new data
    let content2 = "This is another test.";
    filesystem
      .write_file(title.to_string(), content2.to_string())
      .unwrap();

    // The file should contain the new data
    let data2 = filesystem.read_file(title.to_string()).unwrap();
    assert_eq!(data2, content2);
  }
}
