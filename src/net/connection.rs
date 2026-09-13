use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};

use crate::error::{Result, TdxError};

pub struct TcpConnection {
    stream: TcpStream,
    /// 收发一旦出错即标记失活：peer_addr() 对已连接 socket 几乎恒为 Ok
    /// （对端 close、本端 shutdown 后仍成功），不能用作存活判定
    /// (CODE_REVIEW P1-8)
    healthy: std::cell::Cell<bool>,
}

impl TcpConnection {
    pub fn connect(ip: &str, port: u16, timeout_secs: f64) -> Result<Self> {
        let addr = format!("{}:{}", ip, port);
        // timeout 是 Python 传入的裸 f64，全链路无校验：负数/NaN/超界值
        // 会让 from_secs_f64 panic（PanicException 不可被 except Exception
        // 捕获）(CODE_REVIEW P1-7)
        if !timeout_secs.is_finite() || timeout_secs <= 0.0 {
            return Err(TdxError::Connection(format!(
                "invalid timeout {}s (must be finite and > 0)",
                timeout_secs
            )));
        }
        // 上界钳制，防止极大有限值溢出 Duration
        let timeout_secs = timeout_secs.min(86400.0);
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
        Ok(Self { stream, healthy: std::cell::Cell::new(true) })
    }

    pub fn send(&mut self, data: &[u8]) -> Result<()> {
        self.stream.write_all(data).map_err(|e| {
            self.healthy.set(false);
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
                self.healthy.set(false);
                TdxError::Connection(format!("recv failed: {}", e))
            })?;
            if n == 0 {
                self.healthy.set(false);
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
        // peer_addr() 对死 socket 也几乎恒 Ok，不可用作存活判定 (P1-8)；
        // 改为"未发生过收发错误 && socket 无错误挂起"
        self.healthy.get() && matches!(self.stream.take_error(), Ok(None))
    }
}
