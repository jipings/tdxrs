//! AsyncTdxHqClient 集成测试
//!
//! 需要网络连接，默认跳过。运行方式:
//!   cargo test --features integration --test test_async_client
//!
//! 测试覆盖: 连接管理、全部 13 个 API、并发请求、交易阶段检测、断线重连

#![cfg(feature = "integration")]

use tdxrs::net::async_client::{AsyncTdxHqClient, DEFAULT_POOL_SIZE};
use tdxrs::net::utils::TradingPhase;
use tdxrs::protocol::constants::DEFAULT_SERVERS;

/// 默认测试服务器
fn test_server() -> (&'static str, u16) {
    let (_, ip, port) = DEFAULT_SERVERS[0];
    (ip, port)
}

// ================================================================
// 连接管理
// ================================================================

#[tokio::test]
async fn test_connect_and_disconnect() {
    let (ip, port) = test_server();
    let client = AsyncTdxHqClient::new();
    let result = client.connect(ip, port, Some(5.0)).await;
    assert!(result.is_ok());
    assert!(result.unwrap());
    // 断言引用 async 侧自己的 DEFAULT_POOL_SIZE(=4，与同步侧 5 同名不同值)：
    // 此前两边各硬编码 4/5，谁改池大小都会静默腐烂 (CODE_REVIEW C2)
    assert_eq!(client.connection_count().await, DEFAULT_POOL_SIZE);

    client.disconnect().await;
    assert_eq!(client.connection_count().await, 0);
}

#[tokio::test]
async fn test_connect_to_any() {
    let client = AsyncTdxHqClient::new();
    let result = client.connect_to_any(Some(5.0)).await;
    assert!(result.is_ok());
    assert!(result.unwrap());
    assert!(client.connection_count().await > 0);
    client.disconnect().await;
}

// ================================================================
// 证券列表 / 数量
// ================================================================

#[tokio::test]
async fn test_get_security_count() {
    let (ip, port) = test_server();
    let client = AsyncTdxHqClient::new();
    client.connect(ip, port, Some(5.0)).await.unwrap();

    // 沪市
    let count = client.get_security_count(1).await;
    assert!(count.is_ok());
    assert!(count.unwrap() > 0);

    // 深市
    let count = client.get_security_count(0).await;
    assert!(count.is_ok());
    assert!(count.unwrap() > 0);

    client.disconnect().await;
}

#[tokio::test]
async fn test_get_security_list() {
    let (ip, port) = test_server();
    let client = AsyncTdxHqClient::new();
    client.connect(ip, port, Some(5.0)).await.unwrap();

    let list = client.get_security_list(1, 0).await;
    assert!(list.is_ok());
    let list = list.unwrap();
    assert!(!list.is_empty());
    // 第一条记录应有代码和名称
    assert!(!list[0].code.is_empty());

    client.disconnect().await;
}

// ================================================================
// K线
// ================================================================

#[tokio::test]
async fn test_get_security_bars() {
    let (ip, port) = test_server();
    let client = AsyncTdxHqClient::new();
    client.connect(ip, port, Some(5.0)).await.unwrap();

    // 600519 日K
    let bars = client.get_security_bars(4, 1, "600519", 0, 10, 0).await;
    assert!(bars.is_ok());
    let bars = bars.unwrap();
    assert_eq!(bars.len(), 10);
    assert!(bars[0].open > 0.0);
    assert!(!bars[0].datetime.is_empty());

    client.disconnect().await;
}

#[tokio::test]
async fn test_get_index_bars() {
    let (ip, port) = test_server();
    let client = AsyncTdxHqClient::new();
    client.connect(ip, port, Some(5.0)).await.unwrap();

    // 上证指数 日K
    let bars = client.get_index_bars(4, 1, "000001", 0, 10, 0).await;
    assert!(bars.is_ok());
    let bars = bars.unwrap();
    assert!(!bars.is_empty());

    client.disconnect().await;
}

// ================================================================
// 实时行情
// ================================================================

#[tokio::test]
async fn test_get_security_quotes() {
    let (ip, port) = test_server();
    let client = AsyncTdxHqClient::new();
    client.connect(ip, port, Some(5.0)).await.unwrap();

    let quotes = client.get_security_quotes(&[(1, "600519")]).await;
    assert!(quotes.is_ok());
    let quotes = quotes.unwrap();
    assert_eq!(quotes.len(), 1);
    assert!(quotes[0].price > 0.0);
    assert_eq!(quotes[0].code, "600519");

    client.disconnect().await;
}

