//! Build script for `nrf-802154-sys`.
//!
//! The target description below is the single source of truth for both the C
//! compiler and bindgen. Keeping the defines and ABI flags in one place is
//! important: several public driver types contain C enums.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bindgen::callbacks::ParseCallbacks;

const DRIVER_ROOT: &str = "third_party/nordic/nrfxlib/nrf_802154";
const WRAPPER: &str = "include/wrapper.h";

#[derive(Clone, Copy, Debug)]
enum Series {
    Nrf52,
    Nrf53,
    Nrf54l,
    Nrf54lNs,
    Nrf54lm20AppS,
    Nrf54h,
}

impl Series {
    fn get() -> Self {
        let enabled = (
            cfg!(feature = "nrf52"),
            cfg!(feature = "nrf53"),
            cfg!(feature = "nrf54l-s"),
            cfg!(feature = "nrf54l-ns"),
            cfg!(feature = "nrf54lm20-app-s"),
            cfg!(feature = "nrf54h"),
        );

        match enabled {
            (true, false, false, false, false, false) => Self::Nrf52,
            (false, true, false, false, false, false) => Self::Nrf53,
            (false, false, true, false, false, false) => Self::Nrf54l,
            (false, false, false, true, false, false) => Self::Nrf54lNs,
            (false, false, false, false, true, false) => Self::Nrf54lm20AppS,
            (false, false, false, false, false, true) => Self::Nrf54h,
            _ => panic!(
                "exactly one nrf-802154-sys architecture feature must be enabled: \
                 nrf52, nrf53, nrf54l-s, nrf54l-ns, nrf54lm20-app-s, or nrf54h"
            ),
        }
    }
}

#[derive(Debug)]
struct TargetConfig {
    rust_target: String,
    cpu: &'static str,
    float_abi: &'static str,
    sl_directory: &'static str,
    defines: Vec<String>,
}

impl TargetConfig {
    fn new(series: Series, rust_target: String) -> Self {
        let (cpu, float_abi, sl_directory, chip, core, chip_core, cpu_mhz) =
            match (series, rust_target.as_str()) {
                (Series::Nrf52, "thumbv7em-none-eabihf") => (
                    "cortex-m4",
                    "hard",
                    "nrf52840",
                    "NRF52840_XXAA",
                    None,
                    None,
                    None,
                ),
                (Series::Nrf52, "thumbv7em-none-eabi") => (
                    "cortex-m4",
                    "soft",
                    "nrf52840",
                    "NRF52840_XXAA",
                    None,
                    None,
                    None,
                ),
                (Series::Nrf53, "thumbv8m.main-none-eabi") => (
                    "cortex-m33+nodsp",
                    "soft",
                    "nrf5340_cpunet",
                    "NRF5340_XXAA",
                    Some("NRF_NETWORK"),
                    Some("NRF5340_XXAA_NETWORK"),
                    None,
                ),
                (Series::Nrf54l, "thumbv8m.main-none-eabihf") => (
                    "cortex-m33",
                    "hard",
                    "nrf54l15_cpuapp",
                    "NRF54L15_XXAA",
                    Some("NRF_APPLICATION"),
                    None,
                    Some(128),
                ),
                (Series::Nrf54l, "thumbv8m.main-none-eabi") => (
                    "cortex-m33",
                    "soft",
                    "nrf54l15_cpuapp",
                    "NRF54L15_XXAA",
                    Some("NRF_APPLICATION"),
                    None,
                    Some(128),
                ),
                (Series::Nrf54lNs, "thumbv8m.main-none-eabihf") => (
                    "cortex-m33",
                    "hard",
                    "nrf54l15_cpuapp_ns",
                    "NRF54L15_XXAA",
                    Some("NRF_APPLICATION"),
                    None,
                    Some(128),
                ),
                (Series::Nrf54lNs, "thumbv8m.main-none-eabi") => (
                    "cortex-m33",
                    "soft",
                    "nrf54l15_cpuapp_ns",
                    "NRF54L15_XXAA",
                    Some("NRF_APPLICATION"),
                    None,
                    Some(128),
                ),
                // The product target is deliberately secure-only and hard-float-only.
                (Series::Nrf54lm20AppS, "thumbv8m.main-none-eabihf") => (
                    "cortex-m33",
                    "hard",
                    "nrf54lm20a_cpuapp",
                    "NRF54LM20A_XXAA",
                    Some("NRF_APPLICATION"),
                    None,
                    Some(128),
                ),
                (Series::Nrf54h, "thumbv8m.main-none-eabihf") => (
                    "cortex-m33",
                    "hard",
                    "nrf54h20_cpurad",
                    "NRF54H20_XXAA",
                    Some("NRF_RADIOCORE"),
                    None,
                    Some(128),
                ),
                (Series::Nrf54h, "thumbv8m.main-none-eabi") => (
                    "cortex-m33",
                    "soft",
                    "nrf54h20_cpurad",
                    "NRF54H20_XXAA",
                    Some("NRF_RADIOCORE"),
                    None,
                    Some(128),
                ),
                _ => panic!("unsupported Rust target {rust_target:?} for {series:?}"),
            };

        let egu = match series {
            Series::Nrf52 | Series::Nrf53 => "NRF_EGU0",
            Series::Nrf54l | Series::Nrf54lNs | Series::Nrf54lm20AppS => "NRF_EGU10",
            Series::Nrf54h => "NRF_EGU020",
        };

        // These definitions are consumed by both clang invocations. Requests
        // execute directly, while ordinary notifications are deferred to SWI.
        let mut defines = vec![
            chip.to_owned(),
            "CONFIG_MPSL".to_owned(),
            "NRF_802154_SERIALIZATION_HOST=0".to_owned(),
            "NRF_802154_INTERNAL_SWI_IRQ_HANDLING=0".to_owned(),
            "NRF_802154_REQUEST_IMPL=0".to_owned(),
            "NRF_802154_NOTIFICATION_IMPL=1".to_owned(),
            format!("NRF_802154_EGU_INSTANCE={egu}"),
        ];
        if let Some(core) = core {
            defines.push(core.to_owned());
        }
        if let Some(chip_core) = chip_core {
            defines.push(chip_core.to_owned());
        }
        if let Some(cpu_mhz) = cpu_mhz {
            defines.push(format!("NRF_CONFIG_CPU_FREQ_MHZ={cpu_mhz}"));
        }

        Self {
            rust_target,
            cpu,
            float_abi,
            sl_directory,
            defines,
        }
    }

