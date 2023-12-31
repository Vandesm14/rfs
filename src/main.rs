use rfs::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mut filesystem = Filesystem::new(Some("harddrive.bin"));

  filesystem.load();
  filesystem
    .write_file("test.txt".to_string(), "This is a test.".to_string())?;
  filesystem
    .write_file("test2.txt".to_string(), "This is another test.".to_string())?;

  Ok(())
}
