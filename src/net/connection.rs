use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};

use crate::error::{Result, TdxError};

pub struct TcpConnection {
    stream: TcpStream,
}

impl TcpConnection {
    pub fn connect(ip: &str, port: u16, timeout_secs: f64) -> Result<Self> {
        let addr = format!("{}:{}", ip, port);
        // 连接阶段也必须受超时约束：阻塞式 TcpStream::connect 对不可达
        // 地址会挂到 OS 级 TCP 超时(~2min)，connect_to_any 遍历上百台
        // 服务器时最坏可挂数小时 (CODE_REVIEW P1-4)
        let sock_addr = addr
            .to_socket_addrs()
            .map_err(|e| TdxError::Connection(format!("resolve {}: {}", addr, e)))?
            .next()
            .ok_or_else(|| TdxError::Connection(format!("resolve {}: no address", addr)))?;
        let connect_to = std::time::Duration::from_secs_f64(timeout_secs.max(0.001));
        let stream = TcpStream::connect_timeout(&sock_addr, connect_to).map_err(|e| {
            TdxError::Connection(format!("Failed to connect to {}: {}", addr, e))
        })?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs_f64(timeout_secs)))
            .map_err(|e| TdxError::Connection(format!("set_read_timeout: {}", e)))?;
        stream
            .set_write_timeout(Some(std::time::Duration::from_secs_f64(timeout_secs)))
            .map_err(|e| TdxError::Connection(format!("set_write_timeout: {}", e)))?;
        Ok(Self { stream })
    }

    pub fn send(&mut self, data: &[u8]) -> Result<()> {
        self.stream.write_all(data).map_err(|e| {
            TdxError::Connection(format!("send failed: {}", e))
        })?;
        Ok(())
    }

    /// Read exactly `len` bytes, looping until all received or error.
    pub fn recv(&mut self, len: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        let mut total = 0;
        while total < len {
            let n = self.stream.read(&mut buf[total..]).map_err(|e| {
                TdxError::Connection(format!("recv failed: {}", e))
            })?;
            if n == 0 {
                return Err(TdxError::Disconnected);
            }
            total += n;
        }
        Ok(buf)
    }

    pub fn close(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }

    pub fn is_open(&self) -> bool {
        self.stream.peer_addr().is_ok()
    }
}
