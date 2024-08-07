use std::io::{self, Read, Seek, SeekFrom, Write};

use serde::{ser::SerializeTuple, Deserialize, Serialize, Serializer};
use thiserror::Error;

pub fn pad_with_byte_size(vec: Vec<u8>, size: u64) -> Vec<u8> {
  [size.to_le_bytes().to_vec(), vec].concat()
}

pub trait BlockAlign {
  const HEADER_SIZE: u64;
  const SIZE: u64;
  const COUNT: u64;

  fn block_size() -> u64 {
    Self::SIZE
  }

  fn block_count() -> u64 {
    Self::COUNT
  }

  fn header_size() -> u64 {
    Self::HEADER_SIZE
  }

  fn super_block_size() -> u64 {
    Self::SIZE * Self::COUNT + Self::HEADER_SIZE
  }

  fn initial_header() -> Vec<u8>;
}

#[repr(C)]
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
pub struct BlockKindMain {
  free_header_ptr: u64,
  free_title_ptr: u64,
  free_data_ptr: u64,

  /// Points to the first header of a valid file. If the value is `0`, then
  /// there is no valid first header.
  first_header_ptr: u64,
}
impl BlockAlign for BlockKindMain {
  const HEADER_SIZE: u64 = 32;
  const SIZE: u64 = 0;
  const COUNT: u64 = 0;

  fn initial_header() -> Vec<u8> {
    [0; Self::HEADER_SIZE as usize].to_vec()
  }
}

#[repr(C)]
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
pub struct BlockKindHeader;
impl BlockAlign for BlockKindHeader {
  const HEADER_SIZE: u64 = 16;
  const SIZE: u64 = 32;
  const COUNT: u64 = 128;

  fn initial_header() -> Vec<u8> {
    let ident: u8 = 1;
    [vec![ident], vec![0; Self::HEADER_SIZE as usize - 1]].concat()
  }
}

#[repr(C)]
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
pub struct BlockKindTitle;
impl BlockAlign for BlockKindTitle {
  const HEADER_SIZE: u64 = 16;
  const SIZE: u64 = 32;
  const COUNT: u64 = 128;

  fn initial_header() -> Vec<u8> {
    let ident: u8 = 2;
    [vec![ident], vec![0; Self::HEADER_SIZE as usize - 1]].concat()
  }
}

#[repr(C)]
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
pub struct BlockKindData;
impl BlockAlign for BlockKindData {
  const HEADER_SIZE: u64 = 16;
  const SIZE: u64 = 128;
  const COUNT: u64 = 32;

  fn initial_header() -> Vec<u8> {
    let ident: u8 = 3;
    [vec![ident], vec![0; Self::HEADER_SIZE as usize - 1]].concat()
  }
}

#[repr(C)]
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
pub struct SuperBlock<T>
where
  T: BlockAlign,
{
  blocks: Block<T>,
}

#[repr(C)]
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
pub struct Block<T>
where
  T: ?Sized,
{
  prev_block: u64,
  next_block: u64,
  data: T,
}

#[repr(C)]
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
pub struct FileHeader {
  start_title_block: u64,
  start_data_block: u64,
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct FileTitle {
  data: Vec<u8>,
}

impl Serialize for FileTitle {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    let mut ser_tuple = serializer.serialize_tuple(self.data.len())?;
    for elem in &self.data {
      ser_tuple.serialize_element(elem)?;
    }
    ser_tuple.end()
  }
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct FileData {
  pub data: Vec<u8>,
}

impl Serialize for FileData {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    let mut ser_tuple = serializer.serialize_tuple(self.data.len())?;
    for elem in &self.data {
      ser_tuple.serialize_element(elem)?;
    }
    ser_tuple.end()
  }
}

