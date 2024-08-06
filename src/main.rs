use std::fs::OpenOptions;

use rfs::filesystem::Filesystem;

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mut filesystem = Filesystem::new(
    OpenOptions::new()
      .read(true)
      .write(true)
      .create(true)
      .truncate(true)
      .open("harddrive.bin")?,
  );
  filesystem.init(4097 * 3 + 32)?;

  Ok(())
}
