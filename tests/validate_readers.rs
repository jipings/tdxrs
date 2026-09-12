use std::fs;

fn golden_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tdxpy")
        .join("test_data")
        .join("golden")
}

fn fixtures_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures")
}

#[test]
fn test_daily_bar_600519() {
    let path = fixtures_dir().join("600519.day");
    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(_) => {
            eprintln!("SKIP {}: fixture 缺失: {} (可运行 tests/gen_binary_fixtures.py 生成)", std::path::Path::new(&file!()).file_name().unwrap().to_string_lossy(), path.display());
            return;
        }
    };
    let records = tdxrs::reader::daily_bar::parse_daily_bar(&data, 0.01).expect("Parse failed");

    // Load golden reference
    let golden_path = golden_dir().join("bars_600519_cat9_日K线.json");
    let golden_data: Vec<serde_json::Value> =
        serde_json::from_str(&match fs::read_to_string(&golden_path) {
            Ok(s) => s,
            Err(_) => {
                eprintln!("SKIP: golden 缺失: {}", golden_path.display());
                return;
            }
        }).unwrap();

    // Filter valid golden records (matching our fixture filter)
    let valid_golden: Vec<_> = golden_data
        .iter()
        .filter(|b| {
            let year = b["year"].as_u64().unwrap_or(0);
            let month = b["month"].as_u64().unwrap_or(0);
            let day = b["day"].as_u64().unwrap_or(0);
            let volume = b["volume"].as_f64().unwrap_or(0.0);
            let amount = b["amount"].as_f64().unwrap_or(0.0);
            let open = b["open"].as_f64().unwrap_or(0.0);

            (1990..=2100).contains(&year)
                && (1..=12).contains(&month)
                && (1..=31).contains(&day)
                && (0.0..=4294967295.0).contains(&volume)
                && (0.0..=1e15).contains(&amount)
                && open > 0.0
                && open <= 100000.0
        })
        .collect();

    assert_eq!(records.len(), valid_golden.len());

    for (i, (record, golden)) in records.iter().zip(valid_golden.iter()).enumerate() {
        let g_year = golden["year"].as_u64().unwrap();
        let g_month = golden["month"].as_u64().unwrap();
        let g_day = golden["day"].as_u64().unwrap();

        assert_eq!(record.year, g_year as u32, "[{i}] year mismatch");
        assert_eq!(record.month, g_month as u32, "[{i}] month mismatch");
        assert_eq!(record.day, g_day as u32, "[{i}] day mismatch");

        let g_open = golden["open"].as_f64().unwrap();
        assert!(
            (record.open - g_open).abs() < 0.02,
            "[{i}] open mismatch: {} vs {}",
            record.open,
            g_open
        );

        let g_volume = golden["volume"].as_f64().unwrap();
        assert!(
            (record.volume - g_volume).abs() < 1.0,
            "[{i}] volume mismatch: {} vs {}",
            record.volume,
            g_volume
        );
    }
}

#[test]
fn test_min_bar_600519() {
    let path = fixtures_dir().join("600519.lc5");
    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(_) => {
            eprintln!("SKIP {}: fixture 缺失: {} (可运行 tests/gen_binary_fixtures.py 生成)", std::path::Path::new(&file!()).file_name().unwrap().to_string_lossy(), path.display());
            return;
        }
    };
    let records = tdxrs::reader::min_bar::parse_lc_min_bar(&data).expect("Parse failed");

    assert!(!records.is_empty());
    assert_eq!(records[0].year, 2023);
    assert_eq!(records[0].month, 1);
    assert_eq!(records[0].day, 9);
}

#[test]
fn test_block_reader() {
    let path = fixtures_dir().join("test_block.dat");
    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(_) => {
            eprintln!("SKIP {}: fixture 缺失: {} (可运行 tests/gen_binary_fixtures.py 生成)", std::path::Path::new(&file!()).file_name().unwrap().to_string_lossy(), path.display());
            return;
        }
    };
    let records = tdxrs::reader::block::parse_block(&data).expect("Parse failed");

    assert_eq!(records.len(), 5);
    assert_eq!(records[0].code, "600000");
    assert_eq!(records[1].code, "000001");
    assert_eq!(records[2].code, "300750");
    assert_eq!(records[3].code, "000001");
    assert_eq!(records[4].code, "399001");
}

#[test]
fn test_financial_reader() {
    let path = fixtures_dir().join("test_finance.dat");
    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(_) => {
            eprintln!("SKIP {}: fixture 缺失: {} (可运行 tests/gen_binary_fixtures.py 生成)", std::path::Path::new(&file!()).file_name().unwrap().to_string_lossy(), path.display());
            return;
        }
    };
    let records = tdxrs::reader::financial::parse_financial(&data).expect("Parse failed");

    assert_eq!(records.len(), 2);
    assert_eq!(records[0].code, "600519");
    assert_eq!(records[0].report_date, 20241231);
    assert_eq!(records[0].fields.len(), 4);
    assert!((records[0].fields[0] - 1835.0).abs() < 0.01);
}
