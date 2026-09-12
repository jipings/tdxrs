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
    except ImportError:
        TdxF10Client = None
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
