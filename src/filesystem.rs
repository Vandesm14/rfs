use std::io::{self, Read, Seek, Write};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub trait BlockAlign {
  const SIZE: usize;
  const COUNT: usize;

  const IDENT: u8;

  fn total_size() -> usize {
    Self::SIZE * Self::COUNT
  }

  fn size() -> usize {
    Self::SIZE
  }

  fn count() -> usize {
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
  free_header_ptr: usize,
  free_title_ptr: usize,
  free_data_ptr: usize,

  /// This is unused but kept for alignment and for future use.
  unused_ptr: usize,
}
impl BlockAlign for BlockKindMain {
  const SIZE: usize = 32;
  const COUNT: usize = 1;

  /// This is not used. The main superblock is always at 0x0.
  const IDENT: u8 = 0;
}

#[repr(C)]
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
struct BlockKindHeader;
impl BlockAlign for BlockKindHeader {
  const SIZE: usize = 32;
  const COUNT: usize = 128;

  const IDENT: u8 = 1;
}

#[repr(C)]
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
struct BlockKindTitle;
impl BlockAlign for BlockKindTitle {
  const SIZE: usize = 32;
  const COUNT: usize = 128;

  const IDENT: u8 = 2;
}

#[repr(C)]
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
struct BlockKindData;
impl BlockAlign for BlockKindData {
  const SIZE: usize = 128;
  const COUNT: usize = 32;

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

  pub fn init(&mut self, size: usize) -> Result<(), BulkError> {
    self.inner.seek(std::io::SeekFrom::Start(0))?;
    let mut cursor = 0;
    let mut buf = vec![0; size];

    let mut needed_size = BlockKindMain::total_size();
    if size < needed_size {
      return Err(BulkError::TooSmallForMainSuperBlock);
    }

    let mut main_super_block = BlockKindMain {
      free_header_ptr: needed_size,
      free_title_ptr: 0,
      free_data_ptr: 0,
      unused_ptr: 0,
    };

    buf[needed_size] = BlockKindHeader::ident();
    main_super_block.free_header_ptr = needed_size + 1;

    needed_size += BlockKindHeader::total_size();
    if size < needed_size {
      return Err(BulkError::TooSmallForHeaderSuperBlock);
    }

    buf[needed_size] = BlockKindTitle::ident();
    main_super_block.free_title_ptr = needed_size + 1;

    needed_size += BlockKindTitle::total_size();
    if size < needed_size {
      return Err(BulkError::TooSmallForTitleSuperBlock);
    }

    buf[needed_size] = BlockKindData::ident();
    main_super_block.free_data_ptr = needed_size + 1;

    needed_size += BlockKindData::total_size();
    if size < needed_size {
      return Err(BulkError::TooSmallForDataSuperBlock);
    }

    let main_super_block_bytes = bincode::serialize(&main_super_block);
    match main_super_block_bytes {
      Ok(bytes) => {
        for (i, byte) in bytes.into_iter().enumerate() {
          buf[i] = byte;
        }
      }
      Err(_) => todo!(),
    }

    self.inner.write_all(&buf)?;
    Ok(())
  }
}
