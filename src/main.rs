use std::fs::OpenOptions;

use rfs::filesystem::{
  BlockAlign, BlockKindData, BlockKindHeader, BlockKindMain, BlockKindTitle,
  File, Filesystem,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mut filesystem = Filesystem::new(
    OpenOptions::new()
      .read(true)
      .write(true)
      .create(true)
      .truncate(true)
      .open("harddrive.bin")?,
  );

  filesystem.init(
    BlockKindMain::super_block_size()
      + BlockKindHeader::super_block_size()
      + BlockKindTitle::super_block_size()
      + BlockKindData::super_block_size(),
  )?;

  filesystem.create(File::new(
    "hello.txt".to_owned(),
    "Hello, World!".to_owned(),
  ))?;
  filesystem.create(File::new(
    "hello2.txt".to_owned(),
    "Hello, from the filesystem!".to_owned(),
  ))?;

  println!("{:?}", filesystem.list());

  // let string = "Hello".to_owned();
  // let mut bytes = string.as_bytes().to_vec();
  // bytes.insert(0, bytes.len() as u8);
  // bytes.extend_from_slice(&[0, 0, 0, 0]);

  // let bytes_len = bytes.first().unwrap();
  // let trimmed_bytes = bytes
  //   .iter()
  //   .skip(1)
  //   .take(*bytes_len as usize)
  //   .copied()
  //   .collect::<Vec<_>>();
  // let from_bytes = std::str::from_utf8(&trimmed_bytes);

  // println!("bytes:   {:?}", bytes);
  // println!("trimmed: {:?}", trimmed_bytes);
  // println!("string:  {:?}", from_bytes);

  Ok(())
}