    fn architecture_args(&self) -> Vec<String> {
        vec![
            format!("--target={}", self.rust_target),
            format!("-mcpu={}", self.cpu),
            "-mthumb".to_owned(),
            format!("-mfloat-abi={}", self.float_abi),
            "-fshort-enums".to_owned(),
        ]
    }

    fn define_args(&self) -> impl Iterator<Item = String> + '_ {
        self.defines.iter().map(|define| format!("-D{define}"))
    }

    fn compiler_args(&self, manifest_dir: &Path) -> Vec<String> {
        let mut args = self.architecture_args();
        args.extend(self.define_args());
        args.extend(include_directories(manifest_dir).map(|path| format!("-I{}", path.display())));
        args
    }

    fn sl_archive(&self, manifest_dir: &Path) -> PathBuf {
        manifest_dir
            .join(DRIVER_ROOT)
            .join("sl/sl/lib")
            .join(self.sl_directory)
            .join(format!("{}-float", self.float_abi))
            .join("libnrf-802154-sl.a")
    }
}

fn include_directories(manifest_dir: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    [
        "third_party/arm/CMSIS_5/CMSIS/Core/Include",
        "include",
        "third_party/nordic/nrfx",
        "third_party/nordic/nrfx/bsp/stable",
        "third_party/nordic/nrfx/bsp/stable/soc",
        "third_party/nordic/nrfx/bsp/stable/mdk",
        "third_party/nordic/nrfx/templates",
        "third_party/nordic/nrfxlib/mpsl/include",
        "third_party/nordic/nrfxlib/mpsl/fem/include",
        "third_party/nordic/nrfxlib/nrf_802154/common/include",
        "third_party/nordic/nrfxlib/nrf_802154/driver/include",
        "third_party/nordic/nrfxlib/nrf_802154/sl/include",
        "third_party/nordic/nrfxlib/nrf_802154/sl/sl/include",
    ]
    .into_iter()
    .map(|path| manifest_dir.join(path))
}

#[derive(Debug)]
struct DoxygenCallbacks;

impl ParseCallbacks for DoxygenCallbacks {
    fn process_comment(&self, comment: &str) -> Option<String> {
        Some(doxygen_rs::transform(
            &comment.replace('[', "\\[").replace("@sa @ref", "@ref"),
        ))
    }
}

