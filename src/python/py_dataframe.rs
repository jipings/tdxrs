//! DataFrame 输出 — 通过 dict-of-lists → pd.DataFrame() 构建
//!
//! 相比于 list[dict], 此方式将数据按列组织, pandas 可直接利用列式内存布局

use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::IntoPyObjectExt;

use crate::protocol::types::*;

/// 构建 dict-of-lists 并调用 pd.DataFrame()
fn make_dataframe(py: Python<'_>, columns: Vec<(&str, Vec<Py<PyAny>>)>) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    for (name, values) in &columns {
        dict.set_item(*name, values.as_slice())?;
    }
    let pd = py.import("pandas")?;
    let df = pd.call_method1("DataFrame", (dict,))?;
    Ok(df.into())
}

// ============================================================
// Bars DataFrame
// ============================================================

pub fn security_bars_to_df(py: Python<'_>, bars: &[SecurityBar]) -> PyResult<Py<PyAny>> {
    let n = bars.len();
    let mut opens = Vec::with_capacity(n);
    let mut closes = Vec::with_capacity(n);
    let mut highs = Vec::with_capacity(n);
    let mut lows = Vec::with_capacity(n);
    let mut vols = Vec::with_capacity(n);
    let mut amounts = Vec::with_capacity(n);
    let mut years = Vec::with_capacity(n);
    let mut months = Vec::with_capacity(n);
    let mut days = Vec::with_capacity(n);
    let mut hours = Vec::with_capacity(n);
    let mut minutes = Vec::with_capacity(n);
    let mut datetimes = Vec::with_capacity(n);

    for b in bars {
        opens.push(b.open.into_py_any(py)?);
        closes.push(b.close.into_py_any(py)?);
        highs.push(b.high.into_py_any(py)?);
        lows.push(b.low.into_py_any(py)?);
        vols.push(b.vol.into_py_any(py)?);
        amounts.push(b.amount.into_py_any(py)?);
        years.push(b.year.into_py_any(py)?);
        months.push(b.month.into_py_any(py)?);
        days.push(b.day.into_py_any(py)?);
        hours.push(b.hour.into_py_any(py)?);
        minutes.push(b.minute.into_py_any(py)?);
        datetimes.push(b.datetime.as_str().into_py_any(py)?);
    }

    make_dataframe(
        py,
        vec![
            ("datetime", datetimes),
            ("year", years),
            ("month", months),
            ("day", days),
            ("hour", hours),
            ("minute", minutes),
            ("open", opens),
            ("high", highs),
            ("low", lows),
            ("close", closes),
            ("vol", vols),
            ("amount", amounts),
        ],
    )
}

pub fn index_bars_to_df(py: Python<'_>, bars: &[IndexBar]) -> PyResult<Py<PyAny>> {
    let n = bars.len();
    let mut opens = Vec::with_capacity(n);
    let mut closes = Vec::with_capacity(n);
    let mut highs = Vec::with_capacity(n);
    let mut lows = Vec::with_capacity(n);
    let mut vols = Vec::with_capacity(n);
    let mut amounts = Vec::with_capacity(n);
    let mut years = Vec::with_capacity(n);
    let mut months = Vec::with_capacity(n);
    let mut days = Vec::with_capacity(n);
    let mut hours = Vec::with_capacity(n);
    let mut minutes = Vec::with_capacity(n);
    let mut datetimes = Vec::with_capacity(n);
    let mut up_counts = Vec::with_capacity(n);
    let mut down_counts = Vec::with_capacity(n);

    for b in bars {
        opens.push(b.open.into_py_any(py)?);
        closes.push(b.close.into_py_any(py)?);
        highs.push(b.high.into_py_any(py)?);
        lows.push(b.low.into_py_any(py)?);
        vols.push(b.vol.into_py_any(py)?);
        amounts.push(b.amount.into_py_any(py)?);
        years.push(b.year.into_py_any(py)?);
        months.push(b.month.into_py_any(py)?);
        days.push(b.day.into_py_any(py)?);
        hours.push(b.hour.into_py_any(py)?);
        minutes.push(b.minute.into_py_any(py)?);
        datetimes.push(b.datetime.as_str().into_py_any(py)?);
        up_counts.push(b.up_count.into_py_any(py)?);
        down_counts.push(b.down_count.into_py_any(py)?);
    }

    make_dataframe(
        py,
        vec![
            ("datetime", datetimes),
            ("year", years),
            ("month", months),
            ("day", days),
            ("hour", hours),
            ("minute", minutes),
            ("open", opens),
            ("high", highs),
            ("low", lows),
            ("close", closes),
            ("vol", vols),
            ("amount", amounts),
            ("up_count", up_counts),
            ("down_count", down_counts),
        ],
    )
}

