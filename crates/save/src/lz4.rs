//! The LZ4 block format, decoded: what Skyrim Special Edition saves are
//! compressed with when their header's compression type is 2.
//!
//! Written from LZ4's published block format description. A block is a run
//! of sequences, each a token byte (the high four bits a literal length,
//! the low four a match length less four, each 15 meaning more length bytes
//! follow, added while they are 255), the literals, then a two-byte
//! little-endian distance back into what was already written and the
//! match. The last sequence has literals only.

use crate::SaveError;

/// Decodes one block that must come out at exactly `len` bytes.
///
/// The output never grows past `len` (it is allocated once, at that size,
/// so the caller bounds the allocation), a distance of zero or past the
/// start is refused, and every pass of the loop consumes input, so it ends.
pub fn decode_block(input: &[u8], len: usize) -> Result<Vec<u8>, SaveError> {
    let bad = |why: &str| SaveError::Corrupt(format!("LZ4: {why}"));
    let mut out: Vec<u8> = Vec::with_capacity(len);
    let mut i = 0usize;
    let more = |i: &mut usize, mut n: usize| -> Result<usize, SaveError> {
        loop {
            let b = *input.get(*i).ok_or(SaveError::Truncated)?;
            *i += 1;
            n = n
                .checked_add(b as usize)
                .ok_or_else(|| bad("a length overflows"))?;
            if b != 255 {
                return Ok(n);
            }
        }
    };
    loop {
        let token = *input.get(i).ok_or(SaveError::Truncated)?;
        i += 1;
        let mut lit = (token >> 4) as usize;
        if lit == 15 {
            lit = more(&mut i, lit)?;
        }
        let end = i.checked_add(lit).ok_or(SaveError::Truncated)?;
        let literals = input.get(i..end).ok_or(SaveError::Truncated)?;
        if out.len() + lit > len {
            return Err(bad("more bytes than the stated length"));
        }
        out.extend_from_slice(literals);
        i = end;
        if i == input.len() {
            break;
        }
        let d = input.get(i..i + 2).ok_or(SaveError::Truncated)?;
        let dist = u16::from_le_bytes([d[0], d[1]]) as usize;
        i += 2;
        if dist == 0 || dist > out.len() {
            return Err(bad("a match reaches before the start"));
        }
        let mut m = (token & 15) as usize;
        if m == 15 {
            m = more(&mut i, m)?;
        }
        let m = m + 4;
        if out.len() + m > len {
            return Err(bad("more bytes than the stated length"));
        }
        let from = out.len() - dist;
        if dist >= m {
            out.extend_from_within(from..from + m);
        } else {
            // Overlapping: the match repeats what it is writing.
            for k in 0..m {
                let b = out[from + k];
                out.push(b);
            }
        }
    }
    if out.len() != len {
        return Err(bad("fewer bytes than the stated length"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_only() {
        assert_eq!(
            decode_block(&[0x50, b'h', b'e', b'l', b'l', b'o'], 5).unwrap(),
            b"hello"
        );
    }

    #[test]
    fn overlapping_match_repeats() {
        // "ab", then a match of 6 at distance 2, then 1 literal ("!").
        let block = [0x22, b'a', b'b', 2, 0, 0x10, b'!'];
        assert_eq!(decode_block(&block, 9).unwrap(), b"abababab!");
    }

    #[test]
    fn long_lengths() {
        // 15 + 5 = 20 literals, then a match of 4 + 15 + 1 = 20 at distance 20,
        // then none.
        let mut block = vec![0xFF, 5];
        block.extend((0..20u8).collect::<Vec<_>>());
        block.extend([20, 0, 1, 0x00]);
        let out = decode_block(&block, 40).unwrap();
        assert_eq!(&out[..20], &out[20..]);
    }

    #[test]
    fn bad_blocks_are_errors() {
        // distance zero
        assert!(decode_block(&[0x10, b'a', 0, 0, 0x00], 10).is_err());
        // distance before the start
        assert!(decode_block(&[0x10, b'a', 5, 0, 0x00], 10).is_err());
        // longer than stated
        assert!(decode_block(&[0x50, 1, 2, 3, 4, 5], 4).is_err());
        assert!(decode_block(&[0x2F, 1, 2, 1, 0, 200, 0x00], 10).is_err());
        // shorter than stated
        assert!(decode_block(&[0x20, 1, 2], 3).is_err());
        // truncated
        assert!(decode_block(&[], 0).is_err());
        assert!(decode_block(&[0x50, 1], 5).is_err());
        assert!(decode_block(&[0xF0, 255, 255], 5).is_err());
        assert!(decode_block(&[0x10, 1, 1], 5).is_err());
    }

    /// A length byte of 255 means another follows; the first that is not
    /// ends it.
    #[test]
    fn length_bytes_of_255_run_on() {
        // 15 + 255 + 3 = 273 literals, then a match of 4 + 15 + 255 + 0 =
        // 274 at distance 1, then two literals.
        let mut block = vec![0xFF, 255, 3];
        block.extend(std::iter::repeat_n(b'x', 272));
        block.push(b'y');
        block.extend([1, 0, 255, 0, 0x20, b'a', b'b']);
        let out = decode_block(&block, 273 + 274 + 2).unwrap();
        assert_eq!(&out[..272], &[b'x'; 272][..]);
        assert!(out[272..547].iter().all(|&b| b == b'y'));
        assert_eq!(&out[547..], b"ab");
        // Cut in the run of length bytes.
        assert_eq!(
            decode_block(&block[..2], 273).err(),
            Some(SaveError::Truncated)
        );
    }

    #[test]
    fn an_empty_block_is_one_token() {
        assert_eq!(decode_block(&[0x00], 0).unwrap(), b"");
        assert!(decode_block(&[0x00], 1).is_err());
    }

    /// A match may reach back to the very first byte, and end exactly at the
    /// stated length.
    #[test]
    fn a_match_from_the_first_byte_to_the_last() {
        let block = [0x40, 1, 2, 3, 4, 4, 0, 0x00];
        assert_eq!(decode_block(&block, 8).unwrap(), [1, 2, 3, 4, 1, 2, 3, 4]);
        assert!(decode_block(&block, 7).is_err());
        // The literals after a match must not run past the length either.
        assert!(decode_block(&[0x40, 1, 2, 3, 4, 4, 0, 0x10, 9], 8).is_err());
    }
}
