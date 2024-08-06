use std::fs::OpenOptions;

use rfs::filesystem::{
  BlockAlign, BlockKindData, BlockKindHeader, BlockKindMain, BlockKindTitle,
  Filesystem,
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

  filesystem.insert("main.rs".into(), "Hello, Mr. Filesystem!".into())?;

  Ok(())
}
