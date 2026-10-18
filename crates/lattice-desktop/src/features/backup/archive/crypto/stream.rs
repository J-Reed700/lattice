//! Streaming AEAD: sealing a payload into framed chunks and reading it back.
use super::TAG_LEN;
use crate::features::backup::archive::format::{ArchiveError, STREAM_NONCE_LEN};
use aead_stream::{DecryptorBE32, EncryptorBE32};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit};
use std::io::{Read, Write};

/// STREAM-BE32 nonce prefix as the AEAD stack wants it.
type StreamNoncePrefix = chacha20poly1305::aead::array::Array<u8, chacha20poly1305::consts::U7>;

fn invalid_data(message: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}

fn truncated() -> std::io::Error {
    invalid_data("backup archive is truncated")
}

fn auth_failed() -> std::io::Error {
    invalid_data("backup archive failed authentication")
}

fn malformed_frame() -> std::io::Error {
    invalid_data("backup archive is corrupt: malformed chunk framing")
}

fn data_after_last() -> std::io::Error {
    invalid_data("backup archive is corrupt: data after the final chunk")
}

fn stream_nonce_prefix(bytes: &[u8; STREAM_NONCE_LEN]) -> StreamNoncePrefix {
    StreamNoncePrefix::from(*bytes)
}

fn write_frame<W: Write>(writer: &mut W, is_last: bool, ciphertext: &[u8]) -> std::io::Result<()> {
    let len = u32::try_from(ciphertext.len())
        .map_err(|_| std::io::Error::other("backup archive chunk is too large to frame"))?;
    writer.write_all(&[u8::from(is_last)])?;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(ciphertext)
}

/// `Write` adapter that buffers plaintext into `chunk_size` blocks and emits
/// STREAM-BE32 ciphertext chunks framed as `[is_last u8][ct_len u32 LE][ct]`.
/// `finish` must be called to flush the final (flagged) chunk.
///
/// Dropping without calling `finish` does not panic; it simply leaves the
/// stream without its flagged final chunk, which readers reject as truncated.
pub struct EncryptingWriter<W: Write> {
    inner: Option<W>,
    encryptor: Option<EncryptorBE32<ChaCha20Poly1305>>,
    aad: Vec<u8>,
    chunk_size: usize,
    pending: Vec<u8>,
}

impl<W: Write> EncryptingWriter<W> {
    pub fn new(
        inner: W,
        file_key: &[u8; 32],
        stream_nonce: &[u8; STREAM_NONCE_LEN],
        aad: &[u8],
        chunk_size: usize,
    ) -> Result<Self, ArchiveError> {
        if chunk_size == 0 {
            return Err(ArchiveError::Other(
                "chunk size must be positive".to_string(),
            ));
        }
        let cipher = ChaCha20Poly1305::new(&chacha20poly1305::Key::from(*file_key));
        let encryptor = EncryptorBE32::from_aead(cipher, &stream_nonce_prefix(stream_nonce));
        Ok(Self {
            inner: Some(inner),
            encryptor: Some(encryptor),
            aad: aad.to_vec(),
            chunk_size,
            pending: Vec::with_capacity(chunk_size),
        })
    }

    /// Encrypt one non-final chunk in place and frame it.
    fn seal_next(&mut self, mut buffer: Vec<u8>) -> std::io::Result<()> {
        let encryptor = self
            .encryptor
            .as_mut()
            .ok_or_else(|| std::io::Error::other("backup archive writer already finished"))?;
        encryptor
            .encrypt_next_in_place(self.aad.as_slice(), &mut buffer)
            .map_err(|_| std::io::Error::other("backup archive encryption failed"))?;
        let inner = self
            .inner
            .as_mut()
            .ok_or_else(|| std::io::Error::other("backup archive writer already finished"))?;
        write_frame(inner, false, buffer.as_slice())
    }

    /// Encrypt and write the last chunk, flush, and hand back the inner writer.
    pub fn finish(mut self) -> Result<W, ArchiveError> {
        let mut inner = self
            .inner
            .take()
            .ok_or_else(|| ArchiveError::Other("writer already finished".to_string()))?;
        let encryptor = self
            .encryptor
            .take()
            .ok_or_else(|| ArchiveError::Other("writer already finished".to_string()))?;
        let mut buffer = std::mem::take(&mut self.pending);
        encryptor
            .encrypt_last_in_place(self.aad.as_slice(), &mut buffer)
            .map_err(|_| ArchiveError::Other("backup archive encryption failed".to_string()))?;
        write_frame(&mut inner, true, buffer.as_slice())?;
        inner.flush()?;
        Ok(inner)
    }
}

impl<W: Write> Write for EncryptingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.encryptor.is_none() || self.inner.is_none() {
            return Err(std::io::Error::other(
                "backup archive writer already finished",
            ));
        }
        self.pending.extend_from_slice(buf);
        while self.pending.len() >= self.chunk_size {
            let rest = self.pending.split_off(self.chunk_size);
            let chunk = std::mem::replace(&mut self.pending, rest);
            self.seal_next(chunk)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // Buffered plaintext is not flushed: a partial chunk can only be
        // emitted as the flagged final chunk, which `finish` does.
        match self.inner.as_mut() {
            Some(inner) => inner.flush(),
            None => Ok(()),
        }
    }
}

