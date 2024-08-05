use std::io::{Read, Seek, Write};

pub struct Filesystem<T>
where
  T: Read + Write + Seek,
{
  inner: T,
}

impl<T> Filesystem<T>
where
  T: Read + Write + Seek,
{
  pub fn new(inner: T) -> Self {
    Filesystem { inner }
  }

  pub fn init(
    &mut self,
    size: usize,
  ) -> Result<(), Box<dyn std::error::Error>> {
    self.inner.seek(std::io::SeekFrom::Start(0))?;
    let buf = vec![0; size];
    self.inner.write_all(&buf)?;
    Ok(())
  }
}
