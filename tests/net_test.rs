//! 原始 TCP 协议冒烟测试
//!
//! 需要网络连接，默认跳过。运行方式:
//!   cargo test --features integration --test net_test
//!
//! 此前无 feature 门控 + 硬编码 IP —— 裸 `cargo test` 在无网环境直接
//! panic，且 IP 过期后无从更新 (CODE_REVIEW C5)

#![cfg(feature = "integration")]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use tdxrs::protocol::constants::DEFAULT_SERVERS;

fn recv_exact(stream: &mut TcpStream, len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    let mut total = 0;
    while total < len {
        let n = stream.read(&mut buf[total..]).unwrap();
        if n == 0 {
            panic!("disconnected");
        }
        total += n;
    }
    buf
}

fn send_and_recv(stream: &mut TcpStream, packet: &[u8]) -> Vec<u8> {
    stream.write_all(packet).unwrap();
    let head = recv_exact(stream, 16);
    let zip_size = u16::from_le_bytes([head[12], head[13]]) as usize;
    let unzip_size = u16::from_le_bytes([head[14], head[15]]) as usize;
    println!("  header: zip={}, unzip={}", zip_size, unzip_size);
    let mut body = Vec::with_capacity(zip_size);
    while body.len() < zip_size {
        let remaining = zip_size - body.len();
        let mut chunk = vec![0u8; remaining];
        let n = stream.read(&mut chunk).unwrap();
        if n == 0 {
            panic!("disconnected during body");
        }
        chunk.truncate(n);
        body.extend_from_slice(&chunk);
    }
    if zip_size != unzip_size {
        use flate2::read::ZlibDecoder;
        use std::io::Read as _;
        let mut decoder = ZlibDecoder::new(&body[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();
        decompressed
    } else {
        body
    }
}

#[test]
fn test_raw_connection() {
    // 服务器地址取自 DEFAULT_SERVERS，与客户端一致 —— 不再硬编码易过期的 IP
    let (_, ip, port) = DEFAULT_SERVERS[0];
    let addr = format!("{}:{}", ip, port);
    let mut stream = TcpStream::connect(&addr).unwrap_or_else(|e| {
        panic!("connect {addr} failed: {e} — 服务器不可达时本测试应整体跳过");
    });
    stream
        .set_read_timeout(Some(Duration::from_secs(5.0 as u64)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5.0 as u64)))
        .unwrap();

    // Setup
    send_and_recv(
        &mut stream,
        &[
            0x0c, 0x02, 0x18, 0x93, 0x00, 0x01, 0x03, 0x00, 0x03, 0x00, 0x0d, 0x00, 0x01,
        ],
    );
    send_and_recv(
        &mut stream,
        &[
            0x0c, 0x02, 0x18, 0x94, 0x00, 0x01, 0x03, 0x00, 0x03, 0x00, 0x0d, 0x00, 0x02,
        ],
    );
    send_and_recv(
        &mut stream,
        &[
            0x0c, 0x03, 0x18, 0x99, 0x00, 0x01, 0x20, 0x00, 0x20, 0x00, 0xdb, 0x0f, 0xd5, 0xd0,
            0xc9, 0xcc, 0xd6, 0xa4, 0xa8, 0xaf, 0x00, 0x00, 0x00, 0x8f, 0xc2, 0x25, 0x40, 0x13,
            0x00, 0x00, 0xd5, 0x00, 0xc9, 0xcc, 0xbd, 0xf0, 0xd7, 0xea, 0x00, 0x00, 0x00, 0x02,
        ],
    );

    // get_security_count
    let mut packet = Vec::new();
    packet.extend_from_slice(&[
        0x0c, 0x0c, 0x18, 0x6c, 0x00, 0x01, 0x08, 0x00, 0x08, 0x00, 0x4e, 0x04,
    ]);
    packet.extend_from_slice(&0u16.to_le_bytes());
    packet.extend_from_slice(&[0x75, 0xc7, 0x33, 0x01]);
    let body = send_and_recv(&mut stream, &packet);
    let count = u16::from_le_bytes([body[0], body[1]]);
    println!("count: {}", count);
    assert!(count > 0);

    stream.shutdown(std::net::Shutdown::Both).ok();
}