// ============================================================
// Quotes DataFrame
// ============================================================

pub fn quotes_to_df(py: Python<'_>, quotes: &[SecurityQuote]) -> PyResult<Py<PyAny>> {
    let n = quotes.len();
    let mut markets = Vec::with_capacity(n);
    let mut codes = Vec::with_capacity(n);
    let mut prices = Vec::with_capacity(n);
    let mut last_close = Vec::with_capacity(n);
    let mut opens = Vec::with_capacity(n);
    let mut highs = Vec::with_capacity(n);
    let mut lows = Vec::with_capacity(n);
    let mut vols = Vec::with_capacity(n);
    let mut cur_vols = Vec::with_capacity(n);
    let mut amounts = Vec::with_capacity(n);
    let mut s_vols = Vec::with_capacity(n);
    let mut b_vols = Vec::with_capacity(n);
    let mut servers = Vec::with_capacity(n);

    for q in quotes {
        markets.push(q.market.into_py_any(py)?);
        codes.push(q.code.as_str().into_py_any(py)?);
        prices.push(q.price.into_py_any(py)?);
        last_close.push(q.last_close.into_py_any(py)?);
        opens.push(q.open.into_py_any(py)?);
        highs.push(q.high.into_py_any(py)?);
        lows.push(q.low.into_py_any(py)?);
        vols.push(q.vol.into_py_any(py)?);
        cur_vols.push(q.cur_vol.into_py_any(py)?);
        amounts.push(q.amount.into_py_any(py)?);
        s_vols.push(q.s_vol.into_py_any(py)?);
        b_vols.push(q.b_vol.into_py_any(py)?);
        servers.push(q.servertime.as_str().into_py_any(py)?);
    }

    make_dataframe(
        py,
        vec![
            ("code", codes),
            ("market", markets),
            ("price", prices),
            ("last_close", last_close),
            ("open", opens),
            ("high", highs),
            ("low", lows),
            ("vol", vols),
            ("cur_vol", cur_vols),
            ("amount", amounts),
            ("s_vol", s_vols),
            ("b_vol", b_vols),
            ("servertime", servers),
        ],
    )
}

// ============================================================
// DailyBarRecord DataFrame (Reader)
// ============================================================

pub fn daily_records_to_df(
    py: Python<'_>,
    records: &[crate::reader::daily_bar::DailyBarRecord],
) -> PyResult<Py<PyAny>> {
    let n = records.len();
    let mut dates = Vec::with_capacity(n);
    let mut opens = Vec::with_capacity(n);
    let mut highs = Vec::with_capacity(n);
    let mut lows = Vec::with_capacity(n);
    let mut closes = Vec::with_capacity(n);
    let mut amounts = Vec::with_capacity(n);
    let mut volumes = Vec::with_capacity(n);
    let mut years = Vec::with_capacity(n);
    let mut months = Vec::with_capacity(n);
    let mut days_v = Vec::with_capacity(n);

    for r in records {
        dates.push(r.date.as_str().into_py_any(py)?);
        opens.push(r.open.into_py_any(py)?);
        highs.push(r.high.into_py_any(py)?);
        lows.push(r.low.into_py_any(py)?);
        closes.push(r.close.into_py_any(py)?);
        amounts.push(r.amount.into_py_any(py)?);
        volumes.push(r.volume.into_py_any(py)?);
        years.push(r.year.into_py_any(py)?);
        months.push(r.month.into_py_any(py)?);
        days_v.push(r.day.into_py_any(py)?);
    }

    make_dataframe(
        py,
        vec![
            ("date", dates),
            ("year", years),
            ("month", months),
            ("day", days_v),
            ("open", opens),
            ("high", highs),
            ("low", lows),
            ("close", closes),
            ("volume", volumes),
            ("amount", amounts),
        ],
    )
}

