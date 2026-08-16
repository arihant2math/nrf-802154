[![CI](https://github.com/sysgrok/nrf-802154/actions/workflows/ci.yml/badge.svg)](https://github.com/sysgrok/nrf-802154/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/nrf-802154-sys.svg)](https://crates.io/crates/nrf-802154-sys)

# nrf-802154-sys

Raw, `no_std` bindings to Nordic Semiconductor's [802.15.4 Radio Driver](https://docs.nordicsemi.com/bundle/ncs-latest/page/nrfxlib/nrf_802154/README.html).
Bindings are generated into `OUT_DIR`; this crate exports only raw FFI.
Applications should use the safe `nrf-802154` crate instead.

## nRF54LM20A application core

Enable `nrf54lm20-app-s` and build for the secure hard-float application core:

```sh
cargo build -p nrf-802154-sys \
  --no-default-features --features nrf54lm20-app-s \
  --target thumbv8m.main-none-eabihf
```

Exactly one architecture feature must be enabled. The LM20A feature is valid
only with `thumbv8m.main-none-eabihf`.

The standalone C build requires `clang`, discoverable `libclang`, CMake, and
`llvm-ar` (the checked-in toolchain supplies it through `llvm-tools-preview`).
A source checkout also requires initialized recursive git submodules. Missing
tools, driver sources, MDK headers, and target service-layer archives are
reported by the build script before compilation.

The LM20A build uses:

- nrfx 4.3.0 final-silicon `NRF54LM20A_XXAA` MDK headers;
- nrfxlib 3.3.0 driver sources and
  `nrf54lm20a_cpuapp/hard-float/libnrf-802154-sl.a`;
- Cortex-M33 hard-float ABI at 128 MHz;
- direct requests and externally-routed SWI notifications.

The Nordic source, binaries, and license notices are included from the pinned
submodules and remain subject to Nordic's respective license terms.
