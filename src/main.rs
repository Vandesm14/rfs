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
    let buf = vec![0u8; 0];
    self.file.seek(std::io::SeekFrom::Start(0)).unwrap();
    self.file.write_all(&buf).unwrap();
  }

  /// Create a file in the filesystem
  fn create_file(&mut self, filename: String, content: String) {
    let name_buf = filename.as_bytes();
    let table = self.memcache[0..self.table_size].to_vec();
    let last_table_addr = table
      .into_iter()
      .enumerate()
      .rev()
      .find(|(_, b)| *b != 0)
      .unwrap_or((0, 0))
      .0;

    println!("last_table_addr: {}", last_table_addr);

    for (i, b) in name_buf.iter().enumerate() {
      self.memcache[dbg!(last_table_addr + i)] = *b;
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