fn generate_bindings(config: &TargetConfig, manifest_dir: &Path, out_dir: &Path) {
    let mut builder = bindgen::Builder::default()
        .use_core()
        .size_t_is_usize(true)
        .header(manifest_dir.join(WRAPPER).to_string_lossy())
        .allowlist_function("nrf_802154_.*")
        .allowlist_type("nrf_802154_.*")
        .allowlist_var("NRF_802154_.*")
        .allowlist_type("nrf_radio_cca_.*")
        .allowlist_var("NRF_RADIO_CCA_.*")
        .blocklist_var("NRF_.*_BASE.*")
        .prepend_enum_name(false)
        .parse_callbacks(Box::new(DoxygenCallbacks));

    for arg in config.compiler_args(manifest_dir) {
        builder = builder.clang_arg(arg);
    }

    let bindings = builder.generate().unwrap_or_else(|error| {
        panic!(
            "failed to generate nrf_802154 bindings ({error}); install Clang and libclang, \
             and set LIBCLANG_PATH if libclang is not discoverable"
        )
    });
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("failed to write generated nrf_802154 bindings to OUT_DIR");
}

fn compile_driver(
    config: &TargetConfig,
    manifest_dir: &Path,
    clang: &Path,
    llvm_ar: &Path,
) -> PathBuf {
    let flags = config.compiler_args(manifest_dir).join(" ");

    // `cmake` asks the `cc` crate for C, C++, and assembler tools even though
    // this standalone project enables C only. Point all probes at Clang so a
    // GNU Arm toolchain is neither required nor spuriously warned about.
    env::set_var("CC", clang);
    env::set_var("CXX", clang);

    cmake::Config::new(manifest_dir.join("cmake"))
        // Build-script flags and tool paths are part of the ABI contract; do
        // not retain a stale CMake cache after a crate or feature update.
        .always_configure(true)
        .define("NRF_802154_SOURCE_DIR", manifest_dir.join(DRIVER_ROOT))
        .define("CMAKE_BUILD_TYPE", "MinSizeRel")
        .define(
            "CMAKE_C_FLAGS",
            format!("-Werror=implicit-function-declaration {flags}"),
        )
        .define("CMAKE_C_FLAGS_MINSIZEREL", "-Os -DNDEBUG")
        .define("CMAKE_SYSTEM_NAME", "Generic")
        .define("CMAKE_SYSTEM_PROCESSOR", "ARM")
        .define("CMAKE_TRY_COMPILE_TARGET_TYPE", "STATIC_LIBRARY")
        .define("CMAKE_C_COMPILER", clang)
        .define("CMAKE_C_COMPILER_TARGET", &config.rust_target)
        // In particular, Apple's BSD ar silently produces an empty archive from
        // ELF objects. Always use LLVM's target-independent archive tools.
        .define("NRF_LLVM_AR", llvm_ar)
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("BUILD_TESTING", "OFF")
        .build_target("nrf-802154-driver")
        .build()
}

fn llvm_tool(name: &str) -> PathBuf {
    let env_name = name.replace('-', "_").to_ascii_uppercase();
    if let Some(path) = env::var_os(&env_name).map(PathBuf::from) {
        ensure_file(&path, &format!("LLVM tool selected by {env_name}"));
        return path;
    }

    // llvm-tools-preview ships the archive tools with the active Rust
    // toolchain, which makes them available on macOS as well as Linux.
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let sysroot = Command::new(rustc)
        .args(["--print", "sysroot"])
        .output()
        .expect("failed to query the Rust sysroot while locating LLVM tools");
    if sysroot.status.success() {
        let host = env::var("HOST").expect("Cargo did not provide HOST to the build script");
        let path = PathBuf::from(String::from_utf8_lossy(&sysroot.stdout).trim())
            .join("lib/rustlib")
            .join(host)
            .join("bin")
            .join(name);
        if path.is_file() {
            return path;
        }
    }

    panic!(
        "required LLVM tool `{name}` was not found; install Rust's llvm-tools-preview component \
         or set {env_name} to its absolute path"
    );
}

fn ensure_tool(tool: &Path) {
    let available = Command::new(tool)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !available {
        panic!(
            "required build tool `{}` was not found; install Clang, libclang, and CMake \
             and ensure their executables are on PATH",
            tool.display()
        );
    }
}

fn ensure_file(path: &Path, description: &str) {
    if !path.is_file() {
        panic!(
            "missing {description} at {}; initialize the repository's git submodules with \
             `git submodule update --init --recursive`",
            path.display()
        );
    }
}

