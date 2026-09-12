//! 板块数据类型定义

/// 板块类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockType {
    /// 指数成分 (block_zs.dat)
    Index,
    /// 行业板块 (block_fg.dat) — 通达信板块标签
    Industry,
    /// 概念板块 (block_gn.dat)
    Concept,
}

impl BlockType {
    /// 对应的文件名
    pub fn filename(&self) -> &'static str {
        match self {
            BlockType::Index => "block_zs.dat",
            BlockType::Industry => "block_fg.dat",
            BlockType::Concept => "block_gn.dat",
        }
    }

    /// 从字符串解析
    // 非标准 FromStr 语义 (返回 Option 而非 Result)，保持既有 API
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "index" | "zs" | "指数" => Some(BlockType::Index),
            "industry" | "fg" | "行业" => Some(BlockType::Industry),
            "concept" | "gn" | "概念" => Some(BlockType::Concept),
            _ => None,
        }
    }

    /// 所有板块类型
    pub fn all() -> &'static [BlockType] {
        &[BlockType::Index, BlockType::Industry, BlockType::Concept]
    }
}

/// 板块信息
#[derive(Debug, Clone)]
pub struct BlockInfo {
    pub name: String,
    pub block_type: BlockType,
    pub stock_count: usize,
}

/// 板块成分查询结果
#[derive(Debug, Clone)]
pub struct BlockConstituents {
    pub block_name: String,
    pub block_type: BlockType,
    pub codes: Vec<String>,
}
