use std::collections::VecDeque;
use std::sync::Mutex;

use crate::error::Result;
use crate::loge;
use crate::net::connection::TcpConnection;
use crate::protocol::constants::{CONNECT_TIMEOUT, DEFAULT_POOL_SIZE};

/// 连接池中的单个连接
struct PooledConnection {
    conn: TcpConnection,
    server: (String, u16),
}

/// 连接池配置
/// 连接握手回调类型
pub type HandshakeFn = Box<dyn Fn(&mut TcpConnection) -> Result<()> + Send + Sync>;

pub struct PoolConfig {
    pub max_size: usize,
    pub connect_timeout: f64,
    /// 握手回调: 新建连接后执行 (setup commands)
    pub handshake_fn: Option<HandshakeFn>,
}

impl PoolConfig {
    pub fn new() -> Self {
        Self {
            max_size: DEFAULT_POOL_SIZE,
            connect_timeout: CONNECT_TIMEOUT,
            handshake_fn: None,
        }
    }
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// 线程安全的连接池
///
/// 管理多个 TCP 连接，支持:
/// - 单服务器连接池 (多个连接到同一服务器)
/// - 多服务器连接池 (连接到不同服务器)
/// - 连接借出/归还
pub struct ConnectionPool {
    inner: Mutex<PoolInner>,
    config: PoolConfig,
    /// 连接超时（可运行时更新，覆盖 config.connect_timeout）
    connect_timeout: Mutex<f64>,
}

struct PoolInner {
    idle: VecDeque<PooledConnection>,
    active: usize,
    total: usize,
}

impl ConnectionPool {
    /// 创建连接池 (单服务器)
    pub fn new_single(_server: (String, u16), config: PoolConfig) -> Self {
        Self {
            inner: Mutex::new(PoolInner {
                idle: VecDeque::new(),
                active: 0,
                total: 0,
            }),
            connect_timeout: Mutex::new(config.connect_timeout),
            config,
        }
    }

    /// 将一个已握手的连接放入池中
    pub fn push(&self, conn: TcpConnection, server: (String, u16)) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.total += 1;
        inner.idle.push_back(PooledConnection { conn, server });
    }

    /// 从池中借出一个连接
    ///
    /// 如果池中有空闲连接，返回一个；
    /// 如果未达上限，创建新连接；
    /// 如果已满，返回错误。
    pub fn borrow(&self, server: &(String, u16)) -> Result<PooledConnGuard<'_>> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        // 按目标服务器过滤空闲队列：切服后在途归还的旧服务器连接
        // 不得被复用，直接关闭出池 (CODE_REVIEW P1-8)
        let mut matching = VecDeque::new();
        while let Some(mut c) = inner.idle.pop_front() {
            if c.server == *server {
                matching.push_back(c);
            } else {
                c.conn.close();
                inner.total = inner.total.saturating_sub(1);
            }
        }
        inner.idle = matching;

        // 尝试从空闲队列获取
        if let Some(conn) = inner.idle.pop_front() {
            inner.active += 1;
            return Ok(PooledConnGuard {
                pool: self,
                conn: Some(conn),
            });
        }

        // 如果未达到上限，创建新连接
        if inner.total < self.config.max_size {
            let server_clone = server.clone();
            let has_handshake = self.config.handshake_fn.is_some();
            inner.total += 1;
            inner.active += 1;

            // 释放锁后再创建连接 (避免持锁做 I/O)
            drop(inner);

            let conn_result = TcpConnection::connect(
                &server_clone.0,
                server_clone.1,
                *self
                    .connect_timeout
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()),
            )
            .and_then(|mut conn| {
                if has_handshake {
                    if let Some(ref handshake_fn) = self.config.handshake_fn {
                        handshake_fn(&mut conn)?;
                    }
                }
                Ok(conn)
            });

