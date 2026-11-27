FROM kraken/tentacle-base
USER root
ARG DEBIAN_FRONTEND=noninteractive
# build-essential and pkg-config: crates with C parts; jq: kraken and agent scripts; libicu: .NET aborts
# without it; tzdata-legacy: the old IANA zone links the .NET sidecar maps Windows zone ids to (CI installs
# it when missing, .github/actions/setup/action.yml); time: GNU `/usr/bin/time -v` for peak memory; python3,
# zip, unzip: al-runner tests and tools call python3 and zip (al-test-cli's corpus_bump_*, record_windows_oracle and
# step_time_guard failed without them). gcc-mingw-w64-x86-64: the linker and C compiler for the Windows GNU
# target below, so Windows-only code (console handlers, kernel32 calls) is checked here before a PR leaves draft.
# libharfbuzz0b, libfontconfig1, fontconfig and the DejaVu and Liberation fonts: the report renderer's probe
# (tools/dotnet-sidecar/Render.cs) needs HarfBuzz, a loadable SkiaSharp (fontconfig) and a font; without them the
# Word-layout report cases keep their placeholder Html and --baseline-diff shows them as false regressions.
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config jq libicu-dev tzdata-legacy time ca-certificates curl python3 zip unzip \
      gcc-mingw-w64-x86-64 \
      libharfbuzz0b libfontconfig1 fontconfig fonts-dejavu-core fonts-liberation \
    && rm -rf /var/lib/apt/lists/*
# Rust as CI pins it (dtolnay/rust-toolchain@1.98.1 in .github/actions/setup/action.yml), outside the cache
# volume (the base points CARGO_HOME at ~/.cache for the registry).
ENV RUSTUP_HOME=/opt/rustup
RUN curl -fsSL https://sh.rustup.rs | CARGO_HOME=/opt/cargo sh -s -- -y --profile minimal \
      --default-toolchain 1.98.1 --component rustfmt,clippy,rust-analyzer --no-modify-path
ENV PATH=/opt/cargo/bin:$PATH
# The Windows GNU target: `cargo check`/`clippy --target x86_64-pc-windows-gnu` of Windows-only code, which CI's
# portable check covers only once a PR leaves draft.
RUN rustup target add x86_64-pc-windows-gnu
# .NET 8 SDK and runtime: the DotNet sidecar (ADR-014, tools/dotnet-sidecar targets net8.0). The .NET 10 runtime
# next to it: BC 29's report renderer and Company-Initialize run on .NET 10 (without it BC 29 skips them).
RUN curl -fsSL https://dot.net/v1/dotnet-install.sh -o /tmp/dotnet-install.sh \
    && bash /tmp/dotnet-install.sh --channel 8.0 --install-dir /opt/dotnet \
    && bash /tmp/dotnet-install.sh --channel 10.0 --runtime dotnet --install-dir /opt/dotnet \
    && rm /tmp/dotnet-install.sh
ENV DOTNET_ROOT=/opt/dotnet PATH=/opt/dotnet:$PATH DOTNET_CLI_TELEMETRY_OPTOUT=1
# Login shells (bash -l: kraken's setup, ssh sessions) rebuild PATH from /etc/profile, so ENV alone is not enough
RUN printf '%s\n' 'export PATH=/opt/cargo/bin:/opt/dotnet:$PATH' 'export DOTNET_ROOT=/opt/dotnet' \
      'export RUSTUP_HOME=/opt/rustup' 'export DOTNET_CLI_TELEMETRY_OPTOUT=1' > /etc/profile.d/al-runner-tools.sh \
    && chown -R dev:dev /opt/rustup /opt/cargo
# Profiling for the performance work: samply (sampling CPU profiler) and linux-perf (`perf stat -e
# instructions:u`, stable where CPU seconds swing on a shared host). Both need perf_event_open: cap_add
# PERFMON in project.yaml and the host's kernel.perf_event_paranoid <= 1. gdb: stacks of a hung process
# (needs the host's kernel.yama.ptrace_scope 0). Pinned samply; the same layers as alsem's image.
RUN apt-get update && apt-get install -y --no-install-recommends linux-perf gdb \
    && rm -rf /var/lib/apt/lists/* && perf --version && gdb --version | head -n 1
RUN CARGO_HOME=/opt/cargo cargo install samply --version 0.13.1 --locked \
    && rm -rf /opt/cargo/registry /opt/cargo/git && chown -R dev:dev /opt/cargo && samply --version
# ends as root: the base entrypoint drops to dev
