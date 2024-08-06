use std::io::{self, Read, Seek, SeekFrom, Write};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub trait BlockAlign {
  const SIZE: u64;
  const COUNT: u64;

  const IDENT: u8;

  fn super_block_size() -> u64 {
    Self::SIZE * Self::COUNT
  }

  fn block_size() -> u64 {
    Self::SIZE
  }

  fn block_count() -> u64 {
    Self::COUNT
  }

  fn ident() -> u8 {
    Self::IDENT
  }
}

#[repr(C)]
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
struct BlockKindMain {
  free_header_ptr: u64,
  free_title_ptr: u64,
  free_data_ptr: u64,

  /// This is unused but kept for alignment and for future use.
  unused_ptr: u64,
}
impl BlockAlign for BlockKindMain {
  const SIZE: u64 = 32;
  const COUNT: u64 = 1;

  /// This is not used. The main superblock is always at 0x0.
  const IDENT: u8 = 0;
}

#[repr(C)]
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
struct BlockKindHeader;
impl BlockAlign for BlockKindHeader {
  const SIZE: u64 = 32;
  const COUNT: u64 = 128;

  const IDENT: u8 = 1;
}

#[repr(C)]
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
struct BlockKindTitle;
impl BlockAlign for BlockKindTitle {
  const SIZE: u64 = 32;
  const COUNT: u64 = 128;

  const IDENT: u8 = 2;
}

#[repr(C)]
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
struct BlockKindData;
impl BlockAlign for BlockKindData {
  const SIZE: u64 = 128;
  const COUNT: u64 = 32;

  const IDENT: u8 = 3;
}

#[repr(C)]
struct SuperBlock<T>
where
  T: BlockAlign,
{
  blocks: Block<T>,
}

#[repr(C)]
struct Block<T>
where
  T: ?Sized,
{
  prev_block: usize,
  next_block: usize,
  data: T,
}

#[repr(C)]
struct FileHeader {
  start_title_block: usize,
  start_file_block: usize,
}

type FileHeaders = Block<FileHeader>;

#[derive(Debug, Error)]
pub enum BulkError {
  #[error("filesystem size is too small to allocate a main superblock")]
  TooSmallForMainSuperBlock,
  #[error("filesystem size is too small to allocate a header superblock")]
  TooSmallForHeaderSuperBlock,
  #[error("filesystem size is too small to allocate a title superblock")]
  TooSmallForTitleSuperBlock,
  #[error("filesystem size is too small to allocate a data superblock")]
  TooSmallForDataSuperBlock,

  #[error(transparent)]
  IO(#[from] io::Error),

  #[error(transparent)]
  Serde(#[from] bincode::Error),
}

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

  fn clear_and_check_size(&mut self, size: u64) -> Result<(), BulkError> {
    self.inner.seek(SeekFrom::Start(0))?;
    let buf = vec![0; size as usize];

    let mut needed_size = BlockKindMain::super_block_size();
    if size < needed_size {
      return Err(BulkError::TooSmallForMainSuperBlock);
    }

    needed_size += BlockKindHeader::super_block_size();
    if size < needed_size {
      return Err(BulkError::TooSmallForHeaderSuperBlock);
    }

    needed_size += BlockKindTitle::super_block_size();
    if size < needed_size {
      return Err(BulkError::TooSmallForTitleSuperBlock);
    }

    needed_size += BlockKindData::super_block_size();
    if size < needed_size {
      return Err(BulkError::TooSmallForDataSuperBlock);
    }

    self.inner.write_all(&buf)?;
    Ok(())
  }

  fn init_main_header(&mut self) -> Result<(), BulkError> {
    let header_sb_start = BlockKindMain::super_block_size();
    let title_sb_start = header_sb_start + BlockKindHeader::super_block_size();
    let data_sb_start = title_sb_start + BlockKindTitle::super_block_size();

    let main_header = BlockKindMain {
      free_header_ptr: header_sb_start,
      free_title_ptr: title_sb_start,
      free_data_ptr: data_sb_start,
      unused_ptr: 0,
    };

    let main_header_bytes = bincode::serialize(&main_header)?;

    self.inner.seek(SeekFrom::Start(0))?;
    self.inner.write_all(&main_header_bytes)?;
    Ok(())
  }

  fn init_superblocks(&mut self) -> Result<(), BulkError> {
    let header_sb_start = BlockKindMain::super_block_size();
    let title_sb_start = header_sb_start + BlockKindHeader::super_block_size();
    let data_sb_start = title_sb_start + BlockKindTitle::super_block_size();

    self.inner.seek(SeekFrom::Start(header_sb_start))?;
    let header_sb_ident = bincode::serialize(&BlockKindHeader::ident())?;
    self.inner.write_all(&header_sb_ident)?;

    self.inner.seek(SeekFrom::Start(title_sb_start))?;
    let title_sb_ident = bincode::serialize(&BlockKindTitle::ident())?;
    self.inner.write_all(&title_sb_ident)?;

    self.inner.seek(SeekFrom::Start(data_sb_start))?;
    let data_sb_ident = bincode::serialize(&BlockKindData::ident())?;
    self.inner.write_all(&data_sb_ident)?;

    Ok(())
  }

  pub fn init(&mut self, size: u64) -> Result<(), BulkError> {
    self.clear_and_check_size(size)?;
    self.init_main_header()?;
    self.init_superblocks()?;

    Ok(())
  }
}
