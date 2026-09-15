//! 智能连接客户端 — 分层健康检查 + 本地缓存
//!
//! 与 `TdxHqClient` 相同的 API，但采用不同的连接策略:
//! - **快速初始连接**: 仅验证 TCP + 握手，不做 K 线健康检查
//! - **惰性健康检查**: 首次 K 线请求返回空时触发，自动切换服务器
//! - **本地缓存**: 记录成功/失败服务器，下次连接优先使用缓存
//! - **黑名单机制**: 连续失败的服务器自动加入黑名单 (24h 过期)
//!
//! ## 使用场景
//!
//! - 网络环境不稳定，部分服务器对当前用户不可用
//! - 需要快速初始化连接，首次 K 线请求可能需要重试
//! - 长期运行，需要自动适应服务器状态变化
//!
//! ## 与 TdxHqClient 对比
//!
//! | 维度 | TdxHqClient | TdxSmartClient |
//! |------|-------------|----------------|
//! | 初始连接 | 无健康检查 | 无健康检查 |
//! | K 线请求 | 直接返回 | 返回空时自动重试 |
//! | 服务器缓存 | 无 | 本地 JSON 缓存 |
//! | 黑名单 | 无 | 自动标记失败服务器 |
//! | 适用场景 | 网络稳定 | 网络不稳定 |
//!
//! ## 注意事项
//!
//! - 首次使用时无缓存，行为与 TdxHqClient 相同
//! - 缓存文件位于 `~/.tdxrs/server_cache.json`（`TDXRS_CACHE_DIR` 可覆盖目录）
//! - 黑名单有效期 24h，过期后自动重试

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::error::Result;
use crate::error_codes::ErrorCode;
use crate::net::client::TdxHqClient;
use crate::net::utils;
use crate::protocol::constants::*;
use crate::protocol::types::*;
use crate::{loge, logi, logw};

// ================================================================
// 服务器缓存
// ================================================================

/// 服务器缓存条目
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ServerCacheEntry {
    ip: String,
    port: u16,
    name: String,
    timestamp: u64,
    latency_ms: u32,
}

/// 黑名单条目
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct BlacklistEntry {
    ip: String,
    port: u16,
    reason: String,
    timestamp: u64,
}

/// 服务器统计
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ServerStats {
    success: u32,
    fail: u32,
    avg_latency: u32,
    /// 连续失败计数（成功清零）。旧缓存文件无此字段，serde default 兜底为 0。
    #[serde(default)]
    consecutive_fail: u32,
}

/// 连续失败达到该次数的服务器自动加入黑名单（成功即清零/解除）
const BLACKLIST_FAIL_STREAK: u32 = 3;

