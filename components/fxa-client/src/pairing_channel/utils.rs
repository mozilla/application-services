use anyhow::Result;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::convert::TryInto;

pub(crate) const VERSION_TLS_1_0: u16 = 0x0301;
pub(crate) const VERSION_TLS_1_2: u16 = 0x0303;
pub(crate) const VERSION_TLS_1_3: u16 = 0x0304;
pub(crate) const TLS_AES_128_GCM_SHA256: u16 = 0x1301;
pub(crate) const HASH_LENGTH: usize = 32;
pub(crate) const SUPPORTED_VERSIONS_ID: u16 = 43;
pub(crate) const PRE_SHARED_KEY_ID: u16 = 41;
pub(crate) const PSK_MODE_KE: u8 = 0;
pub(crate) const PSK_KEY_ID: u16 = 45;

pub(crate) fn write_u24(buf: &mut BytesMut, num: u32) {
    buf.put_u16((num >> 8) as u16);
    buf.put_u8((num & 0xFF) as u8);
}

pub(crate) fn read_u24(buf: &mut Bytes) -> Result<u32> {
    let first_16 = read_u16(buf)?;
    let last_8 = read_u8(buf)?;
    Ok((first_16 << 8) as u32 | last_8 as u32)
}

pub(crate) fn read_u16(buf: &mut Bytes) -> Result<u16> {
    if buf.remaining() < 2 {
        anyhow::bail!("Buffer too small to read u16");
    }
    Ok(buf.get_u16())
}

pub(crate) fn read_u8(buf: &mut Bytes) -> Result<u8> {
    if !buf.has_remaining() {
        anyhow::bail!("Buffer too small to read u8")
    }
    Ok(buf.get_u8())
}

pub(crate) fn read_u32(buf: &mut Bytes) -> Result<u32> {
    if buf.remaining() < 4 {
        anyhow::bail!("Buffer too small to read u32")
    }
    Ok(buf.get_u32())
}

pub(crate) fn read_bytes(buf: &mut Bytes, len: usize) -> Result<Vec<u8>> {
    if buf.remaining() < len {
        anyhow::bail!("Buffer too small to read {} bytes", len)
    }
    let res = (&buf.bytes()[..len]).to_vec();
    buf.advance(len);
    Ok(res)
}

pub(crate) fn read_bytes_with_u8_len(buf: &mut Bytes) -> Result<Vec<u8>> {
    let len = read_u8(buf)?;
    read_bytes(buf, len as usize)
}

pub(crate) fn read_bytes_with_u16_len(buf: &mut Bytes) -> Result<Vec<u8>> {
    let len = read_u16(buf)?;
    read_bytes(buf, len as usize)
}

pub(crate) fn read_bytes_with_u24_len(buf: &mut Bytes) -> Result<Vec<u8>> {
    let len = read_u24(buf)?;
    read_bytes(buf, len as usize)
}

pub(crate) fn write_bytes_with_u8_len(buf: &mut BytesMut, bytes: &[u8]) -> Result<()> {
    let len: u8 = bytes.len().try_into()?;
    buf.put_u8(len);
    buf.put(bytes);
    Ok(())
}

pub(crate) fn write_bytes_with_u16_len(buf: &mut BytesMut, bytes: &[u8]) -> Result<()> {
    let len: u16 = bytes.len().try_into()?;
    buf.put_u16(len);
    buf.put(bytes);
    Ok(())
}

pub(crate) fn write_with_callback_u16_len<F>(buf: &mut BytesMut, callback: &mut F) -> Result<()>
where
    F: FnMut(&mut BytesMut) -> Result<()>,
{
    let mut other_buf = BytesMut::new();
    (callback)(&mut other_buf)?;
    buf.put_u16(other_buf.len().try_into()?);
    buf.put(other_buf);
    Ok(())
}

pub(crate) fn write_with_callback_u8_len<F>(buf: &mut BytesMut, callback: &mut F) -> Result<()>
where
    F: FnMut(&mut BytesMut) -> Result<()>,
{
    let mut other_buf = BytesMut::new();
    (callback)(&mut other_buf)?;
    buf.put_u8(other_buf.len().try_into()?);
    buf.put(other_buf);
    Ok(())
}

pub(crate) fn read_callback_till_done<F>(buf: &mut Bytes, callback: &mut F) -> Result<()>
where
    F: FnMut(&mut Bytes) -> Result<()>,
{
    while buf.has_remaining() {
        // TODO: Add check to ensure that the buf has advanced.
        (callback)(buf)?;
    }
    Ok(())
}
