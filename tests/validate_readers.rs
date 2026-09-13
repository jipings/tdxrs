//! Reader 集成验证 —— 内联构造二进制字节直接喂各解析器。
//!
//! 此前的实现依赖 tests/fixtures/（不入库）与仓库外 `../tdxpy` 的 golden
//! JSON：干净 clone 下 4 个测试全部静默空跑；golden 过滤口径（open-only）
//! 与生成器（四价格）不一致；`.lc5` 整数格式 fixture 却调浮点解析器
//! `parse_lc_min_bar`，等于没测到声称的路径 (CODE_REVIEW C3/C4)。
//!
//! 现改为完全自包含：字节在测试内构造，断言与字节同源，随时可跑。
//! 多记录、GBK 板块名、type 过滤等此前单测未覆盖的路径在此验证。

// ================================================================
// 编码辅助 —— 与 TDX 文件格式一一对应
// ================================================================

/// TDX 日期编码: (year-2004)*2048 + month*100 + day
fn tdx_date(year: u32, month: u32, day: u32) -> u32 {
    (year - 2004) * 2048 + month * 100 + day
}

/// TDX 时间编码: 当日分钟数
fn tdx_time(hour: u32, minute: u32) -> u16 {
    (hour * 60 + minute) as u16
}

/// 追加一条 .day 日线记录 `<IIIIIfII>`: date, open, high, low, close, amount, volume, reserved
// 记录布局天然 10 个字段，与协议层同例豁免
#[allow(clippy::too_many_arguments)]
fn push_day(
    data: &mut Vec<u8>,
    year: u32,
    month: u32,
    day: u32,
    open: u32,
    high: u32,
    low: u32,
    close: u32,
    amount: f32,
    volume: u32,
) {
    data.extend_from_slice(&tdx_date(year, month, day).to_le_bytes());
    data.extend_from_slice(&open.to_le_bytes());
    data.extend_from_slice(&high.to_le_bytes());
    data.extend_from_slice(&low.to_le_bytes());
    data.extend_from_slice(&close.to_le_bytes());
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&volume.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
}

/// 追加一条整数格式分钟线 `<HHIIIIfII>`: date, time, open, high, low, close, amount, volume, reserved
// 记录布局天然 12 个字段，与协议层同例豁免
#[allow(clippy::too_many_arguments)]
fn push_min_bar(
    data: &mut Vec<u8>,
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    open: u32,
    high: u32,
    low: u32,
    close: u32,
    amount: f32,
    volume: u32,
) {
    data.extend_from_slice(&(tdx_date(year, month, day) as u16).to_le_bytes());
    data.extend_from_slice(&tdx_time(hour, minute).to_le_bytes());
    data.extend_from_slice(&open.to_le_bytes());
    data.extend_from_slice(&high.to_le_bytes());
    data.extend_from_slice(&low.to_le_bytes());
    data.extend_from_slice(&close.to_le_bytes());
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&volume.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
}

/// 追加一条浮点格式分钟线 `<HHfffffII>`: date, time, open, high, low, close, amount, volume, reserved
// 记录布局天然 12 个字段，与协议层同例豁免
#[allow(clippy::too_many_arguments)]
fn push_lc_min_bar(
    data: &mut Vec<u8>,
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    open: f32,
    high: f32,
    low: f32,
    close: f32,
    amount: f32,
    volume: u32,
) {
    data.extend_from_slice(&(tdx_date(year, month, day) as u16).to_le_bytes());
    data.extend_from_slice(&tdx_time(hour, minute).to_le_bytes());
    data.extend_from_slice(&open.to_le_bytes());
    data.extend_from_slice(&high.to_le_bytes());
    data.extend_from_slice(&low.to_le_bytes());
    data.extend_from_slice(&close.to_le_bytes());
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&volume.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
}

// ================================================================
// 日线 .day —— 多记录
// ================================================================