// ============================================================
// Finance DataFrame (multi-stock)
// ============================================================

pub fn finance_to_df(py: Python<'_>, infos: &[(FinanceInfo,)]) -> PyResult<Py<PyAny>> {
    let n = infos.len();
    let mut markets = Vec::with_capacity(n);
    let mut codes = Vec::with_capacity(n);
    let mut zonggubens = Vec::with_capacity(n);
    let mut liutonggubens = Vec::with_capacity(n);
    let mut jingzichans = Vec::with_capacity(n);
    let mut jingliruns = Vec::with_capacity(n);
    let mut zhuyingshourus = Vec::with_capacity(n);
    let mut meigujingzichans = Vec::with_capacity(n);
    let mut yingyeliruns = Vec::with_capacity(n);
    let mut provinces = Vec::with_capacity(n);
    let mut industries = Vec::with_capacity(n);

    for (info,) in infos {
        markets.push(info.market.into_py_any(py)?);
        codes.push(info.code.as_str().into_py_any(py)?);
        zonggubens.push(info.zongguben.into_py_any(py)?);
        liutonggubens.push(info.liutongguben.into_py_any(py)?);
        jingzichans.push(info.jingzichan.into_py_any(py)?);
        jingliruns.push(info.jinglirun.into_py_any(py)?);
        zhuyingshourus.push(info.zhuyingshouru.into_py_any(py)?);
        meigujingzichans.push(info.meigujingzichan.into_py_any(py)?);
        yingyeliruns.push(info.yingyelirun.into_py_any(py)?);
        provinces.push(info.province.into_py_any(py)?);
        industries.push(info.industry.into_py_any(py)?);
    }

    make_dataframe(
        py,
        vec![
            ("code", codes),
            ("market", markets),
            ("zongguben", zonggubens),
            ("liutongguben", liutonggubens),
            ("jingzichan", jingzichans),
            ("jinglirun", jingliruns),
            ("zhuyingshouru", zhuyingshourus),
            ("yingyelirun", yingyeliruns),
            ("meigujingzichan", meigujingzichans),
            ("province", provinces),
            ("industry", industries),
        ],
    )
}

// ============================================================
// MinBarRecord DataFrame
// ============================================================

pub fn min_records_to_df(
    py: Python<'_>,
    records: &[crate::reader::min_bar::MinBarRecord],
) -> PyResult<Py<PyAny>> {
    let n = records.len();
    let mut dates = Vec::with_capacity(n);
    let mut opens = Vec::with_capacity(n);
    let mut highs = Vec::with_capacity(n);
    let mut lows = Vec::with_capacity(n);
    let mut closes = Vec::with_capacity(n);
    let mut amounts = Vec::with_capacity(n);
    let mut volumes = Vec::with_capacity(n);
    let mut years = Vec::with_capacity(n);
    let mut months = Vec::with_capacity(n);
    let mut days_v = Vec::with_capacity(n);
    let mut hours = Vec::with_capacity(n);
    let mut minutes = Vec::with_capacity(n);

    for r in records {
        dates.push(r.date.as_str().into_py_any(py)?);
        opens.push(r.open.into_py_any(py)?);
        highs.push(r.high.into_py_any(py)?);
        lows.push(r.low.into_py_any(py)?);
        closes.push(r.close.into_py_any(py)?);
        amounts.push(r.amount.into_py_any(py)?);
        volumes.push(r.volume.into_py_any(py)?);
        years.push(r.year.into_py_any(py)?);
        months.push(r.month.into_py_any(py)?);
        days_v.push(r.day.into_py_any(py)?);
        hours.push(r.hour.into_py_any(py)?);
        minutes.push(r.minute.into_py_any(py)?);
    }

    make_dataframe(
        py,
        vec![
            ("date", dates),
            ("year", years),
            ("month", months),
            ("day", days_v),
            ("hour", hours),
            ("minute", minutes),
            ("open", opens),
            ("high", highs),
            ("low", lows),
            ("close", closes),
            ("volume", volumes),
            ("amount", amounts),
        ],
    )
}
