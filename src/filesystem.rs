use std::io::{self, Read, Seek, Write};

use thiserror::Error;

pub trait BlockAlign {
  const SIZE: usize;
  const COUNT: usize;

  fn total_size() -> usize {
    Self::SIZE * Self::COUNT
  }

  fn size() -> usize {
    Self::SIZE
  }

  fn count() -> usize {
    Self::COUNT
  }
}

struct BlockKindMain;
impl BlockAlign for BlockKindMain {
  const SIZE: usize = 32;
  const COUNT: usize = 1;
}

struct BlockKindHeader;
impl BlockAlign for BlockKindHeader {
  const SIZE: usize = 32;
  const COUNT: usize = 128;
}

struct BlockKindTitle;
impl BlockAlign for BlockKindTitle {
  const SIZE: usize = 32;
  const COUNT: usize = 128;
}

struct BlockKindData;
impl BlockAlign for BlockKindData {
  const SIZE: usize = 128;
  const COUNT: usize = 32;
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
    let buf = vec![0; size];
    self.inner.write_all(&buf)?;

    let mut needed_size = BlockKindMain::total_size();
    if size < needed_size {
      return Err(BulkError::TooSmallForMainSuperBlock);
    }

    needed_size += BlockKindHeader::total_size();
    if size < needed_size {
      return Err(BulkError::TooSmallForHeaderSuperBlock);
    }

    needed_size += BlockKindTitle::total_size();
    if size < needed_size {
      return Err(BulkError::TooSmallForTitleSuperBlock);
    }

    needed_size += BlockKindData::total_size();
    if size < needed_size {
      return Err(BulkError::TooSmallForDataSuperBlock);
    }

    Ok(())
  }
}