#[derive(Debug, Error)]
pub enum InitializationError {
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

#[derive(Debug, Error)]
pub enum GenericError {
  #[error(transparent)]
  IO(#[from] io::Error),

  #[error(transparent)]
  Serde(#[from] bincode::Error),
}

#[derive(Debug, Error)]
pub enum FilesystemError {
  #[error(transparent)]
  InitializationError(InitializationError),
}

impl From<InitializationError> for FilesystemError {
  fn from(value: InitializationError) -> Self {
    Self::InitializationError(value)
  }
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

  fn clear_and_check_size(
    &mut self,
    size: u64,
  ) -> Result<(), InitializationError> {
    self.inner.seek(SeekFrom::Start(0))?;
    let buf = vec![0; size as usize];

    let mut needed_size = BlockKindMain::super_block_size();
    if size < needed_size {
      return Err(InitializationError::TooSmallForMainSuperBlock);
    }

    needed_size += BlockKindHeader::super_block_size();
    if size < needed_size {
      return Err(InitializationError::TooSmallForHeaderSuperBlock);
    }

    needed_size += BlockKindTitle::super_block_size();
    if size < needed_size {
      return Err(InitializationError::TooSmallForTitleSuperBlock);
    }

    needed_size += BlockKindData::super_block_size();
    if size < needed_size {
      return Err(InitializationError::TooSmallForDataSuperBlock);
    }

    self.inner.write_all(&buf)?;

    Ok(())
  }

  fn init_main_header(&mut self) -> Result<(), InitializationError> {
    let header_sb_start = BlockKindMain::super_block_size();
    let title_sb_start = header_sb_start + BlockKindHeader::super_block_size();
    let data_sb_start = title_sb_start + BlockKindTitle::super_block_size();

    let main_header = BlockKindMain {
      free_header_ptr: header_sb_start + BlockKindHeader::header_size(),
      free_title_ptr: title_sb_start + BlockKindTitle::header_size(),
      free_data_ptr: data_sb_start + BlockKindData::header_size(),
      first_header_ptr: 0,
    };

    let main_header_bytes = bincode::serialize(&main_header)?;
    self.inner.seek(SeekFrom::Start(0))?;
    self.inner.write_all(&main_header_bytes)?;

    Ok(())
  }

  fn init_superblocks(&mut self) -> Result<(), InitializationError> {
    let header_sb_start = BlockKindMain::super_block_size();
    let title_sb_start = header_sb_start + BlockKindHeader::super_block_size();
    let data_sb_start = title_sb_start + BlockKindTitle::super_block_size();

    // Initialize Header Superblock
    self.inner.seek(SeekFrom::Start(header_sb_start))?;
    self.inner.write_all(&BlockKindHeader::initial_header())?;

    let mut prev_block = 0;
    for i in 0..BlockKindHeader::block_count() {
      let cursor = self.inner.stream_position()?;
      let next_block = if i < BlockKindHeader::block_count() - 1 {
        cursor + BlockKindHeader::block_size()
      } else {
        0
      };
      let header_block = Block::<FileHeader> {
        prev_block,
        next_block,
        data: FileHeader {
          start_title_block: 0,
          start_data_block: 0,
        },
      };

      prev_block = cursor;

      let header_block_bytes = bincode::serialize(&header_block)?;
      self.inner.write_all(&header_block_bytes)?;
    }

    // Initialize Title Superblock
    self.inner.seek(SeekFrom::Start(title_sb_start))?;
    self.inner.write_all(&BlockKindTitle::initial_header())?;

    let mut prev_block = 0;
    for i in 0..BlockKindTitle::block_count() {
      let cursor = self.inner.stream_position()?;
      let next_block = if i < BlockKindTitle::block_count() - 1 {
        cursor + BlockKindTitle::block_size()
      } else {
        0
      };
      let title_block = Block::<FileTitle> {
        prev_block,
        next_block,
        data: FileTitle {
          data: [0; 16].to_vec(),
        },
      };

      prev_block = cursor;

      let title_block_bytes = bincode::serialize(&title_block)?;
      self.inner.write_all(&title_block_bytes)?;
    }

    // Initialize Data Superblock
    self.inner.seek(SeekFrom::Start(data_sb_start))?;
    self.inner.write_all(&BlockKindData::initial_header())?;

    let mut prev_block = 0;
    for i in 0..BlockKindData::block_count() {
      let cursor = self.inner.stream_position()?;
      let next_block = if i < BlockKindData::block_count() - 1 {
        cursor + BlockKindData::block_size()
      } else {
        0
      };
      let data_block = Block::<FileData> {
        prev_block,
        next_block,
        data: FileData {
          data: [0; 112].to_vec(),
        },
      };

      prev_block = cursor;

      let data_block_bytes = bincode::serialize(&data_block)?;
      self.inner.write_all(&data_block_bytes)?;
    }

    Ok(())
  }

  pub fn init(&mut self, size: u64) -> Result<(), FilesystemError> {
    self.clear_and_check_size(size)?;
    self.init_main_header()?;
    self.init_superblocks()?;

    Ok(())
  }

  fn write_main_header(
    &mut self,
    main_header: BlockKindMain,
  ) -> Result<(), GenericError> {
    self.inner.seek(SeekFrom::Start(0))?;
    let main_header_bytes = bincode::serialize(&main_header)?;
    self.inner.write_all(&main_header_bytes)?;

    Ok(())
  }

  fn write_header_block(
    &mut self,
    index: u64,
    header_block: Block<FileHeader>,
  ) -> Result<(), GenericError> {
    self.inner.seek(SeekFrom::Start(index))?;
    let header_block_bytes = bincode::serialize(&header_block)?;
    self.inner.write_all(&header_block_bytes)?;

    Ok(())
  }

  fn write_title_block(
    &mut self,
    index: u64,
    title_block: Block<FileTitle>,
  ) -> Result<(), GenericError> {
    self.inner.seek(SeekFrom::Start(index))?;
    let title_block_bytes = bincode::serialize(&title_block)?;
    self.inner.write_all(&title_block_bytes)?;

    Ok(())
  }

  fn write_data_block(
    &mut self,
    index: u64,
    data_block: Block<FileData>,
  ) -> Result<(), GenericError> {
    self.inner.seek(SeekFrom::Start(index))?;
    let data_block_bytes = bincode::serialize(&data_block)?;
    self.inner.write_all(&data_block_bytes)?;

    Ok(())
  }

  fn read_main_header(&mut self) -> Result<BlockKindMain, GenericError> {
    self.inner.seek(SeekFrom::Start(0))?;
    let main_header =
      bincode::deserialize_from::<_, BlockKindMain>(&mut self.inner)?;
    Ok(main_header)
  }

  fn read_header_block(
    &mut self,
    index: u64,
  ) -> Result<Option<Block<FileHeader>>, GenericError> {
    if index == 0 {
      return Ok(None);
    }
    self.inner.seek(SeekFrom::Start(index))?;
    let header_block =
      bincode::deserialize_from::<_, Block<FileHeader>>(&mut self.inner)?;
    Ok(Some(header_block))
  }

  fn read_title_block(
    &mut self,
    index: u64,
  ) -> Result<Option<Block<FileTitle>>, GenericError> {
    if index == 0 {
      return Ok(None);
    }
    self.inner.seek(SeekFrom::Start(index))?;
    let mut title_block_bytes = vec![0; BlockKindTitle::SIZE as usize];
    self.inner.read_exact(&mut title_block_bytes)?;
    let title_block = bincode::deserialize_from(&title_block_bytes[..])?;
    Ok(Some(title_block))
  }

  fn read_data_block(
    &mut self,
    index: u64,
  ) -> Result<Option<Block<FileData>>, GenericError> {
    if index == 0 {
      return Ok(None);
    }
    self.inner.seek(SeekFrom::Start(index))?;
    let mut data_block_bytes = vec![0; BlockKindData::SIZE as usize];
    self.inner.read_exact(&mut data_block_bytes)?;
    let data_block = bincode::deserialize_from(&data_block_bytes[..])?;
    Ok(Some(data_block))
  }

  pub fn insert<D>(&mut self, name: String, data: D) -> Result<(), GenericError>
  where
    D: AsRef<[u8]>,
  {
    let mut main_header = self.read_main_header()?;

    let free_file_header = self
      .read_header_block(main_header.free_header_ptr)?
      .unwrap_or_else(|| todo!("no header block"));

    let prev_file_header =
      self.read_header_block(free_file_header.prev_block)?;
    let next_file_header =
      self.read_header_block(free_file_header.next_block)?;

    let header_block = Block {
      prev_block: free_file_header.prev_block,
      next_block: main_header.first_header_ptr,
      data: FileHeader {
        start_title_block: main_header.free_title_ptr,
        start_data_block: main_header.free_data_ptr,
      },
    };

    let free_title_block = self
      .read_title_block(main_header.free_title_ptr)?
      .unwrap_or_else(|| todo!("no title block"));
    let free_data_block = self
      .read_data_block(main_header.free_data_ptr)?
      .unwrap_or_else(|| todo!("no data block"));

    let title_bytes = name.as_bytes();
    if title_bytes.len() > 16 {
      todo!("cannot store files with names greater than 16 bytes");
    }

    let title_block = Block {
      prev_block: 0,
      next_block: 0,
      data: FileTitle {
        data: title_bytes.to_vec(),
      },
    };

    let mut data_bytes: Vec<u8> = Vec::new();
    for byte in data.as_ref().bytes() {
      data_bytes.push(byte?);
    }

    if data_bytes.len() > 112 {
      todo!("cannot store files with data greater than 112 bytes");
    }

    let data_block = Block {
      prev_block: 0,
      next_block: 0,
      data: FileData { data: data_bytes },
    };

    // Write Ops
    self.write_header_block(main_header.free_header_ptr, header_block)?;
    self.write_title_block(main_header.free_title_ptr, title_block)?;
    self.write_data_block(main_header.free_data_ptr, data_block)?;

    // Main Header
    main_header.first_header_ptr = main_header.free_header_ptr;
    main_header.free_title_ptr = free_title_block.next_block;
    self.write_main_header(main_header)?;

    Ok(())
  }
}
