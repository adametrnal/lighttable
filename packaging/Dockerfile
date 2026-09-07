# Build only from scripts/package-release.py source output.
FROM ubuntu:24.04 AS build
ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl build-essential pkg-config git python3 python3-pil lua5.4 luajit \
    libx11-dev libxi-dev libxrandr-dev libxcursor-dev libxinerama-dev libegl1-mesa-dev \
    libgl1-mesa-dev libwayland-dev libxkbcommon-dev libxkbcommon-x11-0 liblcms2-dev \
    xvfb xauth libgl1-mesa-dri desktop-file-utils && rm -rf /var/lib/apt/lists/*
ENV PATH="/root/.cargo/bin:${PATH}"
ENV CARGO_BUILD_JOBS=4
WORKDIR /app
COPY rust-toolchain.toml ./
RUN curl --fail --location --proto '=https' --tlsv1.2 https://sh.rustup.rs -o /tmp/rustup-init.sh \
    && sh /tmp/rustup-init.sh -y --profile minimal --default-toolchain none \
    && rustup show
COPY . .
RUN --mount=type=cache,target=/app/target --mount=type=cache,target=/root/.cargo/registry \
    bash scripts/validate-release.sh /out \
    && python3 scripts/make-demo-catalog.py /tmp/runtime-demo

# Exercise the packaged executable without the compiler or development packages.
FROM ubuntu:24.04 AS runtime-check
ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    liblcms2-2 libegl1 libgl1 libgl1-mesa-dri libwayland-client0 libxkbcommon0 \
    libxkbcommon-x11-0 libx11-6 libxi6 libxrandr2 libxcursor1 libxinerama1 \
    xvfb xauth && rm -rf /var/lib/apt/lists/*
COPY --from=build /out/bundle /opt/lighttable
COPY --from=build /out/*.tar.gz /out/*.zip /out/SHA256SUMS /out/desktop-smoke.txt /out/synthetic-inspect.txt /release/
COPY --from=build /tmp/runtime-demo /tmp/runtime-demo
RUN mkdir -m 700 /tmp/runtime && \
    LIBGL_ALWAYS_SOFTWARE=1 WINIT_UNIX_BACKEND=x11 XDG_RUNTIME_DIR=/tmp/runtime \
    xvfb-run -a -s '-screen 0 1600x1000x24' /opt/lighttable/bin/lighttable \
    '/tmp/runtime-demo/Synthetic Demo.lrcat' --smoke-test /tmp/runtime-smoke \
    && test -f /tmp/runtime-smoke/success.txt \
    && cp /tmp/runtime-smoke/success.txt /release/runtime-smoke.txt \
    && cp /tmp/runtime-smoke/grid.png /release/runtime-grid.png \
    && cp /tmp/runtime-smoke/loupe.png /release/runtime-loupe.png \
    && cd /release && sha256sum --check SHA256SUMS

FROM scratch AS artifacts
COPY --from=runtime-check /release/ /