/// 服务器缓存文件结构
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ServerCache {
    version: u32,
    last_success: Option<ServerCacheEntry>,
    blacklist: Vec<BlacklistEntry>,
    server_stats: HashMap<String, ServerStats>,
    /// 落盘路径。`None` 表示纯内存（不落盘）。
    ///
    /// 不参与序列化：旧缓存文件无此字段，反序列化后由 `load_from` 绑定，
    /// 缓存文件格式保持 `version/last_success/blacklist/server_stats` 不变。
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl ServerCache {
    /// 空缓存，绑定默认用户路径 (`TDXRS_CACHE_DIR` 可覆盖)
    fn new() -> Self {
        Self::with_path(Self::cache_path())
    }

    /// 空缓存，绑定指定路径
    ///
    /// 单测必须用本构造器指向临时目录：`record_*`/`add_to_blacklist` 都会
    /// 立即落盘，若沿用默认路径，`cargo test` 会覆盖开发者真实的服务器缓存
    /// （实测后果：缓存被写成占位地址 1.2.3.4，下次冷启动在此地址上白等超时）。
    fn with_path(path: impl Into<PathBuf>) -> Self {
        Self {
            version: 1,
            last_success: None,
            blacklist: Vec::new(),
            server_stats: HashMap::new(),
            path: Some(path.into()),
        }
    }

    /// 获取缓存文件路径
    ///
    /// 优先级：`TDXRS_CACHE_DIR` > `USERPROFILE` > `HOME` > 当前目录。
    fn cache_path() -> PathBuf {
        if let Ok(dir) = std::env::var("TDXRS_CACHE_DIR") {
            if !dir.is_empty() {
                return PathBuf::from(dir).join("server_cache.json");
            }
        }
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        home.join(".tdxrs").join("server_cache.json")
    }

    /// 加载缓存
    fn load() -> Self {
        Self::load_from(Self::cache_path())
    }

    /// 从指定路径加载缓存（加载失败退回空缓存，仍绑定该路径）
    fn load_from(path: PathBuf) -> Self {
        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<Self>(&content) {
                Ok(mut cache) => {
                    cache.path = Some(path);
                    cache
                }
                Err(e) => {
                    logw!("cache", "failed to parse cache: {}", e);
                    Self::with_path(path)
                }
            },
            Err(_) => Self::with_path(path),
        }
    }

    /// 保存缓存（未绑定路径时不落盘）
    fn save(&self) {
        let Some(path) = self.path.as_ref() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(self) {
            Ok(content) => {
                if let Err(e) = fs::write(path, content) {
                    logw!("cache", "failed to save cache: {}", e);
                }
            }
            Err(e) => {
                logw!("cache", "failed to serialize cache: {}", e);
            }
        }
    }

    /// 清除 last_success（缓存指向不可用服务器时自愈）
    fn clear_last_success(&mut self) {
        if self.last_success.is_some() {
            self.last_success = None;
            self.save();
        }
    }

    /// 检查服务器是否在黑名单中
    fn is_blacklisted(&self, ip: &str, port: u16) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.blacklist.iter().any(|entry| {
            entry.ip == ip && entry.port == port && now - entry.timestamp < 86400
            // 24h 过期
        })
    }

    /// 添加服务器到黑名单
    fn add_to_blacklist(&mut self, ip: &str, port: u16, reason: &str) {
        // 避免重复添加
        if self.is_blacklisted(ip, port) {
            return;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.blacklist.push(BlacklistEntry {
            ip: ip.to_string(),
            port,
            reason: reason.to_string(),
            timestamp: now,
        });

        // 清理过期条目
        self.blacklist.retain(|entry| now - entry.timestamp < 86400);

        self.save();
    }

    /// 记录成功连接
    fn record_success(&mut self, ip: &str, port: u16, name: &str, latency_ms: u32) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.last_success = Some(ServerCacheEntry {
            ip: ip.to_string(),
            port,
            name: name.to_string(),
            timestamp: now,
            latency_ms,
        });

        // 更新统计
        let key = format!("{}:{}", ip, port);
        let stats = self.server_stats.entry(key).or_insert(ServerStats {
            success: 0,
            fail: 0,
            avg_latency: 0,
            consecutive_fail: 0,
        });
        stats.success += 1;
        stats.consecutive_fail = 0;
        stats.avg_latency = (stats.avg_latency * (stats.success - 1) + latency_ms) / stats.success;

        // 成功即解除拉黑：黑名单条目的恢复路径（probe_and_cache 探活成功）
        self.blacklist.retain(|e| !(e.ip == ip && e.port == port));

        self.save()
    }

    /// 记录失败连接
    fn record_failure(&mut self, ip: &str, port: u16) {
        let key = format!("{}:{}", ip, port);
        let should_blacklist = {
            let stats = self.server_stats.entry(key).or_insert(ServerStats {
                success: 0,
                fail: 0,
                avg_latency: 0,
                consecutive_fail: 0,
            });
            stats.fail += 1;
            stats.consecutive_fail += 1;
            stats.consecutive_fail >= BLACKLIST_FAIL_STREAK
        };
        if should_blacklist {
            self.add_to_blacklist(ip, port, "connect_fail_streak");
        }

        self.save()
    }
}

// ================================================================
// TdxSmartClient
// ================================================================

/// 智能连接客户端
///
/// 包装 `TdxHqClient`，增加惰性健康检查和服务器缓存功能。
pub struct TdxSmartClient {
    /// 内部客户端
    inner: TdxHqClient,
    /// 服务器缓存
    cache: Mutex<ServerCache>,
    /// 当前连接的服务器信息
    current_server: Mutex<Option<(String, u16, String)>>,
    /// 是否已通过健康检查
    health_checked: AtomicBool,
    /// 重试次数上限
    max_retry: usize,
}

impl TdxSmartClient {
    /// 创建新的智能客户端
    pub fn new() -> Self {
        Self {
            inner: TdxHqClient::new(),
            cache: Mutex::new(ServerCache::load()),
            current_server: Mutex::new(None),
            health_checked: AtomicBool::new(false),
            max_retry: 3,
        }
    }

