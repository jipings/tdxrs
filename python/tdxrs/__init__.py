"""tdxrs - 通达信行情数据解析库 (Rust 实现)

核心模块:
- Reader: 日线、分钟线、板块、财务数据解析
- Client: 行情客户端 (TdxHqClient, AsyncTdxHqClient, TdxDirectClient, TdxSmartClient, TdxHqFundClient, TdxBlockClient)

用法:
    from tdxrs import TdxHqClient, TdxSmartClient, DailyBarReader
"""

try:
    from tdxrs._internal import (
        DailyBarReader, MinBarReader, LcMinBarReader, BlockReader, FinancialReader,
        TdxHqClient, AsyncTdxHqClient, TdxDirectClient, TdxSmartClient, TdxHqFundClient, TdxBlockClient,
    )
    try:
        # 需要 cargo feature "f10" (默认启用)
        from tdxrs._internal import TdxF10Client
        _HAS_F10 = True
    except ImportError:
        _HAS_F10 = False
except ImportError:
    raise ImportError(
        "tdxrs native module not found. Please install with: pip install tdxrs"
    )

__version__ = "0.6.7"
__all__ = [
    "DailyBarReader", "MinBarReader", "LcMinBarReader", "BlockReader", "FinancialReader",
    "TdxHqClient", "AsyncTdxHqClient", "TdxDirectClient", "TdxSmartClient", "TdxHqFundClient", "TdxBlockClient",
    "TdxF10Client",
]


def __getattr__(name):
    if name == "TdxF10Client" and not _HAS_F10:
        raise ImportError(
            "TdxF10Client 需要 cargo feature 'f10'（默认启用，当前构建未包含）。"
            "重新构建: cargo build --features f10"
        )
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
