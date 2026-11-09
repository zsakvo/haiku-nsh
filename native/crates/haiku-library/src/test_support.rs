//! 从原 downloader 测试迁入的共用音频夹具。

/// 最小合法 FLAC：`fLaC` 魔数 + 单个 STREAMINFO 元数据块（无音频帧，够 lofty 读写标签）。
pub fn minimal_flac() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"fLaC");
    out.push(0x80); // last-metadata-block + type 0 (STREAMINFO)
    out.extend_from_slice(&[0, 0, 34]); // 块长 34
    let mut si = Vec::new();
    si.extend_from_slice(&4096u16.to_be_bytes()); // min blocksize
    si.extend_from_slice(&4096u16.to_be_bytes()); // max blocksize
    si.extend_from_slice(&[0, 0, 0]); // min framesize
    si.extend_from_slice(&[0, 0, 0]); // max framesize
                                      // 20b sample_rate | 3b channels-1 | 5b bps-1 | 36b total_samples
    let packed: u64 = (44100u64 << 44) | (1u64 << 41) | (15u64 << 36) | 44100u64;
    si.extend_from_slice(&packed.to_be_bytes());
    si.extend_from_slice(&[0u8; 16]); // md5
    assert_eq!(si.len(), 34);
    out.extend_from_slice(&si);
    out
}

/// 最小合法 JPEG（lofty 只看前 8 字节判 mime）。
pub fn tiny_jpeg() -> Vec<u8> {
    let mut v = vec![
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01,
    ];
    v.extend_from_slice(&[0u8; 16]);
    v.extend_from_slice(&[0xFF, 0xD9]);
    v
}