    /// 连接到任意可用服务器 (快速模式)
    ///
    /// 仅验证 TCP + 握手，不做 K 线健康检查。
    /// 优先使用缓存的成功服务器。
    pub fn connect_to_any(&self, timeout: Option<f64>) -> Result<bool> {
        // 先 clone 出缓存条目再放锁：持锁跨 connect 会让 stats/保存路径阻塞
        // 整个 timeout 窗口（实测坏缓存下冷启动 8.14s 全程持锁）。
        let cached = self
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .last_success
            .clone();

        // 1. 尝试缓存的成功服务器
        if let Some(last) = cached {
            logi!(
                "smart",
                "trying cached server: {} ({}:{})",
                last.name,
                last.ip,
                last.port
            );
            match self.connect_and_record(&last.ip, last.port, &last.name, timeout) {
                Ok(true) => return Ok(true),
                _ => {
                    logw!(
                        "smart",
                        "cached server {} unavailable, trying next",
                        last.ip
                    );
                    // 自愈：缓存指向不可用服务器时清掉 last_success，
                    // 否则每次冷启动都要在这个地址上白等一个 timeout。
                    self.cache
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .clear_last_success();
                }
            }
        }

        // 2. 遍历 PRIMARY_SERVERS (跳过黑名单)
        for &(name, ip, port) in PRIMARY_SERVERS {
            if self
                .cache
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_blacklisted(ip, port)
            {
                logi!("smart", "skipping blacklisted server: {}:{}", ip, port);
                continue;
            }

            match self.connect_and_record(ip, port, name, timeout) {
                Ok(true) => return Ok(true),
                _ => continue,
            }
        }

        // 3. 兜底: 遍历 ALL_KNOWN_SERVERS
        for &(name, ip, port) in ALL_KNOWN_SERVERS {
            if self
                .cache
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_blacklisted(ip, port)
            {
                continue;
            }

            match self.connect_and_record(ip, port, name, timeout) {
                Ok(true) => return Ok(true),
                _ => continue,
            }
        }

        loge!("smart", "all servers unreachable");
        Err(ErrorCode::CONNECTION_FAILED.err("all servers unreachable"))
    }

