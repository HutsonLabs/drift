use std::borrow::Cow;

use ironrdp_core::{
    Decode, DecodeResult, Encode, EncodeResult, IntoOwned, ReadCursor, WriteCursor, cast_int, ensure_size,
    invalid_field_err,
};
use ironrdp_pdu::impl_pdu_borrowing;
use ironrdp_pdu::utils::{CharacterSet, read_string_from_cursor, write_string_to_cursor};

use crate::pdu::PartialHeader;

/// Represents `CLIPRDR_TEMP_DIRECTORY`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientTemporaryDirectory<'a> {
    path_buffer: Cow<'a, [u8]>,
}

impl_pdu_borrowing!(ClientTemporaryDirectory<'_>, OwnedClientTemporaryDirectory);

impl IntoOwned for ClientTemporaryDirectory<'_> {
    type Owned = OwnedClientTemporaryDirectory;

    fn into_owned(self) -> Self::Owned {
        OwnedClientTemporaryDirectory {
            path_buffer: Cow::Owned(self.path_buffer.into_owned()),
        }
    }
}

impl ClientTemporaryDirectory<'_> {
    const PATH_BUFFER_SIZE: usize = 520;

    const NAME: &'static str = "CLIPRDR_TEMP_DIRECTORY";
    const INNER_SIZE: usize = Self::PATH_BUFFER_SIZE;

    /// Creates new `ClientTemporaryDirectory` and encodes given path to UTF-16 representation.
    pub fn new(path: &str) -> EncodeResult<Self> {
        let mut buffer = vec![0x00; Self::PATH_BUFFER_SIZE];

        {
            let mut cursor = WriteCursor::new(&mut buffer);
            write_string_to_cursor(&mut cursor, path, CharacterSet::Unicode, true)?;
        }

        Ok(Self {
            path_buffer: Cow::Owned(buffer),
        })
    }

    /// Returns parsed temporary directory path.
    pub fn temporary_directory_path(&self) -> DecodeResult<String> {
        let mut cursor = ReadCursor::new(&self.path_buffer);

        read_string_from_cursor(&mut cursor, CharacterSet::Unicode, true)
            .map_err(|_| invalid_field_err!("wszTempDir", "failed to decode temp dir path"))
    }
}

impl Encode for ClientTemporaryDirectory<'_> {
    fn encode(&self, dst: &mut WriteCursor<'_>) -> EncodeResult<()> {
        let header = PartialHeader::new(cast_int!("dataLen", Self::INNER_SIZE, in: dst)?);
        header.encode(dst)?;

        ensure_size!(in: dst, size: Self::INNER_SIZE);
        dst.write_slice(&self.path_buffer);

        Ok(())
    }

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn size(&self) -> usize {
        PartialHeader::SIZE + Self::INNER_SIZE
    }
}

impl<'de> Decode<'de> for ClientTemporaryDirectory<'de> {
    fn decode(src: &mut ReadCursor<'de>) -> DecodeResult<Self> {
        let header = PartialHeader::decode(src)?;
        if header.data_length() != Self::INNER_SIZE {
            return Err(invalid_field_err!("dataLen", "CLIPRDR_TEMP_DIRECTORY dataLen must be 520", in: src));
        }

        ensure_size!(in: src, size: Self::INNER_SIZE);
        let buffer = src.read_slice(Self::PATH_BUFFER_SIZE);

        // wszTempDir is a null-terminated string that must fit in the fixed buffer.
        if !buffer.chunks_exact(2).any(|unit| unit == [0, 0]) {
            return Err(invalid_field_err!("wszTempDir", "path is not null-terminated", in: src));
        }

        Ok(Self {
            path_buffer: Cow::Borrowed(buffer),
        })
    }
}
