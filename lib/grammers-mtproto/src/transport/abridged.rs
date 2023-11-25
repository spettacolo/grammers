// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
use super::{Error, Transport};
use bytes::{BufMut, BytesMut};
use grammers_crypto::RingBuffer;

/// The lightest MTProto transport protocol available. This is an
/// implementation of the [abridged transport].
///
/// * Overhead: very small.
/// * Minimum envelope length: 1 byte.
/// * Maximum envelope length: 4 bytes.
///
/// It serializes the input payload as follows, if the length is small enough:
///
/// ```text
/// +-+----...----+
/// |L|  payload  |
/// +-+----...----+
///  ^ 1 byte
/// ```
///
/// Otherwise:
///
/// ```text
/// +----+----...----+
/// | len|  payload  |
/// +----+----...----+
///  ^^^^ 4 bytes
/// ```
///
/// [abridged transport]: https://core.telegram.org/mtproto/mtproto-transports#abridged
pub struct Abridged {
    init: bool,
}

#[allow(clippy::new_without_default)]
impl Abridged {
    pub fn new() -> Self {
        Self { init: false }
    }
}

impl Transport for Abridged {
    fn pack(&mut self, input: &[u8], output: &mut RingBuffer<u8>) {
        assert_eq!(input.len() % 4, 0);

        if !self.init {
            output.push(0xef);
            self.init = true;
        }

        let len = input.len() / 4;
        if len < 127 {
            output.push(len as u8);
            output.extend(input);
        } else {
            output.push(0x7f);
            output.extend(&(len as u32).to_le_bytes()[..3]);
            output.extend(input);
        }
    }

    fn unpack(&mut self, input: &[u8], output: &mut BytesMut) -> Result<usize, Error> {
        if input.is_empty() {
            return Err(Error::MissingBytes);
        }

        let header_len;
        let len = input[0];
        let len = if len < 127 {
            header_len = 1;
            len as i32
        } else {
            if input.len() < 4 {
                return Err(Error::MissingBytes);
            }

            header_len = 4;
            let mut len = [0; 4];
            len[..3].copy_from_slice(&input[1..4]);
            i32::from_le_bytes(len)
        };

        let len = len * 4;
        if (input.len() as i32) < header_len + len {
            return Err(Error::MissingBytes);
        }

        let header_len = header_len as usize;
        let len = len as usize;

        output.put(&input[header_len..header_len + len]);
        Ok(header_len + len)
    }

    fn reset(&mut self) {
        self.init = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns a new abridged transport, `n` bytes of input data for it, and an empty output buffer.
    fn setup_pack(n: u32) -> (Abridged, Vec<u8>, RingBuffer<u8>) {
        let input = (0..n).map(|x| (x & 0xff) as u8).collect();
        (Abridged::new(), input, RingBuffer::with_capacity(0, 0))
    }

    #[test]
    fn pack_empty() {
        let (mut transport, input, mut output) = setup_pack(0);
        transport.pack(&input, &mut output);
        assert_eq!(&output[..], &[0xef, 0]);
    }

    #[test]
    #[should_panic]
    fn pack_non_padded() {
        let (mut transport, input, mut output) = setup_pack(7);
        transport.pack(&input, &mut output);
    }

    #[test]
    fn pack_normal() {
        let (mut transport, input, mut output) = setup_pack(128);
        transport.pack(&input, &mut output);
        assert_eq!(&output[..2], &[0xef, 32]);
        assert_eq!(&output[2..output.len()], &input[..]);
    }

    #[test]
    fn pack_large() {
        let (mut transport, input, mut output) = setup_pack(1024);
        transport.pack(&input, &mut output);
        assert_eq!(&output[..5], &[0xef, 127, 0, 1, 0]);
        assert_eq!(&output[5..], &input[..]);
    }

    #[test]
    fn unpack_small() {
        let mut transport = Abridged::new();
        let input = [1];
        let mut output = BytesMut::new();
        assert_eq!(
            transport.unpack(&input, &mut output),
            Err(Error::MissingBytes)
        );
    }

    #[test]
    fn unpack_normal() {
        let (mut transport, input, mut packed) = setup_pack(128);
        let mut unpacked = BytesMut::new();
        transport.pack(&input, &mut packed);
        transport.unpack(&packed[1..], &mut unpacked).unwrap();
        assert_eq!(input, unpacked);
    }

    #[test]
    fn unpack_two_at_once() {
        let (mut transport, input, mut packed) = setup_pack(128);
        let mut unpacked = BytesMut::new();
        transport.pack(&input, &mut packed);
        let two_input = packed
            .as_ref()
            .iter()
            .copied()
            .skip(1)
            .chain(packed.as_ref().iter().copied().skip(1))
            .collect::<Vec<_>>();
        let n = transport.unpack(&two_input, &mut unpacked).unwrap();
        assert_eq!(input, unpacked);
        assert_eq!(n, packed.len() - 1);
    }

    #[test]
    fn unpack_large() {
        let (mut transport, input, mut packed) = setup_pack(1024);
        let mut unpacked = BytesMut::new();
        transport.pack(&input, &mut packed);
        transport.unpack(&packed[1..], &mut unpacked).unwrap();
        assert_eq!(input, unpacked);
    }
}
