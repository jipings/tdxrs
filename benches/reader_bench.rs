use criterion::{criterion_group, criterion_main, Criterion};

fn bench_daily_bar_parse(c: &mut Criterion) {
    // 构造测试数据: 1000条日线记录
    let mut data = Vec::with_capacity(1000 * 32);
    for _ in 0..1000 {
        let date: u32 = 36979; // 2024-01-15
        let open: u32 = 1050;
        let high: u32 = 1080;
        let low: u32 = 1030;
        let close: u32 = 1060;
        let amount: f32 = 123456.5;
        let volume: u32 = 50000;
        let reserved: u32 = 0;

        data.extend_from_slice(&date.to_le_bytes());
        data.extend_from_slice(&open.to_le_bytes());
        data.extend_from_slice(&high.to_le_bytes());
        data.extend_from_slice(&low.to_le_bytes());
        data.extend_from_slice(&close.to_le_bytes());
        data.extend_from_slice(&amount.to_le_bytes());
        data.extend_from_slice(&volume.to_le_bytes());
        data.extend_from_slice(&reserved.to_le_bytes());
    }

    c.bench_function("daily_bar_parse_1000", |b| {
        b.iter(|| tdxrs::reader::daily_bar::parse_daily_bar(&data, 0.01).unwrap())
    });
}

fn bench_min_bar_parse(c: &mut Criterion) {
    // 构造测试数据: 1000条分钟线记录
    let mut data = Vec::with_capacity(1000 * 32);
    for _ in 0..1000 {
        let date: u16 = 36979u16;
        let time: u16 = 575;
        let open: u32 = 1050;
        let high: u32 = 1080;
        let low: u32 = 1030;
        let close: u32 = 1060;
        let amount: f32 = 123456.5;
        let volume: u32 = 5000;
        let reserved: u32 = 0;

        data.extend_from_slice(&date.to_le_bytes());
        data.extend_from_slice(&time.to_le_bytes());
        data.extend_from_slice(&open.to_le_bytes());
        data.extend_from_slice(&high.to_le_bytes());
        data.extend_from_slice(&low.to_le_bytes());
        data.extend_from_slice(&close.to_le_bytes());
        data.extend_from_slice(&amount.to_le_bytes());
        data.extend_from_slice(&volume.to_le_bytes());
        data.extend_from_slice(&reserved.to_le_bytes());
    }

    c.bench_function("min_bar_parse_1000", |b| {
        b.iter(|| tdxrs::reader::min_bar::parse_min_bar(&data).unwrap())
    });
}

criterion_group!(benches, bench_daily_bar_parse, bench_min_bar_parse);
criterion_main!(benches);
