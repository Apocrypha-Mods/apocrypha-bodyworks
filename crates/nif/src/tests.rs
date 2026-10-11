use super::*;

/// A small Special Edition file: two blocks, two strings.
pub(crate) fn sample(stream_version: u32) -> Vec<u8> {
    let mut w = Writer::default();
    w.bytes(b"Gamebryo File Format, Version 20.2.0.7\n");
    w.u32(VERSION_20_2_0_7);
    w.u8(1);
    w.u32(12);
    w.u32(2);
    w.u32(stream_version);
    w.short_string(b"me\0");
    if stream_version > 130 {
        w.u32(7);
    }
    if stream_version < 131 {
        w.short_string(b"\0");
    }
    w.short_string(b"\0");
    if stream_version >= 103 {
        w.short_string(b"x.nif\0");
    }
    w.u16(2);
    w.sized_string(b"NiNode");
    w.sized_string(b"BSTriShape");
    w.u16(0);
    w.u16(1);
    w.u32(3);
    w.u32(5);
    w.u32(2);
    w.u32(5);
    w.sized_string(b"Scene");
    w.sized_string(b"Body");
    w.u32(0);
    w.bytes(&[1, 2, 3]);
    w.bytes(&[4, 5, 6, 7, 8]);
    w.u32(1);
    w.i32(0);
    w.out
}

#[test]
fn a_file_reads_and_writes_back_as_it_was() {
    for stream_version in [83, 100, 130, 155] {
        let bytes = sample(stream_version);
        let nif = Nif::read(&bytes).unwrap();
        assert_eq!(nif.blocks.len(), 2);
        assert_eq!(nif.kind_of(0), "NiNode");
        assert_eq!(nif.kind_of(1), "BSTriShape");
        assert_eq!(nif.blocks[1].data, [4, 5, 6, 7, 8]);
        assert_eq!(nif.string(1), Some(b"Body".as_slice()));
        assert_eq!(nif.string(-1), None);
        assert_eq!(nif.roots, [0]);
        assert_eq!(nif.write(), bytes, "stream version {stream_version}");
    }
    assert_eq!(Nif::read(&sample(100)).unwrap().game(), Game::SkyrimSe);
    assert_eq!(Nif::read(&sample(130)).unwrap().game(), Game::Fallout4);
}

#[test]
fn a_changed_block_changes_only_its_own_size() {
    let mut nif = Nif::read(&sample(100)).unwrap();
    nif.blocks[0].data.extend([9, 9]);
    let again = Nif::read(&nif.write()).unwrap();
    assert_eq!(again.blocks[0].data, [1, 2, 3, 9, 9]);
    assert_eq!(again.blocks[1].data, [4, 5, 6, 7, 8]);
}

#[test]
fn a_new_string_and_a_new_kind_are_added_once() {
    let mut nif = Nif::read(&sample(100)).unwrap();
    assert_eq!(nif.intern(b"Body"), 1);
    assert_eq!(nif.intern(b"A longer name"), 2);
    assert_eq!(nif.intern(b"A longer name"), 2);
    assert_eq!(nif.header.max_string_len, 13);
    assert_eq!(nif.kind_index("NiNode"), 0);
    assert_eq!(nif.kind_index("NiSkinInstance"), 2);
    let again = Nif::read(&nif.write()).unwrap();
    assert_eq!(again.string(2), Some(b"A longer name".as_slice()));
    assert_eq!(again.header.block_types.len(), 3);
}

