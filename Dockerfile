# tdxrs — multi-stage Docker build for Linux wheel
#
# Build test image (runs `import tdxrs` self-check):
#   docker build -t tdxrs .
#   docker run --rm tdxrs
#
# Extract the Linux wheel:
#   docker build --target builder -t tdxrs-builder .
#   docker run --rm -v "$PWD":/out tdxrs-builder sh -c 'cp dist/*.whl /out/'

# === Stage 1: Build ===
FROM python:3.13-slim AS builder

# Install Rust toolchain
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl build-essential pkg-config libssl-dev && \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && \
    rm -rf /var/lib/apt/lists/*
ENV PATH="/root/.cargo/bin:${PATH}"

RUN pip install --no-cache-dir maturin

WORKDIR /app
# Copy only what's needed for the build (see .dockerignore)
# pyproject.toml 必须复制：maturin 的 module-name=tdxrs._internal /
# python-source 配置全在其中。缺了它扩展以顶层 tdxrs 名安装，
# 而 PyO3 导出符号是 PyInit__internal，import tdxrs 必失败。
# README.md 是 wheel 元数据（Cargo.toml/pyproject 的 readme 字段），
# maturin build 打包时必读。
# benches/ 虽不参与 wheel，但 Cargo.toml 显式声明了 [[bench]] reader_bench，
# cargo 解析 manifest 需要该文件存在，缺了整个构建直接报错
COPY pyproject.toml README.md ./
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY python/ python/
COPY benches/ benches/

# Build the wheel, then install it into a venv.
# pip install from the built wheel (而非 maturin develop) 端到端验证
# 真实发布产物可安装可导入
RUN python -m venv .venv && \
    . .venv/bin/activate && \
    maturin build --release --out dist && \
    pip install --no-cache-dir dist/*.whl

# === Stage 2: Test image ===
FROM python:3.13-slim
COPY --from=builder /app/.venv /app/.venv
ENV PATH="/app/.venv/bin:${PATH}"
CMD ["python", "-c", "import tdxrs; print('tdxrs OK')"]