use std::io::{Read, Seek, Write};

#[derive(Debug)]
struct Filesystem {
  path: String,
  file: std::fs::File,
  table_size: usize,
  total_size: usize,
  memcache: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Delimiters {
  Separator = 128,
}

impl Filesystem {
  fn new(path: &str) -> Self {
    let mut file = std::fs::OpenOptions::new()
      .create(true)
      .write(true)
      .read(true)
      .open(path)
      .unwrap();

    Self {
      path: path.to_string(),
      file,
      table_size: 256,
      total_size: 256 * 3,
      memcache: vec![],
    }
  }

  /// Flush the memory cache to the file
  fn flush(&mut self) {
    self.clear();
    let _ = self.file.write(&self.memcache);
  }

  /// Load the file into memory
  fn load(&mut self) {
    // Load the file into memory
    let mut buf = vec![0u8; self.total_size];
    let _ = self.file.read(&mut buf);
    self.memcache = buf;
  }

  /// Clear the file
  fn clear(&mut self) {
    let buf = vec![0u8; self.total_size];
    self.file.seek(std::io::SeekFrom::Start(0)).unwrap();
    self.file.write_all(&buf).unwrap();
    self.file.seek(std::io::SeekFrom::Start(0)).unwrap();
  }

  /// Create a file in the filesystem
  fn create_file(&mut self, filename: String, content: String) {
    let name_buf = filename.as_bytes();
    let table = self.memcache[0..self.table_size].to_vec();
    // let last_table_addr = table
    //   .into_iter()
    //   .enumerate()
    //   .rev()
    //   .find(|(_, b)| *b != 0)
    //   .unwrap_or((0, 0))
    //   .0;
    let last_table_addr = 0;

    // let mut buf: Vec<u8> = vec![name_buf.len() as u8];

    /*
      - len of header (u16)
      - addr of data (u16)
      - len of data (u16)
      - name (utf-8)
    */

    let content_buf = content.as_bytes();

    let mut buf: Vec<u8> = vec![0, 0];
    let data_addr = (self.table_size as u16).to_le_bytes();
    let data_len = (content_buf.len() as u16).to_le_bytes();

    buf.extend_from_slice(&data_addr);
    buf.extend_from_slice(&data_len);
    buf.extend_from_slice(name_buf);

    let buf_len = (buf.len() as u16).to_le_bytes();
    buf[0] = buf_len[0];
    buf[1] = buf_len[1];

    for (i, b) in buf.iter().enumerate() {
      self.memcache[last_table_addr + i] = *b;
    }

    for (i, b) in content_buf.iter().enumerate() {
      self.memcache[self.table_size + i] = *b;
    }

    self.flush();
  }

  /// Destroy the filesystem (deletes the db file)
  fn destroy(&self) {
    std::fs::remove_file(&self.path).unwrap();
  }
}

fn main() {
  let mut filesystem = Filesystem::new("harddrive.bin");

  filesystem.clear();
  filesystem.load();

  filesystem.create_file("test.txt".to_string(), "Hello, world!".to_string());
}
