use std::io::Write;

#[derive(Debug)]
struct Filesystem {
  path: String,
  file: std::fs::File,
}

impl Filesystem {
  fn new(path: &str) -> Self {
    let mut file = std::fs::OpenOptions::new()
      .create(true)
      .write(true)
      .open(path)
      .unwrap();

    Self {
      path: path.to_string(),
      file,
    }
  }

  fn init(&mut self) {
    let buf = [0u8; 1024];
    self.file.write_all(&buf).unwrap();
  }
}

fn main() {
  let mut filesystem = Filesystem::new("harddrive.bin");

  filesystem.init();
}
