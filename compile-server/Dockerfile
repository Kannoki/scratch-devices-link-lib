# ==============================================================================
# Stage 1: Build the Rust binary
# ==============================================================================
FROM rust:1.80-bookworm AS builder

WORKDIR /usr/src/compile-server
COPY Cargo.toml ./
# Create dummy src for caching dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release || true

# Copy actual source code and build
COPY src ./src
RUN touch src/main.rs && cargo build --release

# ==============================================================================
# Stage 2: Production runtime with arduino-cli and ESP32 toolchain
# ==============================================================================
FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive
ENV COMPILE_SERVER_PORT=8765
ENV COMPILE_SERVER_HOST=0.0.0.0
ENV ARDUINO_CLI_PATH=/usr/local/bin/arduino-cli
ENV ARDUINO_DATA_DIR=/root/.arduino15
ENV MAX_CONCURRENT_JOBS=4
ENV JOB_TIMEOUT_SECS=180

# Install dependencies (Python 3 is required by esp32 tools / esptool)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    python3 \
    python3-pip \
    python3-serial \
    && rm -rf /var/lib/apt/lists/*

# Install arduino-cli
RUN curl -fsSL https://raw.githubusercontent.com/arduino/arduino-cli/master/install.sh | BINDIR=/usr/local/bin sh

# Initialize arduino-cli configuration and install ESP32 core
RUN arduino-cli config init \
    && arduino-cli config add board_manager.additional_urls https://espressif.github.io/arduino-esp32/package_esp32_index.json \
    && arduino-cli core update-index \
    && arduino-cli core install esp32:esp32 \
    && rm -rf /root/.arduino15/staging/*

# Copy compiled binary from builder
COPY --from=builder /usr/src/compile-server/target/release/windify-compile-server /usr/local/bin/windify-compile-server

EXPOSE 8765

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8765/status || exit 1

CMD ["windify-compile-server"]
