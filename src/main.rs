use rfs::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mut filesystem = Filesystem::new("harddrive.bin");

  filesystem.load();
  filesystem
    .create_file("test.txt".to_string(), "This is a test.".to_string())?;

  Ok(())
}