// ================================================================
// 分时 / 逐笔
// ================================================================

#[tokio::test]
async fn test_get_minute_time_data() {
    let (ip, port) = test_server();
    let client = AsyncTdxHqClient::new();
    client.connect(ip, port, Some(5.0)).await.unwrap();

    // 当日分时在非交易时段/休市日按设计返回空列表 (README 差异表)，
    // 不能作为非空断言的依据 —— 周末跑必挂。改用指定交易日的历史分时:
    // 2026-09-11 (周五) 为确定性的交易日
    let data = client
        .get_history_minute_time_data(1, "600519", 20260911)
        .await;
    assert!(data.is_ok());
    let data = data.unwrap();
    assert!(!data.is_empty(), "历史分时在交易日应有数据");
    assert!(data[0].price > 0.0);

    client.disconnect().await;
}

#[tokio::test]
async fn test_get_transaction_data() {
    let (ip, port) = test_server();
    let client = AsyncTdxHqClient::new();
    client.connect(ip, port, Some(5.0)).await.unwrap();

    let data = client.get_transaction_data(1, "600519", 0, 100).await;
    assert!(data.is_ok());
    let data = data.unwrap();
    assert!(!data.is_empty());

    client.disconnect().await;
}

// ================================================================
// 财务 / 除权除息
// ================================================================

#[tokio::test]
async fn test_get_finance_info() {
    let (ip, port) = test_server();
    let client = AsyncTdxHqClient::new();
    client.connect(ip, port, Some(5.0)).await.unwrap();

    let info = client.get_finance_info(1, "600519").await;
    assert!(info.is_ok());
    let info = info.unwrap();
    assert_eq!(info.code, "600519");

    client.disconnect().await;
}

#[tokio::test]
async fn test_get_xdxr_info() {
    let (ip, port) = test_server();
    let client = AsyncTdxHqClient::new();
    client.connect(ip, port, Some(5.0)).await.unwrap();

    let data = client.get_xdxr_info(1, "600519").await;
    assert!(data.is_ok());
    let data = data.unwrap();
    assert!(!data.is_empty());

    client.disconnect().await;
}

// ================================================================
// 并发请求
// ================================================================

#[tokio::test]
async fn test_concurrent_requests() {
    let (ip, port) = test_server();
    let client = AsyncTdxHqClient::new();
    client.connect(ip, port, Some(5.0)).await.unwrap();

    // 4 个不同 API 并发执行
    let (bars, quotes, count, finance) = tokio::join!(
        client.get_security_bars(4, 1, "600519", 0, 5, 0),
        client.get_security_quotes(&[(1, "600519"), (0, "000858")]),
        client.get_security_count(1),
        client.get_finance_info(1, "600519"),
    );

    assert!(bars.is_ok());
    assert!(quotes.is_ok());
    assert!(count.is_ok());
    assert!(finance.is_ok());

    assert_eq!(bars.unwrap().len(), 5);
    assert_eq!(quotes.unwrap().len(), 2);
    assert!(count.unwrap() > 0);
    assert_eq!(finance.unwrap().code, "600519");

    client.disconnect().await;
}

// ================================================================
// 交易阶段检测
// ================================================================

#[tokio::test]
async fn test_phase_detection() {
    let mut client = AsyncTdxHqClient::new();
    let phase = client.auto_detect_phase();
    assert!(matches!(
        phase,
        TradingPhase::Trading | TradingPhase::PrePost | TradingPhase::Closed
    ));
}

// ================================================================
// 断线重连
// ================================================================

#[tokio::test]
async fn test_reconnect_after_disconnect() {
    let (ip, port) = test_server();
    let client = AsyncTdxHqClient::new();

    // 第一次连接
    client.connect(ip, port, Some(5.0)).await.unwrap();
    let count1 = client.get_security_count(1).await;
    assert!(count1.is_ok());

    // 断开
    client.disconnect().await;
    assert_eq!(client.connection_count().await, 0);

    // 重新连接
    client.connect(ip, port, Some(5.0)).await.unwrap();
    let count2 = client.get_security_count(1).await;
    assert!(count2.is_ok());

    // 结果应一致
    assert_eq!(count1.unwrap(), count2.unwrap());

    client.disconnect().await;
}
