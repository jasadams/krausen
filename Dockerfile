# Build the SSR server and hydrated frontend in the same directory. Leptos
# server-function identifiers must agree between the two compilation targets.
FROM node:22-bookworm@sha256:94c973596a8581a87ba42746ceb0b0e0b5246d098de45b1ce04dcc967cfc5dfb AS node
FROM rust:1.98-bookworm@sha256:c49256cbe5ea0188bc658a689500d70c41eb51f009a7a7be209caf60a944f3ec AS builder
ARG CARGO_BUILD_JOBS=4
ARG VERIFY_BUILD_LIMITS=0
ARG LIMIT_CHECK_NONCE=0
ENV CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS}
# Coordinated local builds can fail closed before compilation if limits vanish.
RUN if [ "$VERIFY_BUILD_LIMITS" = 1 ]; then \
      test -n "$LIMIT_CHECK_NONCE" \
      && \
      p="$(cut -d: -f3 /proc/self/cgroup)" \
      && test "$(cat "/sys/fs/cgroup${p}/cpu.max")" = '400000 100000' \
      && test "$(cat "/sys/fs/cgroup${p}/memory.max")" = '8589934592' \
      && printf 'build cgroup limits verified: cpu=4 memory=8GiB\n'; \
    fi
COPY --from=node /usr/local/bin/node /usr/local/bin/node
COPY --from=node /usr/local/lib/node_modules /usr/local/lib/node_modules
RUN ln -s /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl pkg-config libssl-dev clang \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown
RUN curl -fsSL -o /tmp/trunk.tar.gz https://github.com/trunk-rs/trunk/releases/download/v0.21.14/trunk-x86_64-unknown-linux-gnu.tar.gz \
    && echo 'f2b4680cd239693a646a2795e4633c625328d7b2a044fbe749fa3a2fe9e7036b  /tmp/trunk.tar.gz' | sha256sum -c - \
    && tar -xzf /tmp/trunk.tar.gz -C /usr/local/bin trunk
RUN curl -fsSL -o /tmp/wasm-bindgen.tar.gz https://github.com/wasm-bindgen/wasm-bindgen/releases/download/0.2.120/wasm-bindgen-0.2.120-x86_64-unknown-linux-musl.tar.gz \
    && echo 'ceb6f0b9fea9d2868f258215a1d7f86a4c28330a0d64432cc7d53674618772d6  /tmp/wasm-bindgen.tar.gz' | sha256sum -c - \
    && tar -xzf /tmp/wasm-bindgen.tar.gz --strip-components=1 -C /usr/local/bin wasm-bindgen-0.2.120-x86_64-unknown-linux-musl/wasm-bindgen
# Trunk's default wasm-opt invocation omits features emitted by current Rust.
# Use a real, pinned optimizer with the same feature flags as the Kyomi release.
RUN curl -fsSL -o /tmp/binaryen.tar.gz https://github.com/WebAssembly/binaryen/releases/download/version_128/binaryen-version_128-x86_64-linux.tar.gz \
    && echo '4ce79586d1c4762502eebe9a1db071fa5e446ef8897f2f766eb1cce5ec6dee9e  /tmp/binaryen.tar.gz' | sha256sum -c - \
    && tar -xzf /tmp/binaryen.tar.gz -C /tmp \
    && mv /tmp/binaryen-version_128/bin/wasm-opt /usr/local/bin/wasm-opt-real \
    && mkdir -p /root/.cache/trunk/wasm-opt-krausen/bin \
    && printf '#!/bin/sh\nexec /usr/local/bin/wasm-opt-real --enable-bulk-memory-opt --enable-nontrapping-float-to-int "$@"\n' > /root/.cache/trunk/wasm-opt-krausen/bin/wasm-opt \
    && chmod +x /root/.cache/trunk/wasm-opt-krausen/bin/wasm-opt
ENV TRUNK_TOOLS_WASM_OPT=krausen
WORKDIR /build
COPY . .
RUN npm ci --prefix crates/krausen-ui --ignore-scripts --no-audit --no-fund
ENV PATH="/build/crates/krausen-ui/node_modules/.bin:${PATH}"
RUN sed -i '/^cargo_profile = /d' crates/krausen-ui/Trunk.toml
RUN cd crates/krausen-ui && trunk build --release --locked
RUN cargo build --locked --release -p krausen-server --bin krausen

FROM debian:bookworm-slim@sha256:a3bba7dc7dbcc1f37c2856c51fc9a7fac96608f50cb87b1b4e4d8b8c85ae988e AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 1000 --no-create-home --shell /usr/sbin/nologin krausen
WORKDIR /app
COPY --from=builder /build/target/release/krausen /app/krausen
COPY --from=builder /build/crates/krausen-ui/dist /app/dist
ENV PORT=8003 TRUNK_DIST_DIR=/app/dist HOME=/tmp TMPDIR=/tmp
EXPOSE 8003
USER 1000:1000
ENTRYPOINT ["/app/krausen"]