/// `Read` adapter that reads framed STREAM chunks, authenticates each, and
/// yields plaintext. Truncation (EOF before the flagged last chunk),
/// reordering, and tampering all surface as `io::ErrorKind::InvalidData`.
pub struct DecryptingReader<R: Read> {
    inner: R,
    decryptor: Option<DecryptorBE32<ChaCha20Poly1305>>,
    aad: Vec<u8>,
    max_ciphertext_len: usize,
    plaintext: Vec<u8>,
    position: usize,
    finished: bool,
    /// Set once a chunk fails; every later `read` repeats the failure so a
    /// caller that ignores one error cannot go on to see a short stream as a
    /// complete one.
    poison: Option<&'static str>,
}

impl<R: Read> DecryptingReader<R> {
    pub fn new(
        inner: R,
        file_key: &[u8; 32],
        stream_nonce: &[u8; STREAM_NONCE_LEN],
        aad: &[u8],
        chunk_size: usize,
    ) -> Result<Self, ArchiveError> {
        if chunk_size == 0 {
            return Err(ArchiveError::Other(
                "chunk size must be positive".to_string(),
            ));
        }
        let cipher = ChaCha20Poly1305::new(&chacha20poly1305::Key::from(*file_key));
        let decryptor = DecryptorBE32::from_aead(cipher, &stream_nonce_prefix(stream_nonce));
        Ok(Self {
            inner,
            decryptor: Some(decryptor),
            aad: aad.to_vec(),
            max_ciphertext_len: chunk_size.saturating_add(TAG_LEN),
            plaintext: Vec::new(),
            position: 0,
            finished: false,
            poison: None,
        })
    }

    /// Read, authenticate, and decrypt the next framed chunk.
    fn fill_next_chunk(&mut self) -> std::io::Result<()> {
        let flag = read_optional_byte(&mut self.inner)?.ok_or_else(truncated)?;
        let is_last = match flag {
            0 => false,
            1 => true,
            _ => return Err(malformed_frame()),
        };
        let mut len_bytes = [0u8; 4];
        read_exact_or_truncated(&mut self.inner, &mut len_bytes)?;
        let ciphertext_len = u32::from_le_bytes(len_bytes) as usize;
        if ciphertext_len < TAG_LEN || ciphertext_len > self.max_ciphertext_len {
            return Err(malformed_frame());
        }
        let mut buffer = vec![0u8; ciphertext_len];
        read_exact_or_truncated(&mut self.inner, buffer.as_mut_slice())?;

        if is_last {
            let decryptor = self.decryptor.take().ok_or_else(data_after_last)?;
            decryptor
                .decrypt_last_in_place(self.aad.as_slice(), &mut buffer)
                .map_err(|_| auth_failed())?;
            self.finished = true;
        } else {
            let decryptor = self.decryptor.as_mut().ok_or_else(data_after_last)?;
            decryptor
                .decrypt_next_in_place(self.aad.as_slice(), &mut buffer)
                .map_err(|_| auth_failed())?;
        }

        if self.finished && read_optional_byte(&mut self.inner)?.is_some() {
            return Err(data_after_last());
        }

        self.plaintext = buffer;
        self.position = 0;
        Ok(())
    }
}

impl<R: Read> Read for DecryptingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if let Some(message) = self.poison {
            return Err(invalid_data(message));
        }
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            let available = self.plaintext.len().saturating_sub(self.position);
            if available > 0 {
                let take = available.min(buf.len());
                let end = self.position.saturating_add(take);
                let source = self
                    .plaintext
                    .get(self.position..end)
                    .ok_or_else(|| std::io::Error::other("backup archive reader lost its place"))?;
                let target = buf
                    .get_mut(..take)
                    .ok_or_else(|| std::io::Error::other("backup archive reader lost its place"))?;
                target.copy_from_slice(source);
                self.position = end;
                return Ok(take);
            }
            if self.finished {
                return Ok(0);
            }
            if let Err(e) = self.fill_next_chunk() {
                self.poison = Some("backup archive could not be authenticated");
                return Err(e);
            }
        }
    }
}

/// Read a single byte, distinguishing a clean EOF from an error.
fn read_optional_byte<R: Read>(reader: &mut R) -> std::io::Result<Option<u8>> {
    let mut byte = [0u8; 1];
    loop {
        match reader.read(&mut byte) {
            Ok(0) => return Ok(None),
            Ok(_) => return Ok(byte.first().copied()),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

/// `read_exact`, but a short read means the archive is truncated.
fn read_exact_or_truncated<R: Read>(reader: &mut R, buf: &mut [u8]) -> std::io::Result<()> {
    reader.read_exact(buf).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            truncated()
        } else {
            e
        }
    })
}