#[test]
fn test_daily_bar_multi_record() {
    let mut data = Vec::new();
    // 3 条日K: 2023-01-09 ~ 2023-01-11, 系数 0.01 (价格 raw/100)
    push_day(
        &mut data,
        2023,
        1,
        9,
        185000,
        186200,
        184800,
        185550,
        8_345_678.5,
        45_600,
    );
    push_day(
        &mut data,
        2023,
        1,
        10,
        185550,
        187000,
        185200,
        186800,
        9_123_456.0,
        51_200,
    );
    push_day(
        &mut data,
        2023,
        1,
        11,
        186800,
        187400,
        186100,
        186500,
        6_500_000.0,
        38_900,
    );

    let records = tdxrs::reader::daily_bar::parse_daily_bar(&data, 0.01).expect("Parse failed");
    assert_eq!(records.len(), 3, "3 条记录应全部解析");

    // 逐条核验日期与 OHLC
    let expected = [
        ("2023-01-09", 1850.00, 1862.00, 1848.00, 1855.50, 45_600.0),
        ("2023-01-10", 1855.50, 1870.00, 1852.00, 1868.00, 51_200.0),
        ("2023-01-11", 1868.00, 1874.00, 1861.00, 1865.00, 38_900.0),
    ];
    for (r, (date, open, high, low, close, volume)) in records.iter().zip(expected) {
        assert_eq!(r.date, date, "date mismatch");
        assert!((r.open - open).abs() < 1e-9, "open {} != {}", r.open, open);
        assert!((r.high - high).abs() < 1e-9);
        assert!((r.low - low).abs() < 1e-9);
        assert!((r.close - close).abs() < 1e-9);
        assert_eq!(r.volume, volume);
    }
    assert_eq!(records[0].year, 2023);
    assert_eq!(records[0].month, 1);
    assert_eq!(records[0].day, 9);
}

// ================================================================
// 分钟线 .lc5/.lc1 —— 两种格式各自配对解析器
// ================================================================

/// 整数格式 `<HHIIIIfII>` —— 对应 `parse_min_bar` (OHLC raw/100)
#[test]
fn test_min_bar_integer_format() {
    let mut data = Vec::new();
    push_min_bar(
        &mut data,
        2023,
        1,
        9,
        9,
        35,
        185000,
        186200,
        184800,
        185550,
        1_234_567.5,
        5_000,
    );
    push_min_bar(
        &mut data, 2023, 1, 9, 9, 40, 185550, 186400, 185300, 186200, 987_654.0, 6_000,
    );

    let records = tdxrs::reader::min_bar::parse_min_bar(&data).expect("Parse failed");
    assert_eq!(records.len(), 2);

    assert_eq!(records[0].date, "2023-01-09 09:35");
    assert_eq!(records[0].hour, 9);
    assert_eq!(records[0].minute, 35);
    assert!(
        (records[0].open - 1850.00).abs() < 1e-9,
        "整数格式 OHLC 应除以 100"
    );
    assert!((records[0].high - 1862.00).abs() < 1e-9);
    assert!((records[0].low - 1848.00).abs() < 1e-9);
    assert!((records[0].close - 1855.50).abs() < 1e-9);
    assert_eq!(records[0].volume, 5_000.0);

    assert_eq!(records[1].date, "2023-01-09 09:40");
    assert!((records[1].close - 1862.00).abs() < 1e-9);
}

/// 浮点格式 `<HHfffffII>` —— 对应 `parse_lc_min_bar` (OHLC 即 f32，不再 /100)
///
/// 此前整数格式的 fixture 被喂给本解析器，f32 位模式读成 1e-42 级垃圾值，
/// 恰好年月日字段布局相同使断言碰巧通过 —— 两种格式必须各自配对 (C4)
#[test]
fn test_lc_min_bar_float_format() {
    let mut data = Vec::new();
    push_lc_min_bar(
        &mut data,
        2023,
        1,
        9,
        13,
        5,
        1850.0,
        1862.0,
        1848.0,
        1855.5,
        1_234_567.5,
        5_000,
    );
    push_lc_min_bar(
        &mut data, 2023, 1, 9, 13, 10, 1855.5, 1864.0, 1853.0, 1862.0, 876_543.0, 6_000,
    );

    let records = tdxrs::reader::min_bar::parse_lc_min_bar(&data).expect("Parse failed");
    assert_eq!(records.len(), 2);

    assert_eq!(records[0].date, "2023-01-09 13:05");
    assert_eq!(records[0].hour, 13);
    assert_eq!(records[0].minute, 5);
    // f32 精度 ~7 位有效数字，用 1e-4 容差
    assert!(
        (records[0].open - 1850.0).abs() < 1e-4,
        "浮点格式 OHLC 不应再除以 100"
    );
    assert!((records[0].high - 1862.0).abs() < 1e-4);
    assert!((records[0].close - 1855.5).abs() < 1e-4);
    assert_eq!(records[0].volume, 5_000.0);

    assert_eq!(records[1].date, "2023-01-09 13:10");
    assert!((records[1].close - 1862.0).abs() < 1e-4);
}

// ================================================================
// 板块 .dat —— flat + group 双模式, type 过滤, GBK 板块名
// ================================================================

