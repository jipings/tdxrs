use std::sync::OnceLock;
use std::time::SystemTime;

/// 日期校验的最大年份 (运行时计算: 当前年份 + 10, 首次调用后缓存)
pub fn max_valid_year() -> u32 {
    static YEAR: OnceLock<u32> = OnceLock::new();
    *YEAR.get_or_init(|| {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let year = (now / 31_556_952 + 1970) as u32; // 365.2425 天 ≈ 秒数
        year + 10
    })
}

/// TDX 日期编码: year = num / 2048 + 2004, month = (num % 2048) / 100, day = (num % 2048) % 100
/// 同时支持 YYYYMMDD 格式 (2004年前的数据)
pub fn decode_date(num: u32) -> (u32, u32, u32) {
    // 检测格式: YYYYMMDD 格式 > 100000, TDX 编码 < 100000
    if num > 100000 {
        // YYYYMMDD 格式 (2004年前的数据)
        let year = num / 10000;
        let month = (num % 10000) / 100;
        let day = num % 100;
        (year, month, day)
    } else {
        // TDX 编码格式
        let year = num / 2048 + 2004;
        let month = (num % 2048) / 100;
        let day = (num % 2048) % 100;
        (year, month, day)
    }
}

/// TDX 日期编码 (u16 版本，用于分钟线)
pub fn decode_date_u16(num: u16) -> (u32, u32, u32) {
    let year = num as u32 / 2048 + 2004;
    let month = (num as u32 % 2048) / 100;
    let day = (num as u32 % 2048) % 100;
    (year, month, day)
}

/// TDX 时间解码: minutes since midnight -> (hour, minute)
pub fn decode_time(minutes: u16) -> (u32, u32) {
    (minutes as u32 / 60, minutes as u32 % 60)
}

/// 格式化日期为 "YYYY-MM-DD" 字符串
pub fn format_date(year: u32, month: u32, day: u32) -> String {
    format!("{:04}-{:02}-{:02}", year, month, day)
}

/// 格式化日期时间为 "YYYY-MM-DD HH:MM" 字符串
pub fn format_datetime(year: u32, month: u32, day: u32, hour: u32, minute: u32) -> String {
    format!("{:04}-{:02}-{:02} {:02}:{:02}", year, month, day, hour, minute)
}

/// 安全的字节切片索引
#[inline(always)]
pub fn get_byte(data: &[u8], pos: usize) -> u8 {
    data[pos]
}

/// 安全地读取 u32 (little-endian)
#[inline(always)]
pub fn read_u32(data: &[u8], pos: usize) -> u32 {
    if pos + 4 > data.len() {
        return 0;
    }
    u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
}

/// 安全地读取 u16 (little-endian)
#[inline(always)]
pub fn read_u16(data: &[u8], pos: usize) -> u16 {
    if pos + 2 > data.len() {
        return 0;
    }
    u16::from_le_bytes([data[pos], data[pos + 1]])
}

/// 安全地读取 f32 (little-endian)
#[inline(always)]
/// 注意：越界时 panic（裸索引），仅可用于已校验长度的本地文件
pub fn read_f32(data: &[u8], pos: usize) -> f32 {
    f32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
}

/// 安全地读取 i32 (little-endian)
#[inline(always)]
pub fn read_i32(data: &[u8], pos: usize) -> i32 {
    i32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
}

/// 安全地读取 i64 (little-endian)
#[inline(always)]
pub fn read_i64(data: &[u8], pos: usize) -> i64 {
    i64::from_le_bytes([
        data[pos], data[pos + 1], data[pos + 2], data[pos + 3],
        data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_valid_year_range() {
        let y = max_valid_year();
        // 当前 2026, +10 = 2036. 有效范围: 2036 ~ 2040+
        assert!((2036..=2050).contains(&y), "max_valid_year={} out of expected range", y);
    }

    #[test]
    fn test_max_valid_year_is_cached() {
        let y1 = max_valid_year();
        let y2 = max_valid_year();
        assert_eq!(y1, y2);
    }

    #[test]
    fn test_decode_date() {
        // zip_day 格式: (year-2004)*2048 + month*100 + day
        // 2026-01-02: (22)*2048 + 100 + 2 = 45158
        let (y, m, d) = decode_date(45158);
        assert_eq!(y, 2026);
        assert_eq!(m, 1);
        assert_eq!(d, 2);
    }

    #[test]
    fn test_decode_date_u16() {
        // 同 decode_date, u16 版本
        let (y, m, d) = decode_date_u16(45158);
        assert_eq!(y, 2026);
        assert_eq!(m, 1);
        assert_eq!(d, 2);
    }

    #[test]
    fn test_format_date() {
        assert_eq!(format_date(2026, 6, 23), "2026-06-23");
    }

    #[test]
    fn test_format_datetime() {
        assert_eq!(format_datetime(2026, 6, 23, 14, 30), "2026-06-23 14:30");
    }
}
