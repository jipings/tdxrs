"""CLI / Downloader 端到端冒烟测试 (CODE_REVIEW 补项)

分层:
- 无网络用例: CLI parse（合成二进制文件）、Downloader 写入器、
  checkpoint、--servers 解析 —— 任何时候都跑
- 网络用例: 连不上服务器自动 skip

运行: pytest tests/test_smoke.py -v
"""
import csv
import json
import os
import socket
import struct
import subprocess
import sys
import tempfile

import pytest

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PY = sys.executable

GOOD_HOST = ("59.36.5.11", 7709)


def server_reachable():
    try:
        s = socket.create_connection(GOOD_HOST, timeout=3)
        s.close()
        return True
    except OSError:
        return False


# ================================================================
# CLI parse —— 合成文件，无网络
# ================================================================

def _run_cli(*args):
    return subprocess.run(
        [PY, "-m", "tdxrs.cli", *args],
        capture_output=True, text=True, cwd=REPO, timeout=60,
    )


def _synth_day(path):
    rec = struct.pack("<IIIIIfII", 20260904, 1050, 1100, 1040, 1080, 123456.0, 98700, 0)
    with open(path, "wb") as f:
        f.write(rec * 3)


def _synth_min(path):
    rec = struct.pack("<HHIIIIfII", 45960, 630, 1050, 1100, 1040, 1080, 123456.0, 98700, 0)
    with open(path, "wb") as f:
        f.write(rec * 3)


def _synth_block(path):
    name = "测试板块".encode("gbk")[:9].ljust(9, b"\x00")
    codes = b"".join(c.encode().ljust(7, b"\x00") for c in ("600000", "600036"))
    with open(path, "wb") as f:
        f.write(b"\x00" * 384 + struct.pack("<H", 1) + name + struct.pack("<HH", 2, 2) + codes)


def test_cli_parse_daily():
    with tempfile.TemporaryDirectory() as td:
        p = os.path.join(td, "sh600000.day")
        _synth_day(p)
        r = _run_cli("parse", p, "--type", "daily")
        assert r.returncode == 0, r.stderr
        assert "10.50" in r.stdout  # 开盘
        assert "98,700" in r.stdout  # 成交量（P0-5 前会与成交额对调）


def test_cli_parse_min():
    with tempfile.TemporaryDirectory() as td:
        p = os.path.join(td, "sz000001.min")
        _synth_min(p)
        r = _run_cli("parse", p, "--type", "min")
        assert r.returncode == 0, r.stderr
        assert "10.50" in r.stdout and "10.80" in r.stdout  # 开/收（P0-11 前整体错位 5 列）


def test_cli_parse_block():
    """P0-7 前必然 TypeError（多传参数 + 字段名错误）"""
    with tempfile.TemporaryDirectory() as td:
        p = os.path.join(td, "block.dat")
        _synth_block(p)
        r = _run_cli("parse", p, "--type", "block")
        assert r.returncode == 0, r.stderr
        assert "600000" in r.stdout and "600036" in r.stdout


# ================================================================
# Downloader 写入器 —— 无网络
# ================================================================

def _bar(dt, c):
    return {"datetime": dt, "open": c, "high": c, "low": c, "close": c, "amount": 1000.0, "vol": 100.0}


@pytest.fixture()
def dl():
    from tdxrs.downloader import Downloader
    return Downloader(data_dir="/tmp/tdxrs_smoke_unused")


def test_downloader_tdx_incremental_keeps_history(dl):
    """P0-5 核心回归: .day 增量追加不丢历史"""
    from tdxrs import DailyBarReader
    with tempfile.TemporaryDirectory() as td:
        p = os.path.join(td, "000001.day")
        old = [_bar(f"2026-08-{d:02d}", 10.0) for d in range(3, 29)]
        dl._write_tdx(p, old)
        n_before = len(DailyBarReader().parse_file(p))
        dl._write_tdx(p, [_bar("2026-09-11", 11.0)], append=True)
        bars = DailyBarReader().parse_file(p)
        assert len(bars) == n_before + 1
        assert bars[0]["date"] == "2026-08-03"
        assert bars[-1]["date"] == "2026-09-11"


def test_downloader_minute_csv_time_alignment(dl):
    """P0-6 核心回归: 时间标签来自记录自身而非固定槽位"""
    data = [
        {"time": "15:00", "price": 11.0, "vol": 100.0},
        {"time": "14:59", "price": 10.9, "vol": 90.0},
        {"time": "09:31", "price": 10.5, "vol": 50.0},
    ]
    with tempfile.TemporaryDirectory() as td:
        p = os.path.join(td, "m.csv")
        dl._write_minute_csv(p, data, 20260911)
        with open(p) as f:
            rows = list(csv.reader(f))[1:]
        assert rows[0][0].endswith("09:31") and rows[-1][0].endswith("15:00")
        assert float(rows[0][1]) == 10.5


def test_downloader_checkpoint_roundtrip(dl):
    """P2-7 回归: checkpoint 读写"""
    from datetime import datetime
    os.makedirs("/tmp/tdxrs_smoke_unused/.tdxrs_meta", exist_ok=True)
    dl._save_checkpoint("sz", "daily", "000002", 2, 3000)
    ck = dl._load_checkpoint()
    assert ck and ck["last_code"] == "000002" and ck["done"] == 2
    # 过期忽略
    stale = {"updated_at": "2020-01-01T00:00:00", "market": "sz", "category": "daily",
             "last_code": "x", "done": 1, "total": 2, "stats": {}}
    with open(dl._checkpoint_path, "w") as f:
        json.dump(stale, f)
    assert dl._load_checkpoint() is None


def test_cli_parse_servers():
    """P2-8 回归: --servers 解析为三元组"""
    from tdxrs.cli import _parse_servers
    assert _parse_servers("1.2.3.4:7709") == [("srv0", "1.2.3.4", 7709)]
    assert _parse_servers("bad") is None
    assert _parse_servers(None) is None


def test_cli_auto_market_bj():
    """P2-9 回归: 北交所代码不归深证"""
    from tdxrs.cli import auto_market
    assert auto_market("600519") == 1
    assert auto_market("000001") == 0
    assert auto_market("430047") is None


# ================================================================
# 网络用例 —— 连不上自动 skip
# ================================================================

@pytest.mark.skipif(not server_reachable(), reason="行情服务器不可达")
def test_cli_quote_smoke():
    r = _run_cli("quote", "000001")
    assert r.returncode == 0, r.stderr


@pytest.mark.skipif(not server_reachable(), reason="行情服务器不可达")
def test_downloader_run_smoke():
    from tdxrs.downloader import Downloader
    from tdxrs import DailyBarReader
    with tempfile.TemporaryDirectory() as td:
        d = Downloader(data_dir=td, rate_limit=30)
        d.run(markets=["sz"], categories=["daily"], codes=["000001"],
              start_date="2026-09-07", end_date="2026-09-11")
        p = os.path.join(td, "sz", "daily", "000001.day")
        assert os.path.exists(p), "下载产物缺失"
        bars = DailyBarReader().parse_file(p)
        assert len(bars) >= 1