fn build_block_file() -> Vec<u8> {
    let mut data = vec![0u8; 384]; // 文件头 384 字节全 0
    data.extend_from_slice(&2u16.to_le_bytes()); // 2 个板块

    // 板块1: type=1 —— parse_block/parse_block_group 均应跳过 (只认 type=2)
    data.extend_from_slice(b"skip blk\x00"); // 9 字节 ASCII 名
    data.extend_from_slice(&1u16.to_le_bytes()); // stock_count = 1
    data.extend_from_slice(&1u16.to_le_bytes()); // block_type = 1
    data.extend_from_slice(b"600000\x00"); // 1 条代码 (7 字节)
    data.extend(std::iter::repeat_n(0u8, 2800 - 7)); // 代码区补齐 2800

    // 板块2: type=2, GBK 板块名 "指数板块" (d6 b8 ca fd b0 e5 bf e9) + \x00
    data.extend_from_slice(&[0xd6, 0xb8, 0xca, 0xfd, 0xb0, 0xe5, 0xbf, 0xe9, 0x00]);
    data.extend_from_slice(&3u16.to_le_bytes()); // stock_count = 3
    data.extend_from_slice(&2u16.to_le_bytes()); // block_type = 2
    for code in [b"600519\x00", b"000858\x00", b"399001\x00"] {
        data.extend_from_slice(code);
    }
    data.extend(std::iter::repeat_n(0u8, 2800 - 3 * 7)); // 代码区补齐 2800

    data
}

#[test]
fn test_block_flat_and_group() {
    let data = build_block_file();

    // flat 模式: type=1 的板块被过滤，只产出 type=2 板块的 3 条代码记录
    let records = tdxrs::reader::block::parse_block(&data).expect("Parse failed");
    assert_eq!(records.len(), 3, "type=1 板块应被过滤");
    assert_eq!(records[0].code, "600519");
    assert_eq!(records[1].code, "000858");
    assert_eq!(records[2].code, "399001");
    assert_eq!(records[0].blockname, "指数板块", "GBK 板块名解码");
    assert_eq!(records[0].block_type, 2);
    assert_eq!(records[0].code_index, 0);
    assert_eq!(records[1].code_index, 1);

    // group 模式: 只保留 type=2 的 1 个分组
    let groups = tdxrs::reader::block::parse_block_group(&data).expect("Parse failed");
    assert_eq!(groups.len(), 1, "type=1 板块应被过滤");
    assert_eq!(groups[0].blockname, "指数板块");
    assert_eq!(groups[0].stock_count, 3);
    assert_eq!(groups[0].code_list, "600519,000858,399001");
}

// ================================================================
// 财务 gpcw*.dat —— 多股票
// ================================================================

#[test]
fn test_financial_multi_stock() {
    // Header 20 字节: type=i16, report_date=u32, max_count=u16, resv, report_size, resv
    let report_size: u32 = 4 * 4; // 每股 4 个 f32
    let mut data = Vec::new();
    data.extend_from_slice(&1i16.to_le_bytes());
    data.extend_from_slice(&20241231u32.to_le_bytes());
    data.extend_from_slice(&2u16.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&report_size.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());

    // 索引 11 字节/条: 6 字节代码 + 分隔符 + u32 报告偏移
    let offset_1 = 20 + 2 * 11; // 第一条报告数据紧跟索引区
    let offset_2 = offset_1 + report_size as usize;
    for (code, offset) in [("600519", offset_1), ("000858", offset_2)] {
        data.extend_from_slice(code.as_bytes());
        data.push(0x00);
        data.extend_from_slice(&(offset as u32).to_le_bytes());
    }

    // 报告数据
    let fields_1 = [1835.0f32, 1849.98, 1807.82, 1841.2];
    let fields_2 = [150.5f32, 155.0, 148.0, 152.3];
    for v in fields_1.iter().chain(fields_2.iter()) {
        data.extend_from_slice(&v.to_le_bytes());
    }

    let records = tdxrs::reader::financial::parse_financial(&data).expect("Parse failed");
    assert_eq!(records.len(), 2);

    assert_eq!(records[0].code, "600519");
    assert_eq!(records[0].report_date, 20241231);
    assert_eq!(records[0].fields.len(), 4);
    for (got, want) in records[0].fields.iter().zip(fields_1) {
        assert!((got - want).abs() < 0.01, "field {} != {}", got, want);
    }

    assert_eq!(records[1].code, "000858");
    for (got, want) in records[1].fields.iter().zip(fields_2) {
        assert!((got - want).abs() < 0.01);
    }
}
