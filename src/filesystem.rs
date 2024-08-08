use std::io::{self, Read, Seek, SeekFrom, Write};

use thiserror::Error;

pub fn pad_with_byte_size(vec: Vec<u8>, size: u64) -> Vec<u8> {
  [size.to_le_bytes().to_vec(), vec].concat()
}

pub trait FromBytes
where
  Self: Sized,
{
  fn from_bytes<R>(reader: &mut R) -> Result<Self, io::Error>
  where
    R: Read;
}

pub trait ToBytes {
  fn to_bytes<W>(&self, writer: &mut W) -> Result<(), io::Error>
  where
    W: Write;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

impl ToBytes for BlockKindMain {
  fn to_bytes<T>(&self, writer: &mut T) -> Result<(), std::io::Error>
  where
    T: Write,
  {
    writer.write_all(&self.free_header_ptr.to_le_bytes())?;
    writer.write_all(&self.free_title_ptr.to_le_bytes())?;
    writer.write_all(&self.free_data_ptr.to_le_bytes())?;
    writer.write_all(&self.first_header_ptr.to_le_bytes())?;

    Ok(())
  }
}

impl FromBytes for BlockKindMain {
  fn from_bytes<T>(reader: &mut T) -> Result<Self, io::Error>
  where
    T: Read,
  {
    let mut free_header_ptr = [0; 8];
    let mut free_title_ptr = [0; 8];
    let mut free_data_ptr = [0; 8];
    let mut first_header_ptr = [0; 8];

    reader.read_exact(&mut free_header_ptr)?;
    reader.read_exact(&mut free_title_ptr)?;
    reader.read_exact(&mut free_data_ptr)?;
    reader.read_exact(&mut first_header_ptr)?;

    Ok(Self {
      free_header_ptr: u64::from_le_bytes(free_header_ptr),
      free_title_ptr: u64::from_le_bytes(free_title_ptr),
      free_data_ptr: u64::from_le_bytes(free_data_ptr),
      first_header_ptr: u64::from_le_bytes(first_header_ptr),
    })
  }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SuperBlock<T>
where
  T: BlockAlign + ToBytes + FromBytes,
{
  blocks: Block<T>,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Block<T>
where
  T: ?Sized + ToBytes + FromBytes,
{
  prev_block: u64,
  next_block: u64,
  data: T,
}

impl<T> ToBytes for Block<T>
where
  T: ?Sized + ToBytes + FromBytes,
{
  fn to_bytes<W>(&self, writer: &mut W) -> Result<(), io::Error>
  where
    W: Write,
  {
    writer.write_all(&self.prev_block.to_le_bytes())?;
    writer.write_all(&self.next_block.to_le_bytes())?;
    self.data.to_bytes(writer)?;
    Ok(())
  }
}

impl<T> FromBytes for Block<T>
where
  T: ?Sized + ToBytes + FromBytes,
{
  fn from_bytes<R>(reader: &mut R) -> Result<Self, io::Error>
  where
    R: Read,
  {
    let mut prev_block = [0; 8];
    let mut next_block = [0; 8];

    reader.read_exact(&mut prev_block)?;
    reader.read_exact(&mut next_block)?;

    let data = T::from_bytes(reader)?;

    Ok(Self {
      prev_block: u64::from_le_bytes(prev_block),
      next_block: u64::from_le_bytes(next_block),
      data,
    })
  }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FileHeader {
  start_title_block: u64,
  start_data_block: u64,
}

impl ToBytes for FileHeader {
  fn to_bytes<T>(&self, writer: &mut T) -> Result<(), io::Error>
  where
    T: Write,
  {
    writer.write_all(&self.start_title_block.to_le_bytes())?;
    writer.write_all(&self.start_data_block.to_le_bytes())?;
    Ok(())
  }
}

impl FromBytes for FileHeader {
  fn from_bytes<T>(reader: &mut T) -> Result<Self, io::Error>
  where
    T: Read,
  {
    let mut start_title_block = [0; 8];
    let mut start_data_block = [0; 8];

    reader.read_exact(&mut start_title_block)?;
    reader.read_exact(&mut start_data_block)?;

    Ok(Self {
      start_title_block: u64::from_le_bytes(start_title_block),
      start_data_block: u64::from_le_bytes(start_data_block),
    })
  }
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileTitle {
  data: [u8; 16],
}

impl ToBytes for FileTitle {
  fn to_bytes<T>(&self, writer: &mut T) -> Result<(), io::Error>
  where
    T: Write,
  {
    writer.write_all(&self.data)?;
    Ok(())
  }
}

impl FromBytes for FileTitle {
  fn from_bytes<T>(reader: &mut T) -> Result<Self, io::Error>
  where
    T: Read,
  {
    let mut data = [0; 16];
    reader.read_exact(&mut data)?;
    Ok(Self { data })
  }
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileData {
  pub data: [u8; 112],
}

impl ToBytes for FileData {
  fn to_bytes<T>(&self, writer: &mut T) -> Result<(), io::Error>
  where
    T: Write,
  {
    writer.write_all(&self.data)?;
    Ok(())
  }
}

impl FromBytes for FileData {
  fn from_bytes<T>(reader: &mut T) -> Result<Self, io::Error>
  where
    T: Read,
  {
    let mut data = [0; 112];
    reader.read_exact(&mut data)?;
    Ok(Self { data })
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
}

#[derive(Debug, Error)]
pub enum GenericError {
  #[error(transparent)]
  IO(#[from] io::Error),
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
    self.inner.seek(SeekFrom::Start(0)).unwrap();
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

    self.inner.write_all(&buf).unwrap();

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

    self.inner.seek(SeekFrom::Start(0)).unwrap();
    main_header.to_bytes(&mut self.inner).unwrap();

    Ok(())
  }

  fn init_superblocks(&mut self) -> Result<(), InitializationError> {
    let header_sb_start = BlockKindMain::super_block_size();
    let title_sb_start = header_sb_start + BlockKindHeader::super_block_size();
    let data_sb_start = title_sb_start + BlockKindTitle::super_block_size();

    // Initialize Header Superblock
    self.inner.seek(SeekFrom::Start(header_sb_start)).unwrap();
    self
      .inner
      .write_all(&BlockKindHeader::initial_header())
      .unwrap();

    let mut prev_block = 0;
    for i in 0..BlockKindHeader::block_count() {
      let cursor = self.inner.stream_position().unwrap();
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

      header_block.to_bytes(&mut self.inner)?;
    }

    // Initialize Title Superblock
    self.inner.seek(SeekFrom::Start(title_sb_start)).unwrap();
    self
      .inner
      .write_all(&BlockKindTitle::initial_header())
      .unwrap();

    let mut prev_block = 0;
    for i in 0..BlockKindTitle::block_count() {
      let cursor = self.inner.stream_position().unwrap();
      let next_block = if i < BlockKindTitle::block_count() - 1 {
        cursor + BlockKindTitle::block_size()
      } else {
        0
      };
      let title_block = Block::<FileTitle> {
        prev_block,
        next_block,
        data: FileTitle { data: [0; 16] },
      };

      prev_block = cursor;

      title_block.to_bytes(&mut self.inner);
    }

    // Initialize Data Superblock
    self.inner.seek(SeekFrom::Start(data_sb_start)).unwrap();
    self
      .inner
      .write_all(&BlockKindData::initial_header())
      .unwrap();

    let mut prev_block = 0;
    for i in 0..BlockKindData::block_count() {
      let cursor = self.inner.stream_position().unwrap();
      let next_block = if i < BlockKindData::block_count() - 1 {
        cursor + BlockKindData::block_size()
      } else {
        0
      };
      let data_block = Block::<FileData> {
        prev_block,
        next_block,
        data: FileData { data: [0; 112] },
      };

      prev_block = cursor;

      data_block.to_bytes(&mut self.inner);
    }

    Ok(())
  }

  pub fn init(&mut self, size: u64) -> Result<(), FilesystemError> {
    self.clear_and_check_size(size).unwrap();
    self.init_main_header().unwrap();
    self.init_superblocks().unwrap();

    Ok(())
  }

  fn write_main_header(
    &mut self,
    main_header: BlockKindMain,
  ) -> Result<(), GenericError> {
    self.inner.seek(SeekFrom::Start(0)).unwrap();
    main_header.to_bytes(&mut self.inner)?;

    Ok(())
  }

  fn write_header_block(
    &mut self,
    index: u64,
    header_block: Block<FileHeader>,
  ) -> Result<(), GenericError> {
    self.inner.seek(SeekFrom::Start(index)).unwrap();
    header_block.to_bytes(&mut self.inner);

    Ok(())
  }

  fn write_title_block(
    &mut self,
    index: u64,
    title_block: Block<FileTitle>,
  ) -> Result<(), GenericError> {
    self.inner.seek(SeekFrom::Start(index)).unwrap();
    title_block.to_bytes(&mut self.inner);

    Ok(())
  }

  fn write_data_block(
    &mut self,
    index: u64,
    data_block: Block<FileData>,
  ) -> Result<(), GenericError> {
    self.inner.seek(SeekFrom::Start(index)).unwrap();
    data_block.to_bytes(&mut self.inner)?;

    Ok(())
  }

  fn read_main_header(&mut self) -> Result<BlockKindMain, GenericError> {
    self.inner.seek(SeekFrom::Start(0)).unwrap();
    let main_header: BlockKindMain =
      BlockKindMain::from_bytes(&mut self.inner)?;
    Ok(main_header)
  }

  fn read_header_block(
    &mut self,
    index: u64,
  ) -> Result<Option<Block<FileHeader>>, GenericError> {
    if index == 0 {
      return Ok(None);
    }
    self.inner.seek(SeekFrom::Start(index)).unwrap();
    let header_block: Block<FileHeader> = Block::from_bytes(&mut self.inner)?;
    Ok(Some(header_block))
  }

  fn read_title_block(
    &mut self,
    index: u64,
  ) -> Result<Option<Block<FileTitle>>, GenericError> {
    if index == 0 {
      return Ok(None);
    }
    self.inner.seek(SeekFrom::Start(index)).unwrap();
    let title_block: Block<FileTitle> = Block::from_bytes(&mut self.inner)?;
    Ok(Some(title_block))
  }

  fn read_data_block(
    &mut self,
    index: u64,
  ) -> Result<Option<Block<FileData>>, GenericError> {
    if index == 0 {
      return Ok(None);
    }
    self.inner.seek(SeekFrom::Start(index)).unwrap();
    let data_block: Block<FileData> = Block::from_bytes(&mut self.inner)?;
    Ok(Some(data_block))
  }

  pub fn insert<D>(&mut self, name: String, data: D) -> Result<(), GenericError>
  where
    D: AsRef<[u8]>,
  {
    let mut main_header = self.read_main_header().unwrap();

    let free_file_header = self
      .read_header_block(main_header.free_header_ptr)?
      .unwrap_or_else(|| todo!("no header block"));

    let prev_file_header =
      self.read_header_block(free_file_header.prev_block).unwrap();
    let next_file_header =
      self.read_header_block(free_file_header.next_block).unwrap();

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

    let mut title_bytes: [u8; 16] = [0; 16];
    if title_bytes.len() > 16 {
      todo!("cannot store files with names greater than 16 bytes");
    }
    for (i, byte) in name.as_bytes().iter().enumerate().take(112) {
      title_bytes[i] = *byte;
    }

    let title_block = Block {
      prev_block: 0,
      next_block: 0,
      data: FileTitle { data: title_bytes },
    };

    let mut data_bytes: [u8; 112] = [0; 112];
    if data_bytes.len() > 112 {
      todo!("cannot store files with data greater than 112 bytes");
    }
    for (i, byte) in data.as_ref().bytes().enumerate().take(112) {
      data_bytes[i] = byte?;
    }

    let data_block = Block {
      prev_block: 0,
      next_block: 0,
      data: FileData { data: data_bytes },
    };

    // Write Ops
    self
      .write_header_block(main_header.free_header_ptr, header_block)
      .unwrap();
    self
      .write_title_block(main_header.free_title_ptr, title_block)
      .unwrap();
    self
      .write_data_block(main_header.free_data_ptr, data_block)
      .unwrap();

    // Main Header
    main_header.first_header_ptr = main_header.free_header_ptr;
    main_header.free_title_ptr = free_title_block.next_block;
    self.write_main_header(main_header).unwrap();

    Ok(())
  }
}