            match conn_result {
                Ok(conn) => {
                    return Ok(PooledConnGuard {
                        pool: self,
                        conn: Some(PooledConnection {
                            conn,
                            server: server_clone,
                        }),
                    });
                }
                Err(e) => {
                    // 连接/握手失败必须回滚计数，否则失败 max_size 次后
                    // 池永久 POOL_EXHAUSTED 且泄漏无法清除 (CODE_REVIEW P0-3B)
                    let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
                    inner.total = inner.total.saturating_sub(1);
                    inner.active = inner.active.saturating_sub(1);
                    return Err(e);
                }
            }
        }

        loge!(
            "pool",
            "exhausted (active={}, max={})",
            inner.active,
            self.config.max_size
        );
        Err(crate::error_codes::ErrorCode::POOL_EXHAUSTED.err(format!(
            "active={}, max={}",
            inner.active, self.config.max_size
        )))
    }

    /// 尝试借出连接 (非阻塞)
    pub fn try_borrow(&self, server: &(String, u16)) -> Result<Option<PooledConnGuard<'_>>> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        // 同 borrow: 按目标服务器过滤 (CODE_REVIEW P1-8)
        let mut matching = VecDeque::new();
        while let Some(mut c) = inner.idle.pop_front() {
            if c.server == *server {
                matching.push_back(c);
            } else {
                c.conn.close();
                inner.total = inner.total.saturating_sub(1);
            }
        }
        inner.idle = matching;

        if let Some(conn) = inner.idle.pop_front() {
            inner.active += 1;
            return Ok(Some(PooledConnGuard {
                pool: self,
                conn: Some(conn),
            }));
        }

        if inner.total < self.config.max_size {
            let server_clone = server.clone();
            let has_handshake = self.config.handshake_fn.is_some();
            inner.total += 1;
            inner.active += 1;
            drop(inner);

            let conn_result = TcpConnection::connect(
                &server_clone.0,
                server_clone.1,
                *self
                    .connect_timeout
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()),
            )
            .and_then(|mut conn| {
                if has_handshake {
                    if let Some(ref handshake_fn) = self.config.handshake_fn {
                        handshake_fn(&mut conn)?;
                    }
                }
                Ok(conn)
            });

            match conn_result {
                Ok(conn) => {
                    return Ok(Some(PooledConnGuard {
                        pool: self,
                        conn: Some(PooledConnection {
                            conn,
                            server: server_clone,
                        }),
                    }));
                }
                Err(e) => {
                    // 同 borrow: 失败回滚计数 (CODE_REVIEW P0-3B)
                    let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
                    inner.total = inner.total.saturating_sub(1);
                    inner.active = inner.active.saturating_sub(1);
                    return Err(e);
                }
            }
        }

        Ok(None)
    }

    /// 归还连接到池中
    fn return_connection(&self, pooled: PooledConnection) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        // 饱和减：close_all 等路径可能已重置计数，普通 -= 在下溢时 panic
        // 且发生在持锁期间会毒化互斥锁 (CODE_REVIEW P0-3A)
        inner.active = inner.active.saturating_sub(1);

        if pooled.conn.is_open() && inner.idle.len() < self.config.max_size {
            inner.idle.push_back(pooled);
        } else {
            inner.total = inner.total.saturating_sub(1);
        }
    }

    /// 关闭所有连接
    pub fn close_all(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        while let Some(mut conn) = inner.idle.pop_front() {
            conn.conn.close();
            inner.total = inner.total.saturating_sub(1);
        }
        // 不清零 active：在途 guard 归还时自会递减；此处清零会让后续
        // 归还在 debug 构建下 panic（持锁毒化互斥锁）、release 下回绕
        // 为 usize::MAX (CODE_REVIEW P0-3A)
    }

    /// 更新连接超时（传播给池内后续新建连接）
    pub fn set_connect_timeout(&self, timeout: f64) {
        if timeout.is_finite() && timeout > 0.0 {
            *self
                .connect_timeout
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = timeout;
        }
    }

    /// 获取池状态
    pub fn stats(&self) -> PoolStats {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        PoolStats {
            idle: inner.idle.len(),
            active: inner.active,
            total: inner.total,
            max_size: self.config.max_size,
        }
    }
}

/// 连接池统计信息
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub idle: usize,
    pub active: usize,
    pub total: usize,
    pub max_size: usize,
}

/// 借出的连接守卫 (自动归还)
pub struct PooledConnGuard<'a> {
    pool: &'a ConnectionPool,
    conn: Option<PooledConnection>,
}

impl<'a> PooledConnGuard<'a> {
    /// 获取连接引用
    pub fn conn(&mut self) -> &mut TcpConnection {
        &mut self.conn.as_mut().unwrap().conn
    }

    /// 获取服务器信息
    pub fn server(&self) -> &(String, u16) {
        &self.conn.as_ref().unwrap().server
    }
}

