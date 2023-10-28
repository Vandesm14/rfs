use std::io::{self, Read, Write};
use thiserror::Error;

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
  /// The address of the data (relative to the start of the data table)
  pub data_addr: u16,

  /// The length of the data
  pub data_len: u16,

  /// The name of the file
  pub name: String,

  /// The address of the file header in the table
  pub addr: u16,
}

impl FileHeader {
  pub fn read(
    reader: &mut impl Read,
    addr: u16,
  ) -> Result<Self, FileHeaderError> {
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
      addr,
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