    /// 连接指定服务器，成功后记录到缓存 (供 `connect_to_any` 遍历使用)
    fn connect_and_record(
        &self,
        ip: &str,
        port: u16,
        name: &str,
        timeout: Option<f64>,
    ) -> Result<bool> {
        let started = Instant::now();
        match self.inner.connect(ip, port, timeout) {
            Ok(true) => {
                *self
                    .current_server
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) =
                    Some((ip.to_string(), port, name.to_string()));
                self.health_checked.store(false, Ordering::SeqCst);
                // 回写成功服务器，使上次失败留下的坏缓存能在一次连接内自愈
                let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
                cache.record_success(ip, port, name, started.elapsed().as_millis() as u32);
                Ok(true)
            }
            _ => {
                let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
                cache.record_failure(ip, port);
                Ok(false)
            }
        }
    }

    /// 惰性健康检查
    ///
    /// 在首次 K 线请求返回空时调用。
    /// 如果健康检查失败，断开连接并尝试下一个服务器。
    fn lazy_health_check(&self) -> bool {
        if self.health_checked.load(Ordering::SeqCst) {
            return true;
        }

        let server = self
            .current_server
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some((ip, port, name)) = server {
            logi!(
                "smart",
                "performing lazy health check on {}:{}...",
                ip,
                port
            );

            // 使用 K 线请求验证
            let packet = utils::build_security_bars_packet(4, 1, "600519", 0, 1, 0);
            match self.inner.send_raw_and_recv(&packet) {
                Ok(body) => {
                    use crate::protocol::parsers::parse_security_bars;
                    match parse_security_bars(&body, 4) {
                        Ok(bars) if !bars.is_empty() => {
                            logi!("smart", "health check passed: got {} bars", bars.len());
                            self.health_checked.store(true, Ordering::SeqCst);
                            self.cache
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .record_success(&ip, port, &name, 0);
                            return true;
                        }
                        Ok(_) => {
                            logw!("smart", "health check failed: K-line empty, server may have protocol anomaly");
                            self.cache
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .add_to_blacklist(&ip, port, "kline_empty");
                            self.cache
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .record_failure(&ip, port);
                            self.inner.disconnect();
                            return false;
                        }
                        Err(e) => {
                            logw!("smart", "health check failed: parse error: {}", e);
                            self.cache
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .add_to_blacklist(&ip, port, "parse_error");
                            self.cache
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .record_failure(&ip, port);
                            self.inner.disconnect();
                            return false;
                        }
                    }
                }
                Err(e) => {
                    logw!("smart", "health check failed: {}", e);
                    self.cache
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .record_failure(&ip, port);
                    self.inner.disconnect();
                    return false;
                }
            }
        }

        false
    }

    /// 尝试切换到下一个服务器
    fn try_next_server(&self) -> Result<bool> {
        let current = self
            .current_server
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();

        // 遍历 PRIMARY_SERVERS，跳过当前和黑名单
        for &(name, ip, port) in PRIMARY_SERVERS {
            if let Some((ref cur_ip, cur_port, _)) = current {
                if ip == cur_ip && port == cur_port {
                    continue;
                }
            }

            if self
                .cache
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_blacklisted(ip, port)
            {
                continue;
            }

            let started = Instant::now();
            match self.inner.connect(ip, port, Some(5.0)) {
                Ok(true) => {
                    *self
                        .current_server
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) =
                        Some((ip.to_string(), port, name.to_string()));
                    self.health_checked.store(false, Ordering::SeqCst);
                    // 切服成功必须回写缓存：否则 last_success 仍指向刚切走的故障机，
                    // 下次冷启动会先在坏地址上白等一个 connect timeout
                    let latency = started.elapsed().as_millis() as u32;
                    self.cache
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .record_success(ip, port, name, latency);
                    logi!("smart", "switched to server: {}:{}", ip, port);
                    return Ok(true);
                }
                _ => {
                    // 候选连接失败同样记账（连续达标自动拉黑），不能无声略过
                    logw!("smart", "candidate {}:{} connect failed", ip, port);
                    self.cache
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .record_failure(ip, port);
                    continue;
                }
            }
        }

        loge!("smart", "no alternative server available");
        Err(ErrorCode::CONNECTION_FAILED.err("no alternative server available"))
    }

    /// 给当前连接的服务器记一次失败（请求报错、准备切服时调用）
    ///
    /// 惰性健康检查路径（lazy_health_check）各自记账；这里只补错误路径的缺口，
    /// 否则连接类故障永不进入连败统计、也永不拉黑。
    fn record_current_failure(&self) {
        let cur = self
            .current_server
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some((ip, port, _)) = cur {
            self.cache
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .record_failure(&ip, port);
        }
    }

    /// 获取 K 线数据 (带自动重试)
    ///
    /// 如果返回空数据，自动触发健康检查并尝试切换服务器。
    pub fn get_security_bars(
        &self,
        category: u8,
        market: u8,
        code: &str,
        start: u32,
        count: u16,
        fq: u8,
    ) -> Result<Vec<SecurityBar>> {
        let mut last_err = None;

        for attempt in 0..self.max_retry {
            match self
                .inner
                .get_security_bars(category, market, code, start, count, fq)
            {
                Ok(bars) if !bars.is_empty() => {
                    // 成功获取数据
                    if attempt > 0 {
                        logi!("smart", "got {} bars after {} retries", bars.len(), attempt);
                    }
                    return Ok(bars);
                }
                Ok(_) => {
                    // 返回空数据，触发健康检查
                    logw!(
                        "smart",
                        "attempt {}/{}: empty response, triggering health check",
                        attempt + 1,
                        self.max_retry
                    );

                    if !self.lazy_health_check() {
                        // 健康检查失败，尝试切换服务器
                        logw!("smart", "health check failed, trying next server...");
                        match self.try_next_server() {
                            Ok(true) => continue,
                            Ok(false) => {
                                last_err =
                                    Some(ErrorCode::CONNECTION_FAILED.err("no more servers"));
                                break;
                            }
                            Err(e) => {
                                last_err = Some(e);
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    logw!(
                        "smart",
                        "attempt {}/{}: error: {}, trying next server",
                        attempt + 1,
                        self.max_retry,
                        e
                    );
                    last_err = Some(e);
                    self.record_current_failure();
                    // 连接错误，尝试切换服务器
                    match self.try_next_server() {
                        Ok(true) => continue,
                        _ => break,
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| ErrorCode::RETRY_EXHAUSTED.err("max retry reached")))
    }

    /// 获取实时行情 (带自动重试)
    pub fn get_security_quotes(&self, all_stock: &[(u8, &str)]) -> Result<Vec<SecurityQuote>> {
        let mut last_err = None;

        for attempt in 0..self.max_retry {
            match self.inner.get_security_quotes(all_stock) {
                Ok(quotes) if !quotes.is_empty() => {
                    if attempt > 0 {
                        logi!(
                            "smart",
                            "got {} quotes after {} retries",
                            quotes.len(),
                            attempt
                        );
                    }
                    return Ok(quotes);
                }
                Ok(_) => {
                    // 返回空数据，触发健康检查
                    logw!(
                        "smart",
                        "attempt {}/{}: empty quotes, triggering health check",
                        attempt + 1,
                        self.max_retry
                    );

                    if !self.lazy_health_check() {
                        logw!("smart", "health check failed, trying next server...");
                        match self.try_next_server() {
                            Ok(true) => continue,
                            _ => break,
                        }
                    }
                }
                Err(e) => {
                    last_err = Some(e);
                    self.record_current_failure();
                    logw!(
                        "smart",
                        "attempt {}/{}: error, trying next server",
                        attempt + 1,
                        self.max_retry
                    );
                    match self.try_next_server() {
                        Ok(true) => continue,
                        _ => break,
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| ErrorCode::RETRY_EXHAUSTED.err("max retry reached")))
    }

    /// 委托其他方法到内部客户端
    pub fn inner(&self) -> &TdxHqClient {
        &self.inner
    }

    /// 获取缓存统计
    pub fn cache_stats(&self) -> String {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        format!(
            "last_success: {:?}, blacklist: {}, stats: {}",
            cache
                .last_success
                .as_ref()
                .map(|s| format!("{}:{}", s.ip, s.port)),
            cache.blacklist.len(),
            cache.server_stats.len()
        )
    }

    /// 清除缓存
    pub fn clear_cache(&self) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        *cache = ServerCache::new();
        cache.save();
        logi!("smart", "cache cleared");
    }

    /// 探测所有服务器并更新缓存
    ///
    /// 类似 mootdx 的 bestip 功能。
    pub fn probe_and_cache(&self, timeout_secs: f64) -> Vec<(String, u16, String, u32)> {
        let mut results = Vec::new();

        for &(name, ip, port) in PRIMARY_SERVERS {
            let start = Instant::now();
            match self.inner.connect(ip, port, Some(timeout_secs)) {
                Ok(true) => {
                    let latency = start.elapsed().as_millis() as u32;
                    self.cache
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .record_success(ip, port, name, latency);
                    results.push((ip.to_string(), port, name.to_string(), latency));
                    logi!("probe", "{}:{} ({}) - {}ms", ip, port, name, latency);
                    self.inner.disconnect();
                }
                _ => {
                    self.cache
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .record_failure(ip, port);
                    logw!("probe", "{}:{} ({}) - failed", ip, port, name);
                }
            }
        }

        results.sort_by_key(|r| r.3);
        results
    }
}

impl Default for TdxSmartClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 单测专用缓存路径：必须指向临时目录
    ///
    /// `record_*`/`add_to_blacklist` 会立即落盘，沿用默认路径会让 `cargo test`
    /// 覆盖开发者真实的 `~/.tdxrs/server_cache.json`（实测把 last_success 写成
    /// 占位地址 1.2.3.4，导致之后每次冷启动白等一个 timeout）。
    fn tmp_cache_path(tag: &str) -> PathBuf {
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("tdxrs_test_cache_{pid}_{tag}"));
        let _ = fs::remove_dir_all(&dir);
        dir.join("server_cache.json")
    }

    #[test]
    fn test_server_cache_blacklist() {
        let mut cache = ServerCache::with_path(tmp_cache_path("blacklist"));
        assert!(!cache.is_blacklisted("1.2.3.4", 7709));

        cache.add_to_blacklist("1.2.3.4", 7709, "test");
        assert!(cache.is_blacklisted("1.2.3.4", 7709));
        assert!(!cache.is_blacklisted("5.6.7.8", 7709));
    }

    #[test]
    fn test_server_cache_stats() {
        let mut cache = ServerCache::with_path(tmp_cache_path("stats"));
        cache.record_success("1.2.3.4", 7709, "test", 100);
        cache.record_success("1.2.3.4", 7709, "test", 200);
        cache.record_failure("1.2.3.4", 7709);

        let key = "1.2.3.4:7709".to_string();
        let stats = cache.server_stats.get(&key).unwrap();
        assert_eq!(stats.success, 2);
        assert_eq!(stats.fail, 1);
        assert_eq!(stats.avg_latency, 150); // (100 + 200) / 2
    }

    #[test]
    fn test_cache_persists_only_to_bound_path() {
        // 写绑定路径后可从该路径读回；文件格式不含 path 字段
        let path = tmp_cache_path("roundtrip");
        let mut cache = ServerCache::with_path(path.clone());
        cache.record_success("9.9.9.9", 7709, "srv", 42);

        let raw = fs::read_to_string(&path).expect("缓存应落盘");
        assert!(!raw.contains("path"), "path 字段不应进缓存文件: {raw}");
        let loaded = ServerCache::load_from(path);
        assert_eq!(loaded.last_success.unwrap().ip, "9.9.9.9");
    }

    #[test]
    fn test_default_cache_path_not_touched_by_bound_cache() {
        // 回归：非默认路径的缓存写入不得触碰真实用户缓存
        let default_path = ServerCache::cache_path();
        let before = fs::read_to_string(&default_path).ok();

        let mut cache = ServerCache::with_path(tmp_cache_path("isolation"));
        cache.record_success("1.2.3.4", 7709, "test", 100);
        cache.add_to_blacklist("1.2.3.4", 7709, "test");

        assert_eq!(
            fs::read_to_string(&default_path).ok(),
            before,
            "默认缓存文件被单测改写: {}",
            default_path.display()
        );
    }

    #[test]
    fn test_clear_last_success_self_heal() {
        // 自愈：坏缓存清掉 last_success 后不再被优先尝试
        let path = tmp_cache_path("selfheal");
        let mut cache = ServerCache::with_path(path.clone());
        cache.record_success("1.2.3.4", 7709, "bad", 100);
        assert!(cache.last_success.is_some());

        cache.clear_last_success();
        assert!(cache.last_success.is_none());
        assert!(ServerCache::load_from(path).last_success.is_none(), "落盘未生效");
    }

    #[test]
    fn test_record_failure_streak_blacklist() {
        // 连接类失败连续累计（成功清零），达到阈值自动拉黑
        let mut cache = ServerCache::with_path(tmp_cache_path("streak"));
        cache.record_failure("1.2.3.4", 7709);
        cache.record_failure("1.2.3.4", 7709);
        assert!(!cache.is_blacklisted("1.2.3.4", 7709), "连败 2 次不应拉黑");

        cache.record_failure("1.2.3.4", 7709);
        assert!(cache.is_blacklisted("1.2.3.4", 7709), "连败 3 次应拉黑");
        assert_eq!(
            cache.blacklist.last().unwrap().reason,
            "connect_fail_streak"
        );
    }

    #[test]
    fn test_record_success_resets_streak_and_unblacklists() {
        let mut cache = ServerCache::with_path(tmp_cache_path("reset"));
        cache.record_failure("1.2.3.4", 7709);
        cache.record_failure("1.2.3.4", 7709);
        cache.record_success("1.2.3.4", 7709, "srv", 10); // streak 清零
        cache.record_failure("1.2.3.4", 7709);
        cache.record_failure("1.2.3.4", 7709);
        assert!(
            !cache.is_blacklisted("1.2.3.4", 7709),
            "成功清零后连败不足阈值不应拉黑"
        );

        cache.record_failure("1.2.3.4", 7709); // streak=3 → 拉黑
        assert!(cache.is_blacklisted("1.2.3.4", 7709));
        cache.record_success("1.2.3.4", 7709, "srv", 10); // 成功解除（探活恢复路径）
        assert!(!cache.is_blacklisted("1.2.3.4", 7709), "成功应解除拉黑");
    }

    #[test]
    fn test_legacy_cache_without_streak_field_loads() {
        // 旧缓存文件的 server_stats 无 consecutive_fail 字段，serde default 兜底
        let path = tmp_cache_path("legacy");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{"version":1,"last_success":null,"blacklist":[],"server_stats":{"1.2.3.4:7709":{"success":1,"fail":1,"avg_latency":10}}}"#,
        )
        .unwrap();
        let cache = ServerCache::load_from(path);
        assert_eq!(cache.server_stats.get("1.2.3.4:7709").unwrap().consecutive_fail, 0);
    }
}