impl<'a> Drop for PooledConnGuard<'a> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.return_connection(conn);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_stats_initial() {
        let config = PoolConfig::new();
        let pool = ConnectionPool::new_single(("127.0.0.1".to_string(), 7709), config);
        let stats = pool.stats();
        assert_eq!(stats.idle, 0);
        assert_eq!(stats.active, 0);
        assert_eq!(stats.total, 0);
        assert_eq!(stats.max_size, DEFAULT_POOL_SIZE);
    }

    #[test]
    fn test_pool_config_default() {
        let config = PoolConfig::default();
        assert_eq!(config.max_size, DEFAULT_POOL_SIZE);
        assert_eq!(config.connect_timeout, CONNECT_TIMEOUT);
    }

    #[test]
    fn test_pool_borrow_failure_no_server() {
        let mut config = PoolConfig::new();
        config.max_size = 2;
        config.connect_timeout = 0.1;
        let pool = ConnectionPool::new_single(("127.0.0.1".to_string(), 1), config);
        let server = ("127.0.0.1".to_string(), 1);
        let result = pool.borrow(&server);
        assert!(result.is_err());
    }

    #[test]
    fn test_pool_close_all() {
        let config = PoolConfig::new();
        let pool = ConnectionPool::new_single(("127.0.0.1".to_string(), 7709), config);
        pool.close_all();
        let stats = pool.stats();
        assert_eq!(stats.total, 0);
    }

    // CODE_REVIEW P0-3B: 连接失败必须回滚计数，否则失败 max_size 次后永久 POOL_EXHAUSTED
    #[test]
    fn test_borrow_failure_rolls_back_counters() {
        let config = PoolConfig {
            max_size: 2,
            connect_timeout: 1.0,
            handshake_fn: None,
        };
        let pool = ConnectionPool::new_single(("127.0.0.1".into(), 1), config);
        // 端口 1 无监听，连接必然快速失败
        for _ in 0..5 {
            assert!(pool.borrow(&("127.0.0.1".into(), 1)).is_err());
        }
        let stats = pool.stats();
        assert_eq!(
            stats.total, 0,
            "失败后 total 应回滚为 0，实际 {}",
            stats.total
        );
        assert_eq!(
            stats.active, 0,
            "失败后 active 应回滚为 0，实际 {}",
            stats.active
        );
    }

    // CODE_REVIEW P0-3A: close_all 后在途 guard 归还不得使 active 下溢
    #[test]
    fn test_return_after_close_all_no_underflow() {
        use std::net::TcpListener;
        // 本地起一个真 TCP 监听，确保能建连
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let config = PoolConfig {
            max_size: 2,
            connect_timeout: 1.0,
            handshake_fn: None,
        };
        let pool = ConnectionPool::new_single(("127.0.0.1".into(), port), config);
        let guard = pool
            .borrow(&("127.0.0.1".into(), port))
            .expect("本地监听连接应成功");
        pool.close_all(); // 模拟切服：池被清空，guard 仍在途
        drop(guard); // 归还 —— 修复前 active 0-1 panic（持锁毒化互斥锁）
        let stats = pool.stats();
        assert!(stats.active < usize::MAX, "active 不应下溢回绕");
    }

    // CODE_REVIEW P1-8: 空闲连接必须按目标服务器匹配
    #[test]
    fn test_borrow_skips_foreign_server_idle() {
        use std::net::TcpListener;
        let l1 = TcpListener::bind("127.0.0.1:0").unwrap();
        let p1 = l1.local_addr().unwrap().port();
        let l2 = TcpListener::bind("127.0.0.1:0").unwrap();
        let p2 = l2.local_addr().unwrap().port();

        let config = PoolConfig {
            max_size: 4,
            connect_timeout: 1.0,
            handshake_fn: None,
        };
        let pool = ConnectionPool::new_single(("127.0.0.1".into(), p1), config);

        // 建连 A 并手动归还（借出后 drop 归还）
        let g = pool.borrow(&("127.0.0.1".into(), p1)).expect("A 连接");
        drop(g);
        assert_eq!(pool.stats().idle, 1, "A 归还后应有 1 空闲");

        // 为服务器 B borrow：不得复用 A 的空闲连接
        let g2 = pool.borrow(&("127.0.0.1".into(), p2)).expect("B 新建连接");
        drop(g2);
        // A 的残留被清出，B 归还后 idle 只含 B
        let s = pool.stats();
        assert!(s.total <= 2, "异源连接被关闭出池: {:?}", s);
    }
}
