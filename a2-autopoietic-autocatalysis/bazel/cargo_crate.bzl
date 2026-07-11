"""Small rules_rust macros preserving Cargo.toml as dependency authority."""

load("@crates//:defs.bzl", "aliases", "all_crate_deps", "crate_edition")
load("@rules_rust//cargo:defs.bzl", "cargo_toml_env_vars")
load("@rules_rust//rust:defs.bzl", "rust_binary", "rust_library", "rust_test")


def cargo_manifest_env():
    cargo_toml_env_vars(
        name = "cargo_env",
        src = "Cargo.toml",
        workspace = "//:Cargo.toml",
    )


def cargo_rust_library(name, internal_deps = [], test_internal_deps = []):
    rust_library(
        name = name,
        aliases = aliases(),
        crate_name = name,
        crate_root = "src/lib.rs",
        deps = all_crate_deps(normal = True) + internal_deps,
        edition = crate_edition(),
        proc_macro_deps = all_crate_deps(proc_macro = True),
        rustc_env_files = [":cargo_env"],
        rustc_flags = ["-Funsafe-code"],
        srcs = native.glob(["src/**/*.rs"]),
        tags = [
            "cargo-package={}".format(name),
            "cargo-target=lib:{}".format(name),
        ],
        version = "0.1.0",
        visibility = ["//visibility:public"],
    )

    rust_test(
        name = "{}_test".format(name),
        aliases = aliases(
            normal_dev = True,
            proc_macro_dev = True,
        ),
        crate = ":{}".format(name),
        deps = all_crate_deps(normal_dev = True) + test_internal_deps,
        edition = crate_edition(),
        proc_macro_deps = all_crate_deps(proc_macro_dev = True),
        rustc_env_files = [":cargo_env"],
        rustc_flags = ["-Funsafe-code"],
        tags = [
            "cargo-package={}".format(name),
            "cargo-test=unit:lib:{}".format(name),
        ],
    )


def cargo_rust_binary(
        package,
        name,
        crate_name = None,
        internal_deps = [],
        test_internal_deps = []):
    rust_binary(
        name = name,
        aliases = aliases(),
        crate_name = crate_name or package,
        crate_root = "src/main.rs",
        deps = all_crate_deps(normal = True) + internal_deps,
        edition = crate_edition(),
        proc_macro_deps = all_crate_deps(proc_macro = True),
        rustc_env_files = [":cargo_env"],
        rustc_flags = ["-Funsafe-code"],
        srcs = native.glob(["src/**/*.rs"]),
        tags = [
            "cargo-package={}".format(package),
            "cargo-target=bin:{}".format(crate_name or package),
        ],
        version = "0.1.0",
        visibility = ["//visibility:public"],
    )

    rust_test(
        name = "{}_test".format(name),
        aliases = aliases(
            normal_dev = True,
            proc_macro_dev = True,
        ),
        crate = ":{}".format(name),
        deps = all_crate_deps(normal_dev = True) + test_internal_deps,
        edition = crate_edition(),
        proc_macro_deps = all_crate_deps(proc_macro_dev = True),
        rustc_env_files = [":cargo_env"],
        rustc_flags = ["-Funsafe-code"],
        tags = [
            "cargo-package={}".format(package),
            "cargo-test=unit:bin:{}".format(crate_name or package),
        ],
    )


def cargo_rust_integration_test(package, name, internal_deps = []):
    rust_test(
        name = "{}_test".format(name),
        aliases = aliases(
            normal = True,
            normal_dev = True,
            proc_macro = True,
            proc_macro_dev = True,
        ),
        crate_name = name,
        crate_root = "tests/{}.rs".format(name),
        deps = all_crate_deps(
            normal = True,
            normal_dev = True,
        ) + internal_deps,
        edition = crate_edition(),
        proc_macro_deps = all_crate_deps(
            proc_macro = True,
            proc_macro_dev = True,
        ),
        rustc_env_files = [":cargo_env"],
        rustc_flags = ["-Funsafe-code"],
        srcs = ["tests/{}.rs".format(name)],
        tags = [
            "cargo-package={}".format(package),
            "cargo-test=integration:{}".format(name),
        ],
    )