#[test]
fn a_broken_file_is_refused_not_read_past() {
    assert_eq!(Nif::read(b"hello"), Err(NifError::NotNif));
    assert_eq!(Nif::read(b"hello\nworld"), Err(NifError::NotNif));
    let bytes = sample(100);
    // Cut anywhere, it is truncated and never a panic.
    for cut in 0..bytes.len() {
        assert!(Nif::read(&bytes[..cut]).is_err(), "cut at {cut}");
    }
    // Bytes after the footer.
    let mut long = bytes.clone();
    long.push(0);
    assert!(matches!(Nif::read(&long), Err(NifError::Corrupt(_))));
    // A block of a kind the table does not have.
    let mut bad = bytes.clone();
    let at = bad.windows(10).position(|w| w == b"BSTriShape").unwrap() + 10;
    bad[at..at + 2].copy_from_slice(&9u16.to_le_bytes());
    assert!(matches!(Nif::read(&bad), Err(NifError::Corrupt(_))));
    // A count far larger than the file.
    let mut huge = bytes;
    let blocks_at = b"Gamebryo File Format, Version 20.2.0.7\n".len() + 9;
    huge[blocks_at..blocks_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(Nif::read(&huge), Err(NifError::Truncated));
    // An older container version.
    let mut old = sample(100);
    let v = b"Gamebryo File Format, Version 20.2.0.7\n".len();
    old[v..v + 4].copy_from_slice(&0x1400_0005u32.to_le_bytes());
    assert!(matches!(Nif::read(&old), Err(NifError::Unsupported(_))));
}

mod geometry {
    use crate::geometry::*;

    #[test]
    fn a_half_float_and_back_is_the_same_half() {
        for h in [0u16, 0x3c00, 0xbc00, 0x3555, 0x7bff, 0x0001, 0x03ff, 0x0400, 0x8000, 0x4248] {
            assert_eq!(f32_to_half(half_to_f32(h)), h, "{h:#06x}");
        }
        assert_eq!(half_to_f32(0x3c00), 1.0);
        assert_eq!(half_to_f32(0xc000), -2.0);
        assert_eq!(f32_to_half(0.5), 0x3800);
        // Too large for a half is the largest there is, not a wrapped one.
        assert_eq!(f32_to_half(1.0e9), 0x7c00);
        assert!(half_to_f32(0x7c00).is_infinite());
    }

    /// Bits 44 and up say what a vertex carries; the low four, its size in
    /// units of four bytes.
    fn desc(attributes: u16, quads: u64) -> u64 {
        (u64::from(attributes) << 44) | quads
    }

    #[test]
    fn a_layout_is_what_the_description_adds_up_to() {
        use attr::*;
        // A Special Edition body: position, UV, normal, tangent, skin.
        let skinned = POSITION | UV | NORMAL | TANGENT | SKINNED;
        let l = Layout::of(desc(skinned, 10), 100).unwrap();
        assert_eq!((l.stride, l.full_precision), (40, true));
        // With colours it is four bytes more.
        assert_eq!(Layout::of(desc(skinned | COLOR, 11), 100).unwrap().stride, 44);
        // Fallout 4 packs positions as halves unless told otherwise.
        let fo4 = Layout::of(desc(POSITION | UV | NORMAL | TANGENT, 5), 130).unwrap();
        assert_eq!((fo4.stride, fo4.full_precision), (20, false));
        assert!(Layout::of(desc(POSITION | UV | NORMAL | TANGENT | FULL_PRECISION, 7), 130).is_some());
        // A size that does not match what is listed is not guessed at.
        assert_eq!(Layout::of(desc(skinned, 9), 100), None);
    }

    #[test]
    fn a_vertex_is_read_and_changed_in_place() {
        use attr::*;
        let attributes = POSITION | UV | NORMAL | TANGENT | SKINNED;
        let layout = Layout::of(desc(attributes, 10), 100).unwrap();
        let mut bytes = vec![0xEEu8; 80];
        set_position_in(&mut bytes, &layout, 1, [1.5, -2.0, 3.25]);
        assert_eq!(position_in(&bytes, &layout, 1), Some([1.5, -2.0, 3.25]));
        // The other vertex, and everything else of this one, is untouched.
        assert!(bytes[..40].iter().all(|b| *b == 0xEE));
        assert!(bytes[52..].iter().all(|b| *b == 0xEE));
        assert_eq!(position_in(&bytes, &layout, 2), None);
    }
}