fn emit_rerun_mdk(path: &Path) {
    let prefixes = [
        "nrf52840",
        "nrf5340_network",
        "nrf54l15",
        "nrf54lm20a",
        "nrf54h20",
        "system_nrf",
    ];
    let exact = [
        "nrf.h",
        "nrf_mem.h",
        "nrf_peripherals.h",
        "nrf_erratas.h",
        "compiler_abstraction.h",
        "haltium_interim.h",
    ];
    for entry in fs::read_dir(path)
        .unwrap_or_else(|error| panic!("failed to inspect {}: {error}", path.display()))
    {
        let path = entry.expect("failed to inspect MDK entry").path();
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        if exact.contains(&name)
            || name.ends_with("_erratas.h")
            || prefixes.iter().any(|prefix| name.starts_with(prefix))
        {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

fn emit_rerun_tree(path: &Path) {
    let entries = fs::read_dir(path)
        .unwrap_or_else(|error| panic!("failed to inspect {}: {error}", path.display()));
    for entry in entries {
        let path = entry
            .expect("failed to inspect vendored source entry")
            .path();
        if path.is_dir() {
            emit_rerun_tree(&path);
        } else if matches!(
            path.extension().and_then(OsStr::to_str),
            Some("c" | "h" | "a" | "txt")
        ) || path.file_name() == Some(OsStr::new("CMakeLists.txt"))
        {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=LIBCLANG_PATH");
    println!("cargo:rerun-if-env-changed=CLANG_PATH");
    println!("cargo:rerun-if-env-changed=LLVM_AR");

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let config = TargetConfig::new(Series::get(), env::var("TARGET").unwrap());
    let archive = config.sl_archive(&manifest_dir);

    let clang = env::var_os("CLANG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("clang"));
    ensure_tool(&clang);
    ensure_tool(Path::new("cmake"));
    let llvm_ar = llvm_tool("llvm-ar");
    ensure_file(&manifest_dir.join(WRAPPER), "bindgen wrapper header");
    ensure_file(
        &manifest_dir
            .join(DRIVER_ROOT)
            .join("driver/src/nrf_802154.c"),
        "Nordic nrf_802154 driver source",
    );
    ensure_file(
        &manifest_dir.join("third_party/nordic/nrfx/bsp/stable/mdk/nrf.h"),
        "Nordic nrfx MDK headers",
    );
    ensure_file(&archive, "target-specific nrf_802154 service-layer archive");

    emit_rerun_tree(&manifest_dir.join("include"));
    emit_rerun_tree(&manifest_dir.join("cmake"));
    emit_rerun_tree(&manifest_dir.join("third_party/arm/CMSIS_5/CMSIS/Core/Include"));
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir
            .join("third_party/nordic/nrfx/nrfx.h")
            .display()
    );
    emit_rerun_tree(&manifest_dir.join("third_party/nordic/nrfx/drivers"));
    emit_rerun_tree(&manifest_dir.join("third_party/nordic/nrfx/hal"));
    emit_rerun_tree(&manifest_dir.join("third_party/nordic/nrfx/lib"));
    emit_rerun_tree(&manifest_dir.join("third_party/nordic/nrfx/templates"));
    emit_rerun_tree(&manifest_dir.join("third_party/nordic/nrfx/bsp/stable/soc"));
    emit_rerun_mdk(&manifest_dir.join("third_party/nordic/nrfx/bsp/stable/mdk"));
    emit_rerun_tree(&manifest_dir.join("third_party/nordic/nrfxlib/mpsl/include"));
    emit_rerun_tree(&manifest_dir.join("third_party/nordic/nrfxlib/mpsl/fem/include"));
    emit_rerun_tree(&manifest_dir.join(DRIVER_ROOT).join("common"));
    emit_rerun_tree(&manifest_dir.join(DRIVER_ROOT).join("driver"));
    emit_rerun_tree(&manifest_dir.join(DRIVER_ROOT).join("sl/include"));
    emit_rerun_tree(&manifest_dir.join(DRIVER_ROOT).join("sl/sl/include"));
    println!("cargo:rerun-if-changed={}", archive.display());

    // CMake bakes the host archiver's absolute path into link.txt. Reusing a
    // cache created by an older crate version can therefore silently recreate
    // empty archives on macOS even after CMAKE_AR changes.
    let cmake_build_dir = out_dir.join("build");
    if cmake_build_dir.exists() {
        fs::remove_dir_all(&cmake_build_dir)
            .expect("failed to remove the stale nrf_802154 CMake build directory");
    }

    let cmake_out = compile_driver(&config, &manifest_dir, &clang, &llvm_ar);
    generate_bindings(&config, &manifest_dir, &out_dir);

    let sl_dir = archive.parent().unwrap();
    let build_dir = cmake_out.join("build");
    println!("cargo:rustc-link-search=native={}", sl_dir.display());
    println!(
        "cargo:rustc-link-search=native={}",
        build_dir.join("driver").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        build_dir.join("common").display()
    );
    println!("cargo:rustc-link-lib=static=nrf-802154-driver");
    println!("cargo:rustc-link-lib=static=nrf-802154-common");
    println!("cargo:rustc-link-lib=static=nrf-802154-sl");
}
