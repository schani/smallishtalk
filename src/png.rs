//! A zero-dependency PNG writer (UI.md §4.4): 8-bit truecolor, "compressed"
//! with stored (uncompressed) DEFLATE blocks inside a minimal zlib stream.
//!
//! This exists so `primSaveForm:toFile:` (333) can emit screenshots a human or
//! an agent can view directly, without pulling an image crate. Small and slow
//! (no real compression), which is exactly right for occasional 1-bit shots.

/// CRC-32 (IEEE, as PNG chunks use). Public so tests can verify chunk CRCs.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Adler-32 (as zlib streams use).
fn adler32(bytes: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in bytes {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// Wrap `raw` in a zlib stream built from stored DEFLATE blocks (BTYPE=00).
fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01]; // CMF/FLG: 32K window, no dict, fastest
    if raw.is_empty() {
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    } else {
        let mut i = 0;
        while i < raw.len() {
            let len = (raw.len() - i).min(0xFFFF);
            let final_block = i + len >= raw.len();
            out.push(if final_block { 1 } else { 0 });
            out.extend_from_slice(&(len as u16).to_le_bytes());
            out.extend_from_slice(&(!(len as u16)).to_le_bytes());
            out.extend_from_slice(&raw[i..i + len]);
            i += len;
        }
    }
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

fn chunk(out: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(name);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(name);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// Encode `rgb` (row-major, `width*height*3` bytes) as a PNG byte vector.
pub fn encode_rgb(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    assert_eq!(rgb.len(), (width * height * 3) as usize, "rgb size mismatch");

    // Raw = each scanline prefixed with a 0 (None) filter byte.
    let stride = (width * 3) as usize;
    let mut raw = Vec::with_capacity((height as usize) * (1 + stride));
    for y in 0..height as usize {
        raw.push(0);
        raw.extend_from_slice(&rgb[y * stride..(y + 1) * stride]);
    }

    let mut out = vec![137, 80, 78, 71, 13, 10, 26, 10]; // PNG signature
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // depth 8, truecolor, deflate, no filter, no interlace
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}
