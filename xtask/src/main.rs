//! `cargo xtask <command>` — build, bundle, sign, install and check Navori.
//!
//! Profiles: `dev` → "Navori Dev.app" (`dev.navori.app.debug`; debug by default,
//!           release when the perf gate bundles it);
//!           `release` → "Navori.app" (`dev.navori.app`, release build).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand, ValueEnum};

mod perf;
mod release;
mod soak;

const SIGNING_IDENTITY: &str = "Navori Local Signing";
const MIN_MACOS: &str = "26.0";

#[derive(Debug, Parser)]
#[command(name = "xtask")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Build the Rust core as a static library + Swift bindings + XCFramework.
    BuildCore {
        #[arg(long, value_enum, default_value_t = Profile::Dev)]
        profile: Profile,
    },
    /// Build everything and assemble a signed .app under build/<profile>/.
    Bundle {
        #[arg(long, value_enum, default_value_t = Profile::Dev)]
        profile: Profile,
    },
    /// Bundle the dev app and (re)launch it.
    Run,
    /// Bundle the release app, install it to /Applications (keeping the old one as .previous) and launch it.
    Install,
    /// One-time: create the self-signed code-signing identity "Navori Local Signing".
    SigningSetup,
    /// Enforce the architecture's crate dependency rules.
    CheckDeps,
    /// Prove `unsafe_code` and `clippy::undocumented_unsafe_blocks` fail the build.
    LintProbe,
    /// Compare workspace dependency versions with the latest stable releases on crates.io.
    Outdated,
    /// Regenerate the checked-in app icons (`xtask/assets/*.icns`) from `xtask/assets/make-icon.swift`.
    Icon,
    /// Rust tests + clippy + fmt check + dependency rules + Swift build.
    Ci,
    /// Measure release code in the dev app and judge it against the budget.
    Perf {
        /// Write perf-baseline.json when every metric passes.
        #[arg(long)]
        record: bool,
    },
    /// Check a release tag against the workspace version, or build the release artifacts.
    Release {
        /// The release tag, e.g. v0.1.0.
        #[arg(long)]
        tag: String,
        /// Directory to write the release artifacts to.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Only check the tag and print version and pre-release status.
        #[arg(long)]
        check: bool,
    },
    /// Gate a release locally (tag free, tree clean, ci, perf, soak gate), then print the tag to push.
    ReleaseCheck,
    /// Soak an isolated Navori under load and judge footprint, threads, fds and hangs.
    Soak {
        /// Run the two-hour tier, which reports and always exits 0.
        #[arg(long)]
        long: bool,
        /// With --long, drive the load for this long instead (90s, 20m, 2h).
        #[arg(long = "for", requires = "long", value_parser = parse_load_duration)]
        for_: Option<Duration>,
        #[command(subcommand)]
        command: Option<SoakCmd>,
    },
    #[command(hide = true)]
    SoakListen {
        #[arg(long)]
        secs: u64,
    },
}

#[derive(Debug, Subcommand)]
enum SoakCmd {
    /// Print footprint, threads and fds of the installed Navori, once or every interval.
    Observe {
        #[arg(long, value_parser = parse_duration)]
        every: Option<Duration>,
    },
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let refuse =
        || format!("{s:?} is not a duration; write a number and a unit, like 90s, 20m or 2h");
    let split = s.len().checked_sub(1).ok_or_else(refuse)?;
    if !s.is_char_boundary(split) {
        return Err(refuse());
    }
    let (digits, unit) = s.split_at(split);
    let n: u64 = digits.parse().map_err(|_| refuse())?;
    if n == 0 {
        return Err(refuse());
    }
    let per = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        _ => return Err(refuse()),
    };
    n.checked_mul(per)
        .map(Duration::from_secs)
        .ok_or_else(refuse)
}

const MAX_SOAK_LOAD: Duration = Duration::from_secs(24 * 3600);

fn parse_load_duration(s: &str) -> Result<Duration, String> {
    let load = parse_duration(s)?;
    if load > MAX_SOAK_LOAD {
        return Err(format!(
            "{s:?} is longer than the 24h soak limit; use 24h or less"
        ));
    }
    Ok(load)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flavor {
    Debug,
    Release,
}

impl Flavor {
    fn cargo_dir(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
    fn swift_config(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Profile {
    Dev,
    Release,
}

impl Profile {
    fn default_flavor(self) -> Flavor {
        match self {
            Self::Dev => Flavor::Debug,
            Self::Release => Flavor::Release,
        }
    }
    fn app_name(self) -> &'static str {
        match self {
            Self::Dev => "Navori Dev",
            Self::Release => "Navori",
        }
    }
    fn icon_file(self) -> &'static str {
        match self {
            Self::Dev => "AppIconDev.icns",
            Self::Release => "AppIcon.icns",
        }
    }
    fn bundle_id(self) -> &'static str {
        match self {
            Self::Dev => "dev.navori.app.debug",
            Self::Release => "dev.navori.app",
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Release => "release",
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = workspace_root()?;
    match cli.command {
        Cmd::BuildCore { profile } => build_core(&root, profile.default_flavor()),
        Cmd::Bundle { profile } => {
            let out_dir = root.join("build").join(profile.name());
            bundle(&root, profile, profile.default_flavor(), &out_dir)
                .map(|app| println!("✓ {}", app.display()))
        }
        Cmd::Run => run_dev(&root),
        Cmd::Install => install(&root),
        Cmd::SigningSetup => signing_setup(&root),
        Cmd::CheckDeps => check_deps(&root),
        Cmd::LintProbe => lint_probe(&root),
        Cmd::Outdated => outdated(&root),
        Cmd::Icon => icon(&root),
        Cmd::Ci => ci(&root),
        Cmd::Perf { record } => {
            if !perf::run(&root, record)? {
                bail!("perf gate failed");
            }
            Ok(())
        }
        Cmd::Release { tag, out, check } => release::run(&root, &tag, out.as_deref(), check),
        Cmd::ReleaseCheck => release::check(&root),
        Cmd::Soak {
            long,
            for_,
            command,
        } => match command {
            Some(SoakCmd::Observe { every }) => soak::observe::observe(&root, every),
            None => {
                let tier = if long {
                    soak::plan::Tier::Long {
                        load: for_.unwrap_or(Duration::from_secs(7200)),
                    }
                } else {
                    soak::plan::Tier::Short
                };
                if !soak::run(&root, tier)? {
                    bail!("soak gate failed");
                }
                Ok(())
            }
        },
        Cmd::SoakListen { secs } => soak::listen(secs),
    }
}

fn workspace_root() -> Result<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map(Path::to_path_buf)
        .context("xtask must live one level below the workspace root")
}

/// Stdio for tool child stdout. Piped so `cargo xtask perf` can keep report
/// stdout free of xcodebuild/swift progress (first line must be `build …s`).
fn tool_stdout() -> Stdio {
    Stdio::piped()
}

/// Runs a command, echoing it, failing on non-zero exit.
/// Child stdout is forwarded to stderr so perf report stdout stays clean.
fn sh(cmd: &mut Command) -> Result<()> {
    let rendered = render(cmd);
    eprintln!("$ {rendered}");
    let mut child = cmd
        .stdout(tool_stdout())
        .spawn()
        .with_context(|| format!("cannot run {rendered}"))?;
    let mut pipe = child
        .stdout
        .take()
        .with_context(|| format!("{rendered} has no stdout"))?;
    let pump = thread::spawn(move || {
        let mut sink = std::io::stderr().lock();
        std::io::copy(&mut pipe, &mut sink)
    });
    let status = child
        .wait()
        .with_context(|| format!("wait for {rendered}"))?;
    pump.join()
        .map_err(|_| anyhow::anyhow!("tool stdout forwarder panicked"))?
        .context("forward tool stdout to stderr")?;
    ensure!(status.success(), "command failed ({status}): {rendered}");
    Ok(())
}

fn output(cmd: &mut Command) -> Result<String> {
    let out = cmd
        .output()
        .with_context(|| format!("cannot run {}", render(cmd)))?;
    ensure!(
        out.status.success(),
        "command failed: {}\n{}",
        render(cmd),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn render(cmd: &Command) -> String {
    let mut s = cmd.get_program().to_string_lossy().into_owned();
    for a in cmd.get_args() {
        let a = a.to_string_lossy();
        if a.contains(' ') {
            s.push_str(&format!(" '{a}'"));
        } else {
            s.push(' ');
            s.push_str(&a);
        }
    }
    s
}

fn cargo(root: &Path) -> Command {
    let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    c.current_dir(root);
    c
}

// ── build-core ──────────────────────────────────────────────────────────────────────────

fn build_core(root: &Path, flavor: Flavor) -> Result<()> {
    let mut build = cargo(root);
    build.args(["build", "-p", "navori-ffi", "--lib"]);
    if flavor == Flavor::Release {
        build.arg("--release");
    }
    sh(&mut build)?;
    sh(cargo(root).args([
        "build",
        "-q",
        "-p",
        "navori-ffi",
        "--features",
        "bindgen",
        "--bin",
        "uniffi-bindgen-swift",
    ]))?;

    let lib = root
        .join("target")
        .join(flavor.cargo_dir())
        .join("libnavori_ffi.a");
    let bindgen = root.join("target/debug/uniffi-bindgen-swift");
    let staging = root.join("build/bindings");
    let headers = staging.join("headers");
    let _ = std::fs::remove_dir_all(&staging);

    let bindgen_run = |args: &[&str], out: &Path| -> Result<()> {
        let mut c = Command::new(&bindgen);
        c.args(args).arg(&lib).arg(out);
        sh(&mut c)
    };
    bindgen_run(&["--swift-sources"], &staging.join("swift"))?;
    bindgen_run(&["--headers"], &headers)?;
    bindgen_run(
        &[
            "--modulemap",
            "--module-name",
            "navori_ffiFFI",
            "--modulemap-filename",
            "module.modulemap",
        ],
        &headers,
    )?;

    let generated = root.join("apps/macos/Generated");
    let xcframework = generated.join("NavoriCoreFFI.xcframework");
    let _ = std::fs::remove_dir_all(&xcframework);
    std::fs::create_dir_all(&generated)?;
    sh(Command::new("xcodebuild")
        .arg("-create-xcframework")
        .arg("-library")
        .arg(&lib)
        .arg("-headers")
        .arg(&headers)
        .arg("-output")
        .arg(&xcframework))?;

    let swift_dir = generated.join("NavoriCore");
    let _ = std::fs::remove_dir_all(&swift_dir);
    std::fs::create_dir_all(&swift_dir)?;
    std::fs::copy(
        staging.join("swift/navori_ffi.swift"),
        swift_dir.join("navori_ffi.swift"),
    )?;
    eprintln!("✓ core bindings → {}", generated.display());
    Ok(())
}

// ── bundle ─────────────────────────────────────────────────────────────────────────────

fn bundle(root: &Path, profile: Profile, flavor: Flavor, out_dir: &Path) -> Result<PathBuf> {
    build_core(root, flavor)?;

    let mut cli = cargo(root);
    cli.args(["build", "-p", "navori-cli", "--bin", "navori"]);
    if flavor == Flavor::Release {
        cli.arg("--release");
    }
    sh(&mut cli)?;

    let pkg = root.join("apps/macos");
    sh(Command::new("swift")
        .args([
            "build",
            "-c",
            flavor.swift_config(),
            "--arch",
            "arm64",
            "--product",
            "Navori",
            "--package-path",
        ])
        .arg(&pkg))?;
    let bin_dir = output(
        Command::new("swift")
            .args([
                "build",
                "-c",
                flavor.swift_config(),
                "--arch",
                "arm64",
                "--show-bin-path",
                "--package-path",
            ])
            .arg(&pkg),
    )?;
    let swift_bin = PathBuf::from(bin_dir.trim()).join("Navori");

    let app = out_dir.join(format!("{}.app", profile.app_name()));
    let _ = std::fs::remove_dir_all(&app);
    let contents = app.join("Contents");
    for d in ["MacOS", "Helpers", "Resources"] {
        std::fs::create_dir_all(contents.join(d))?;
    }
    std::fs::copy(&swift_bin, contents.join("MacOS/Navori")).context("copy app binary")?;
    std::fs::copy(
        root.join("target").join(flavor.cargo_dir()).join("navori"),
        contents.join("Helpers/navori"),
    )
    .context("copy CLI")?;
    std::fs::copy(
        root.join("xtask/assets").join(profile.icon_file()),
        contents.join("Resources/AppIcon.icns"),
    )
    .context("copy app icon")?;
    std::fs::write(contents.join("PkgInfo"), "APPL????")?;
    std::fs::write(
        contents.join("Info.plist"),
        info_plist(profile, &build_number(root)),
    )?;

    sign(
        &contents.join("Helpers/navori"),
        &format!("{}.cli", profile.bundle_id()),
    )?;
    sign(&app, profile.bundle_id())?;
    Ok(app)
}

/// Renders both icon variants with Swift/CoreGraphics and packs them with `iconutil`.
fn icon(root: &Path) -> Result<()> {
    let script = root.join("xtask/assets/make-icon.swift");
    let tmp = std::env::temp_dir().join(format!("navori-icon-{}", std::process::id()));
    for (variant, file) in [("release", "AppIcon.icns"), ("dev", "AppIconDev.icns")] {
        let set = tmp.join(format!("{variant}.iconset"));
        sh(Command::new("swift").arg(&script).arg(variant).arg(&set))?;
        sh(Command::new("iconutil")
            .args(["-c", "icns", "-o"])
            .arg(root.join("xtask/assets").join(file))
            .arg(&set))?;
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

fn build_number(root: &Path) -> String {
    let count = Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    count.unwrap_or_else(|| {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        (secs / 60).to_string()
    })
}

fn info_plist(profile: Profile, build: &str) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let name = profile.app_name();
    let id = profile.bundle_id();
    let p = profile.name();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleDisplayName</key><string>{name}</string>
    <key>CFBundleExecutable</key><string>Navori</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundleIdentifier</key><string>{id}</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>{name}</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>{version}</string>
    <key>CFBundleVersion</key><string>{build}</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
    <key>LSMinimumSystemVersion</key><string>{MIN_MACOS}</string>
    <key>LSUIElement</key><true/>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSPrincipalClass</key><string>NSApplication</string>
    <key>NSHumanReadableCopyright</key><string>Personal build</string>
    <key>NavoriProfile</key><string>{p}</string>
</dict>
</plist>
"#
    )
}

/// Signs with the local identity when present; otherwise ad-hoc with a loud warning.
fn sign(path: &Path, identifier: &str) -> Result<()> {
    let identity = std::env::var("NAVORI_SIGN_IDENTITY")
        .ok()
        .or_else(|| has_identity().then(|| SIGNING_IDENTITY.to_owned()));
    let identity = identity.unwrap_or_else(|| {
        eprintln!(
            "⚠ signing identity \"{SIGNING_IDENTITY}\" not found — signing ad-hoc.\n  \
             Ad-hoc builds lose Keychain/Accessibility grants on every rebuild. Run `cargo xtask signing-setup` once."
        );
        "-".to_owned()
    });
    sh(Command::new("codesign")
        .args([
            "--force",
            "--options",
            "runtime",
            "--timestamp=none",
            "--identifier",
            identifier,
            "--sign",
        ])
        .arg(&identity)
        .arg(path))
}

fn has_identity() -> bool {
    Command::new("security")
        .args(["find-identity", "-v", "-p", "codesigning"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(SIGNING_IDENTITY))
        .unwrap_or(false)
}

// ── run / install ───────────────────────────────────────────────────────────────────────

fn quit_app(bundle_id: &str, executable_hint: &str) {
    let _ = Command::new("osascript")
        .args([
            "-e",
            &format!("tell application id \"{bundle_id}\" to quit"),
        ])
        .output();
    for _ in 0..50 {
        let running = Command::new("pgrep")
            .args(["-f", executable_hint])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !running {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    eprintln!("⚠ {bundle_id} did not quit within 5 s");
}

fn run_dev(root: &Path) -> Result<()> {
    let profile = Profile::Dev;
    let app = bundle(
        root,
        profile,
        profile.default_flavor(),
        &root.join("build").join(profile.name()),
    )?;
    quit_app(
        Profile::Dev.bundle_id(),
        "Navori Dev.app/Contents/MacOS/Navori",
    );
    sh(Command::new("open").arg(&app))?;
    println!("✓ launched {}", app.display());
    Ok(())
}

fn install(root: &Path) -> Result<()> {
    let profile = Profile::Release;
    let app = bundle(
        root,
        profile,
        profile.default_flavor(),
        &root.join("build").join(profile.name()),
    )?;
    sh(Command::new("codesign")
        .args(["--verify", "--strict", "--deep"])
        .arg(&app))?;

    let target = PathBuf::from("/Applications/Navori.app");
    let previous = PathBuf::from("/Applications/Navori.app.previous");
    quit_app(
        Profile::Release.bundle_id(),
        "/Applications/Navori.app/Contents/MacOS/Navori",
    );
    if target.exists() {
        if previous.exists() {
            std::fs::remove_dir_all(&previous).context("remove old .previous")?;
        }
        std::fs::rename(&target, &previous).context("keep current install as .previous")?;
    }
    if let Err(e) = sh(Command::new("ditto").arg(&app).arg(&target)) {
        if previous.exists() {
            let _ = std::fs::remove_dir_all(&target);
            let _ = std::fs::rename(&previous, &target);
            eprintln!("↩ restored the previous install");
        }
        return Err(e);
    }
    sh(Command::new("open").arg(&target))?;
    println!(
        "✓ installed {} (previous kept at {})",
        target.display(),
        previous.display()
    );
    Ok(())
}

// ── signing-setup ──────────────────────────────────────────────────────────────────────

fn signing_setup(root: &Path) -> Result<()> {
    if has_identity() {
        println!("✓ \"{SIGNING_IDENTITY}\" already exists and is valid for code signing.");
        return Ok(());
    }
    let dir = root.join("build/signing");
    std::fs::create_dir_all(&dir)?;
    let key = dir.join("navori.key.pem");
    let cert = dir.join("navori.cert.pem");
    let p12 = dir.join("navori.p12");
    let password = format!("navori-{}", std::process::id());

    // /usr/bin/openssl (LibreSSL) writes PKCS#12 files that `security import` accepts.
    sh(Command::new("/usr/bin/openssl")
        .args([
            "req", "-x509", "-newkey", "rsa:3072", "-sha256", "-days", "3650", "-nodes",
        ])
        .arg("-keyout")
        .arg(&key)
        .arg("-out")
        .arg(&cert)
        .args(["-subj", &format!("/CN={SIGNING_IDENTITY}")])
        .args(["-addext", "basicConstraints=critical,CA:false"])
        .args(["-addext", "keyUsage=critical,digitalSignature"])
        .args(["-addext", "extendedKeyUsage=critical,codeSigning"]))?;
    sh(Command::new("/usr/bin/openssl")
        .args(["pkcs12", "-export", "-name", SIGNING_IDENTITY])
        .arg("-inkey")
        .arg(&key)
        .arg("-in")
        .arg(&cert)
        .arg("-out")
        .arg(&p12)
        .args(["-passout", &format!("pass:{password}")]))?;

    let keychain = dirs_home()?.join("Library/Keychains/login.keychain-db");
    sh(Command::new("security")
        .arg("import")
        .arg(&p12)
        .arg("-k")
        .arg(&keychain)
        .args(["-P", &password, "-T", "/usr/bin/codesign"]))?;
    println!("macOS will now ask you to confirm trusting this certificate for code signing.");
    sh(Command::new("security")
        .args(["add-trusted-cert", "-p", "codeSign", "-k"])
        .arg(&keychain)
        .arg(&cert))?;

    for f in [&key, &p12] {
        let _ = std::fs::remove_file(f);
    }
    ensure!(
        has_identity(),
        "identity was imported but is not valid for code signing yet"
    );
    println!(
        "✓ \"{SIGNING_IDENTITY}\" is ready. Every build is now signed with a stable identity."
    );
    Ok(())
}

fn dirs_home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set")
}

// ── check-deps ─────────────────────────────────────────────────────────────────────────

/// Architecture rules (docs/ARCHITECTURE.md §3), checked against `cargo metadata`.
fn check_deps(root: &Path) -> Result<()> {
    let meta: serde_json::Value = serde_json::from_str(&output(cargo(root).args([
        "metadata",
        "--format-version",
        "1",
        "--no-deps",
    ]))?)?;
    let packages = meta["packages"]
        .as_array()
        .context("cargo metadata: packages")?;
    let workspace: toml::Table = std::fs::read_to_string(root.join("Cargo.toml"))
        .context("read workspace Cargo.toml")?
        .parse()
        .context("parse workspace Cargo.toml")?;
    let workspace_lints = workspace
        .get("workspace")
        .and_then(|workspace| workspace.get("lints"))
        .context("workspace lints missing")?;
    let mut expected_rust = workspace_lints
        .get("rust")
        .and_then(toml::Value::as_table)
        .context("workspace rust lints missing")?
        .clone();
    expected_rust.remove("unsafe_code");
    let expected_clippy = workspace_lints
        .get("clippy")
        .and_then(toml::Value::as_table)
        .context("workspace clippy lints missing")?;

    let mut violations = Vec::new();
    for p in packages {
        let name = p["name"].as_str().unwrap_or_default();
        let path = p["manifest_path"].as_str().unwrap_or_default();
        let deps: Vec<(&str, &str)> = p["dependencies"]
            .as_array()
            .map(|d| {
                d.iter()
                    .map(|d| {
                        (
                            d["name"].as_str().unwrap_or_default(),
                            d["kind"].as_str().unwrap_or("normal"),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let internal: Vec<&str> = deps
            .iter()
            .filter(|(n, kind)| n.starts_with("navori-") && *kind != "dev")
            .map(|(n, _)| *n)
            .collect();

        for d in p["dependencies"].as_array().into_iter().flatten() {
            let features: Vec<&str> = d["features"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .collect();
            let dep_name = d["name"].as_str().unwrap_or_default();
            let kind = d["kind"].as_str().unwrap_or("normal");
            violations.extend(fake_feature_violation(name, dep_name, kind, &features));
            if let Some(dep_package) = packages.iter().find(|q| q["name"] == dep_name) {
                violations.extend(forwarded_fake_edge_violations(
                    name,
                    dep_name,
                    kind,
                    &features,
                    &feature_table(dep_package),
                    &dep_aliases(dep_package),
                ));
            }
        }
        violations.extend(default_forwarding_violations(
            name,
            &feature_table(p),
            &dep_aliases(p),
        ));

        for dep in &internal {
            if let Some(rule) = internal_dep_violation(name, path, dep) {
                violations.push(format!("{name} → {dep}: {rule}"));
            }
        }

        if name != "navori-ffi" && name != "navori-macos" {
            let manifest = read_manifest(path)?;
            let inherits = manifest
                .get("lints")
                .and_then(|lints| lints.get("workspace"))
                .and_then(toml::Value::as_bool)
                == Some(true);
            if !inherits {
                violations.push(format!("{name}: [lints] workspace = true is required"));
            }
        }
        if name == "navori-macos" {
            let manifest = read_manifest(path)?;
            let rust_lints = manifest
                .get("lints")
                .and_then(|lints| lints.get("rust"))
                .and_then(toml::Value::as_table);
            let clippy_lints = manifest
                .get("lints")
                .and_then(|lints| lints.get("clippy"))
                .and_then(toml::Value::as_table);
            if rust_lints != Some(&expected_rust) {
                violations.push(format!(
                    "{name}: [lints.rust] must match the workspace rust lints minus unsafe_code"
                ));
            }
            if clippy_lints != Some(expected_clippy) {
                violations.push(format!(
                    "{name}: [lints.clippy] must match the workspace clippy lints"
                ));
            }
        }
    }
    violations.extend(public_unsafe_items(root)?);
    if violations.is_empty() {
        println!("✓ dependency rules hold for {} packages", packages.len());
        Ok(())
    } else {
        bail!("dependency rule violations:\n  {}", violations.join("\n  "))
    }
}

/// The rule an internal dependency edge breaks, if any.
fn internal_dep_violation(package: &str, path: &str, dep: &str) -> Option<String> {
    let is_module = path.contains("/modules/");
    let is_ai = path.contains("/ai/");
    let dep_is_module = dep.starts_with("navori-mod-");
    let rule: Option<&str> = if dep == "navori-ffi" {
        Some("nothing may depend on navori-ffi (it is the app's composition root)")
    } else if package == "navori-protocol" {
        Some("navori-protocol must stay dependency-free")
    } else if package == "navori-cli"
        && !["navori-config", "navori-ipc", "navori-protocol"].contains(&dep)
    {
        Some("the CLI is a thin client: only config, ipc and protocol")
    } else if package == "navori-macos" {
        Some("navori-macos has no internal dependencies")
    } else if dep == "navori-macos"
        && package != "navori-procs"
        && package != "navori-ffi"
        && !is_module
    {
        Some("only navori-procs, navori-ffi, or a module may depend on navori-macos")
    } else if is_module && dep_is_module {
        Some("modules must not depend on other modules (use the event bus)")
    } else if is_module && dep.starts_with("navori-ai") {
        Some("modules must not depend on AI crates")
    } else if package == "navori-agent" && dep == "navori-llm" {
        Some("navori-agent must not depend on navori-llm")
    } else if is_ai && dep_is_module {
        Some("AI crates see modules only through ActionSpec / ContextFragment")
    } else if !is_module && !is_ai && package != "navori-ffi" && dep_is_module {
        Some("foundation crates must not depend on modules")
    } else {
        None
    };
    rule.map(str::to_owned)
}

/// Crates whose `fake` feature switches on unencrypted in-memory test doubles (ARCH-8).
const FAKE_GUARDED: [&str; 3] = ["navori-llm", "navori-macos", "navori-secrets"];

/// Reports `fake` enabled for a guarded crate through a non-dev dependency edge.
fn fake_feature_violation(krate: &str, dep: &str, kind: &str, features: &[&str]) -> Option<String> {
    (FAKE_GUARDED.contains(&dep) && kind != "dev" && features.contains(&"fake")).then(|| {
        format!(
            "{krate} → {dep}: the `fake` feature is test-only (ARCH-8) and may be enabled only \
             through a dev-dependency, not a {kind} dependency"
        )
    })
}

/// A package's `[features]` table as `cargo metadata` reports it.
type FeatureTable = std::collections::BTreeMap<String, Vec<String>>;

/// A package's dependency names as written in its manifest (the `rename`, else the crate
/// name) mapped to the real package name. A name absent from the map stands for itself.
type Aliases = std::collections::BTreeMap<String, String>;

/// Reads the alias map from the `dependencies` of a `cargo metadata` package.
fn dep_aliases(package: &serde_json::Value) -> Aliases {
    package["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| {
            Some((
                d["rename"].as_str()?.to_owned(),
                d["name"].as_str()?.to_owned(),
            ))
        })
        .collect()
}

/// Reads the `features` map of a `cargo metadata` package.
fn feature_table(package: &serde_json::Value) -> FeatureTable {
    package["features"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(feature, entries)| {
            let entries = entries
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|entry| entry.as_str().map(str::to_owned))
                .collect();
            (feature.clone(), entries)
        })
        .collect()
}

/// Features of one package that forward `fake` to a guarded crate (`crate/fake` or
/// `crate?/fake`), as `(feature, guarded crate)`. `dep:name` entries and the implicit
/// feature of an optional dependency only switch the dependency on, so they do not count.
fn fake_forwarding_features(features: &FeatureTable, aliases: &Aliases) -> Vec<(String, String)> {
    features
        .iter()
        .flat_map(|(feature, entries)| {
            entries.iter().filter_map(move |entry| {
                let (krate, enabled) = entry.split_once('/')?;
                let krate = krate.strip_suffix('?').unwrap_or(krate);
                let krate = aliases.get(krate).map_or(krate, String::as_str);
                (enabled == "fake" && FAKE_GUARDED.contains(&krate))
                    .then(|| (feature.clone(), krate.to_owned()))
            })
        })
        .collect()
}

/// The features switched on by `roots` within one package, transitively. Entries naming
/// another crate (`dep:x`, `x/y`) are not followed; the visited set ends feature cycles.
fn enabled_features<'a>(
    features: &'a FeatureTable,
    roots: impl IntoIterator<Item = &'a str>,
) -> std::collections::BTreeSet<&'a str> {
    let mut seen = std::collections::BTreeSet::new();
    let mut pending: Vec<&str> = roots.into_iter().collect();
    while let Some(name) = pending.pop() {
        if let Some((key, entries)) = features.get_key_value(name)
            && seen.insert(key.as_str())
        {
            pending.extend(entries.iter().map(String::as_str));
        }
    }
    seen
}

/// Reports a forwarding feature that the package's `default` set switches on, directly or
/// through other features of the same package.
fn default_forwarding_violations(
    krate: &str,
    features: &FeatureTable,
    aliases: &Aliases,
) -> Vec<String> {
    let on = enabled_features(features, ["default"]);
    fake_forwarding_features(features, aliases)
        .into_iter()
        .filter(|(feature, _)| on.contains(feature.as_str()))
        .map(|(feature, guarded)| {
            if feature == "default" {
                format!(
                    "{krate} → {guarded}: the default features of {krate} forward the test-only \
                     `fake` feature (ARCH-8); stop forwarding `fake` from default and enable it \
                     only through a dev-dependency"
                )
            } else {
                format!(
                    "{krate} → {guarded}: the default features of {krate} enable `{feature}`, \
                     which forwards the test-only `fake` feature (ARCH-8); make `{feature}` \
                     opt-in and enable it only through a dev-dependency"
                )
            }
        })
        .collect()
}

/// Reports a non-dev dependency edge that turns on a forwarding feature of another package.
fn forwarded_fake_edge_violations(
    krate: &str,
    dep: &str,
    kind: &str,
    enabled: &[&str],
    dep_features: &FeatureTable,
    dep_aliases: &Aliases,
) -> Vec<String> {
    if kind == "dev" {
        return Vec::new();
    }
    let on = enabled_features(dep_features, enabled.iter().copied());
    fake_forwarding_features(dep_features, dep_aliases)
        .into_iter()
        .filter(|(feature, _)| on.contains(feature.as_str()))
        .map(|(feature, guarded)| {
            format!(
                "{krate} → {dep}: enables `{feature}`, which forwards the test-only `fake` feature \
                 to {guarded} (ARCH-8); it may be enabled only through a dev-dependency, not a \
                 {kind} dependency"
            )
        })
        .collect()
}

fn read_manifest(path: &str) -> Result<toml::Table> {
    std::fs::read_to_string(path)
        .with_context(|| format!("read {path}"))?
        .parse()
        .with_context(|| format!("parse {path}"))
}

fn public_unsafe_items(root: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    rust_sources(&root.join("crates/macos/src"), &mut files)?;
    let mut violations = Vec::new();
    for file in files {
        let text =
            std::fs::read_to_string(&file).with_context(|| format!("read {}", file.display()))?;
        for (idx, line) in text.lines().enumerate() {
            if line.contains("pub unsafe fn")
                || line.contains("pub unsafe trait")
                || line.contains("pub(crate) unsafe trait")
                || line.contains("pub unsafe extern")
            {
                let rel = file.strip_prefix(root).unwrap_or(file.as_path());
                violations.push(format!(
                    "navori-macos: public unsafe item at {}:{}: no pub unsafe fn, pub unsafe trait, pub(crate) unsafe trait, or pub unsafe extern",
                    rel.display(),
                    idx + 1
                ));
            }
        }
    }
    Ok(violations)
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let mut entries = std::fs::read_dir(dir)
        .with_context(|| format!("read {}", dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("read {}", dir.display()))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn lint_probe(root: &Path) -> Result<()> {
    let dir = std::env::temp_dir().join(format!("navori-lint-probe-{}", std::process::id()));
    let _cleanup = RemoveOnDrop(dir.clone());
    if dir.exists() {
        std::fs::remove_dir_all(&dir).context("reset lint-probe directory")?;
    }
    std::fs::create_dir_all(&dir).context("create lint-probe directory")?;

    let workspace_lints = workspace_lint_table(root)?;
    std::fs::write(
        dir.join("Cargo.toml"),
        probe_workspace_manifest(&workspace_lints)?,
    )
    .context("write lint-probe workspace manifest")?;
    write_probe_member(
        &dir,
        "a",
        &member_lints(root, "crates/procs/Cargo.toml")?,
        "pub fn probe() -> u8 { let x = 1u8;\n// SAFETY: x is a live local u8.\nunsafe { std::ptr::read(&x) } }\n",
    )?;
    write_probe_member(
        &dir,
        "b",
        &member_lints(root, "crates/macos/Cargo.toml")?,
        "pub fn probe() -> u8 { let x = 1u8;\nunsafe { std::ptr::read(&x) } }\n",
    )?;

    require_codes("a", &clippy_error_codes(&dir.join("a"))?, &["unsafe_code"])?;
    require_codes(
        "b",
        &clippy_error_codes(&dir.join("b"))?,
        &["clippy::undocumented_unsafe_blocks"],
    )?;
    println!("✓ lint-probe");
    Ok(())
}

fn workspace_lint_table(root: &Path) -> Result<toml::Value> {
    let workspace: toml::Table = std::fs::read_to_string(root.join("Cargo.toml"))
        .context("read workspace Cargo.toml")?
        .parse()
        .context("parse workspace Cargo.toml")?;
    workspace
        .get("workspace")
        .and_then(|workspace| workspace.get("lints"))
        .cloned()
        .context("workspace lints missing")
}

fn member_lints(root: &Path, rel: &str) -> Result<toml::Value> {
    let path = root.join(rel);
    let manifest = read_manifest(
        path.to_str()
            .with_context(|| format!("{rel} is not valid UTF-8"))?,
    )?;
    manifest
        .get("lints")
        .cloned()
        .with_context(|| format!("{rel} has no [lints]"))
}

fn probe_workspace_manifest(lints: &toml::Value) -> Result<String> {
    let mut package = toml::Table::new();
    package.insert("edition".into(), "2024".into());
    let mut workspace = toml::Table::new();
    workspace.insert("resolver".into(), "3".into());
    workspace.insert(
        "members".into(),
        toml::Value::Array(vec!["a".into(), "b".into()]),
    );
    workspace.insert("package".into(), toml::Value::Table(package));
    workspace.insert("lints".into(), lints.clone());
    let mut manifest = toml::Table::new();
    manifest.insert("workspace".into(), toml::Value::Table(workspace));
    toml::to_string(&manifest).context("serialize lint-probe workspace")
}

fn write_probe_member(dir: &Path, name: &str, lints: &toml::Value, src: &str) -> Result<()> {
    let member = dir.join(name);
    std::fs::create_dir_all(member.join("src")).context("create lint-probe member")?;
    let mut edition = toml::Table::new();
    edition.insert("workspace".into(), toml::Value::Boolean(true));
    let mut package = toml::Table::new();
    package.insert("name".into(), name.into());
    package.insert("version".into(), "0.0.0".into());
    package.insert("edition".into(), toml::Value::Table(edition));
    let mut manifest = toml::Table::new();
    manifest.insert("package".into(), toml::Value::Table(package));
    manifest.insert("lints".into(), lints.clone());
    std::fs::write(
        member.join("Cargo.toml"),
        toml::to_string(&manifest).context("serialize lint-probe member")?,
    )
    .context("write lint-probe member manifest")?;
    std::fs::write(member.join("src/lib.rs"), src).context("write lint-probe fixture")?;
    Ok(())
}

fn clippy_error_codes(dir: &Path) -> Result<Vec<String>> {
    let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cmd.current_dir(dir)
        .args(["clippy", "--message-format=json", "--", "-D", "warnings"])
        .env("CARGO_TERM_COLOR", "never")
        // The probe dir is deleted on drop; a inherited target dir would leak the build.
        .env_remove("CARGO_TARGET_DIR");
    let out = cmd
        .output()
        .context("cannot run cargo clippy for lint-probe")?;
    let stdout = String::from_utf8(out.stdout).context("lint-probe clippy stdout is not UTF-8")?;
    let mut codes = Vec::new();
    for line in stdout.lines() {
        if !line.starts_with('{') {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(line).context("lint-probe clippy JSON line")?;
        if value["reason"] != "compiler-message" || value["message"]["level"] != "error" {
            continue;
        }
        if let Some(code) = value["message"]["code"]["code"].as_str() {
            codes.push(code.to_owned());
        }
    }
    if !out.status.success() && codes.is_empty() {
        bail!(
            "lint-probe clippy failed with no error codes:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(codes)
}

fn require_codes(fixture: &str, got: &[String], expected: &[&str]) -> Result<()> {
    let same = got.len() == expected.len() && got.iter().zip(expected).all(|(got, exp)| got == exp);
    if !same {
        bail!(
            "fixture {fixture}: expected [{}], got [{}]",
            expected.join(", "),
            got.join(", ")
        );
    }
    Ok(())
}

// ── outdated ───────────────────────────────────────────────────────────────────────────

fn outdated(root: &Path) -> Result<()> {
    let manifest: toml::Table = std::fs::read_to_string(root.join("Cargo.toml"))?.parse()?;
    let deps = manifest
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.as_table())
        .context("[workspace.dependencies] missing")?;
    let mut stale = 0;
    for (name, spec) in deps {
        let wanted = match spec {
            toml::Value::String(v) => v.clone(),
            toml::Value::Table(t) => match t.get("version").and_then(|v| v.as_str()) {
                Some(v) => v.to_owned(),
                None => continue, // path dependency
            },
            _ => continue,
        };
        let body = output(Command::new("curl").args([
            "-sf",
            "-H",
            "User-Agent: navori-xtask",
            &format!("https://crates.io/api/v1/crates/{name}"),
        ]))?;
        let latest =
            serde_json::from_str::<serde_json::Value>(&body)?["crate"]["max_stable_version"]
                .as_str()
                .unwrap_or("?")
                .to_owned();
        let latest_core = latest.split('+').next().unwrap_or(&latest);
        let marker = if latest_core == wanted { "✓" } else { "↑" };
        if marker == "↑" {
            stale += 1;
        }
        println!("{marker} {name:<22} {wanted:<12} latest {latest}");
    }
    println!("{stale} dependencies behind the latest stable release");
    Ok(())
}

// ── ci ─────────────────────────────────────────────────────────────────────────────────

fn ci(root: &Path) -> Result<()> {
    sh(cargo(root).args(["fmt", "--all", "--check"]))?;
    sh(cargo(root).args([
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ]))?;
    sh(cargo(root).args(["test", "--workspace"]))?;
    check_deps(root)?;
    lint_probe(root)?;
    release::check_workflows(root)?;
    build_core(root, Profile::Dev.default_flavor())?;
    sh(Command::new("swift")
        .args(["build", "--arch", "arm64", "--package-path"])
        .arg(root.join("apps/macos")))?;
    sh(Command::new("swift")
        .args(["test", "--arch", "arm64", "--package-path"])
        .arg(root.join("apps/macos")))?;
    println!("✓ ci passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::process::Command;

    use std::time::Duration;

    use super::{internal_dep_violation, parse_duration, parse_load_duration, tool_stdout};

    #[test]
    fn durations_read_seconds_minutes_and_hours() {
        assert_eq!(parse_duration("90s").unwrap(), Duration::from_secs(90));
        assert_eq!(parse_duration("20m").unwrap(), Duration::from_secs(1200));
        assert_eq!(parse_duration("2h").unwrap(), Duration::from_secs(7200));
    }

    #[test]
    fn empty_zero_and_unknown_unit_durations_are_refused() {
        for bad in ["", "0s", "0m", "5d", "s", "-5s"] {
            let err = parse_duration(bad).unwrap_err();
            assert!(err.contains(&format!("{bad:?}")), "{err}");
        }
    }

    #[test]
    fn a_soak_load_longer_than_a_day_is_refused_naming_the_value() {
        assert_eq!(
            parse_load_duration("24h").unwrap(),
            Duration::from_secs(86400)
        );
        let err = parse_load_duration("25h").unwrap_err();
        assert!(err.contains("25h") && err.contains("24h"), "{err}");
    }

    #[test]
    fn a_duration_without_a_unit_is_refused() {
        let err = parse_duration("90").unwrap_err();
        assert!(err.contains("90"), "{err}");
    }

    #[test]
    fn the_agent_crate_must_not_depend_on_the_llm_crate() {
        assert_eq!(
            internal_dep_violation("navori-agent", "/repo/ai/agent", "navori-llm").as_deref(),
            Some("navori-agent must not depend on navori-llm")
        );
        assert_eq!(
            internal_dep_violation("navori-agent", "/repo/ai/agent", "navori-context"),
            None
        );
    }

    /// Child tool progress must be on a pipe, not inherited stdout, so
    /// `cargo xtask perf` can emit `build <seconds>s` as the first report line.
    #[test]
    fn tool_stdout_is_piped_so_report_stdout_stays_clean() {
        let mut child = Command::new("/bin/echo")
            .arg("xcframework successfully written out to: /tmp/fake")
            .stdout(tool_stdout())
            .spawn()
            .unwrap();
        let mut pipe = child
            .stdout
            .take()
            .expect("tool stdout must be a pipe so perf report stdout stays clean");
        let mut buf = String::new();
        pipe.read_to_string(&mut buf).unwrap();
        let status = child.wait().unwrap();
        assert!(status.success());
        assert!(
            buf.contains("xcframework successfully written out to:"),
            "piped tool stdout was empty: {buf:?}"
        );
    }

    use super::fake_feature_violation;

    #[test]
    fn fake_through_a_normal_dependency_on_macos_is_rejected() {
        let v = fake_feature_violation("navori-procs", "navori-macos", "normal", &["fake"])
            .expect("fake on a normal dependency must be rejected");
        assert!(v.contains("navori-procs"), "{v}");
        assert!(v.contains("navori-macos"), "{v}");
        assert!(v.contains("fake"), "{v}");
    }

    #[test]
    fn fake_through_a_normal_dependency_on_secrets_is_rejected() {
        let v = fake_feature_violation("navori-ffi", "navori-secrets", "normal", &["fake"])
            .expect("fake on a normal dependency must be rejected");
        assert!(v.contains("navori-ffi"), "{v}");
        assert!(v.contains("navori-secrets"), "{v}");
        assert!(v.contains("fake"), "{v}");
    }

    #[test]
    fn fake_through_a_normal_dependency_on_llm_is_rejected() {
        let v = fake_feature_violation("navori-ffi", "navori-llm", "normal", &["fake"])
            .expect("fake on a normal dependency must be rejected");
        assert!(v.contains("navori-llm"), "{v}");
        assert!(v.contains("fake"), "{v}");
    }

    #[test]
    fn fake_through_a_build_dependency_is_rejected() {
        assert!(
            fake_feature_violation("navori-ffi", "navori-secrets", "build", &["fake"]).is_some()
        );
        assert!(fake_feature_violation("navori-ffi", "navori-macos", "build", &["fake"]).is_some());
    }

    /// `cargo metadata` reports a `[target.*.dependencies]` entry with kind `normal`
    /// (the platform is carried separately), so it is the same edge kind.
    #[test]
    fn fake_through_a_target_specific_normal_dependency_is_rejected() {
        assert!(
            fake_feature_violation("navori-ffi", "navori-macos", "normal", &["x", "fake"])
                .is_some()
        );
    }

    #[test]
    fn fake_through_a_dev_dependency_passes() {
        assert_eq!(
            fake_feature_violation("navori-procs", "navori-macos", "dev", &["fake"]),
            None
        );
        assert_eq!(
            fake_feature_violation("navori-ffi", "navori-secrets", "dev", &["fake"]),
            None
        );
    }

    #[test]
    fn secrets_self_dev_dependency_passes() {
        assert_eq!(
            fake_feature_violation("navori-secrets", "navori-secrets", "dev", &["fake"]),
            None
        );
    }

    #[test]
    fn normal_dependency_without_fake_passes() {
        assert_eq!(
            fake_feature_violation("navori-ffi", "navori-macos", "normal", &[]),
            None
        );
        assert_eq!(
            fake_feature_violation("navori-ffi", "navori-secrets", "normal", &["other"]),
            None
        );
    }

    #[test]
    fn fake_on_an_unguarded_dependency_is_out_of_scope() {
        assert_eq!(
            fake_feature_violation("navori-ffi", "navori-config", "normal", &["fake"]),
            None
        );
    }

    use super::{
        Aliases, FeatureTable, default_forwarding_violations, fake_forwarding_features,
        forwarded_fake_edge_violations,
    };

    fn no_aliases() -> Aliases {
        Aliases::new()
    }

    fn aliases(pairs: &[(&str, &str)]) -> Aliases {
        pairs
            .iter()
            .map(|(a, r)| ((*a).to_owned(), (*r).to_owned()))
            .collect()
    }

    fn table(entries: &[(&str, &[&str])]) -> FeatureTable {
        entries
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.iter().map(|s| (*s).to_owned()).collect()))
            .collect()
    }

    #[test]
    fn a_default_feature_forwarding_fake_is_rejected() {
        let t = table(&[
            ("default", &["testing"]),
            ("testing", &["navori-secrets/fake"]),
        ]);
        let v = default_forwarding_violations("navori-ffi", &t, &no_aliases());
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("navori-ffi"), "{v:?}");
        assert!(v[0].contains("testing"), "{v:?}");
        assert!(v[0].contains("navori-secrets"), "{v:?}");
    }

    #[test]
    fn a_forwarding_feature_reached_through_other_features_is_rejected() {
        let t = table(&[
            ("default", &["a"]),
            ("a", &["b"]),
            ("b", &["navori-macos/fake"]),
        ]);
        let v = default_forwarding_violations("navori-procs", &t, &no_aliases());
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains('b') && v[0].contains("navori-macos"), "{v:?}");
    }

    #[test]
    fn an_optional_dependency_spelling_is_rejected_from_default_too() {
        let t = table(&[
            ("default", &["testing", "other"]),
            ("testing", &["navori-secrets?/fake", "other"]),
            ("other", &[]),
        ]);
        let v = default_forwarding_violations("navori-ffi", &t, &no_aliases());
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("navori-secrets"), "{v:?}");
    }

    #[test]
    fn a_forwarding_feature_outside_default_passes() {
        let t = table(&[
            ("default", &["other"]),
            ("other", &[]),
            ("testing", &["navori-secrets/fake", "navori-macos?/fake"]),
        ]);
        assert!(default_forwarding_violations("navori-ffi", &t, &no_aliases()).is_empty());
        assert_eq!(fake_forwarding_features(&t, &no_aliases()).len(), 2);
    }

    #[test]
    fn a_package_without_default_passes() {
        let t = table(&[("testing", &["navori-secrets/fake"])]);
        assert!(default_forwarding_violations("navori-ffi", &t, &no_aliases()).is_empty());
    }

    #[test]
    fn dep_syntax_and_implicit_features_do_not_forward() {
        let t = table(&[
            ("default", &["navori-secrets", "dep-form"]),
            ("navori-secrets", &["dep:navori-secrets"]),
            ("dep-form", &["dep:navori-macos"]),
        ]);
        assert!(fake_forwarding_features(&t, &no_aliases()).is_empty());
        assert!(default_forwarding_violations("navori-ffi", &t, &no_aliases()).is_empty());
    }

    #[test]
    fn forwarding_to_an_unguarded_crate_or_another_feature_is_out_of_scope() {
        let t = table(&[
            ("default", &["a"]),
            ("a", &["navori-config/fake", "navori-secrets/other"]),
        ]);
        assert!(fake_forwarding_features(&t, &no_aliases()).is_empty());
        assert!(default_forwarding_violations("navori-ffi", &t, &no_aliases()).is_empty());
    }

    #[test]
    fn feature_cycles_terminate() {
        let t = table(&[
            ("default", &["a"]),
            ("a", &["b"]),
            ("b", &["a", "default", "navori-secrets/fake"]),
        ]);
        let v = default_forwarding_violations("navori-ffi", &t, &no_aliases());
        assert_eq!(v.len(), 1, "{v:?}");
        let cyclic = table(&[("default", &["a"]), ("a", &["b"]), ("b", &["a", "default"])]);
        assert!(default_forwarding_violations("navori-ffi", &cyclic, &no_aliases()).is_empty());
    }

    #[test]
    fn a_normal_edge_enabling_a_forwarding_feature_is_rejected() {
        let other = table(&[("testing", &["navori-secrets/fake"])]);
        let v = forwarded_fake_edge_violations(
            "navori-ffi",
            "navori-other",
            "normal",
            &["testing"],
            &other,
            &no_aliases(),
        );
        assert_eq!(v.len(), 1, "{v:?}");
        for needle in ["navori-ffi", "navori-other", "testing", "navori-secrets"] {
            assert!(v[0].contains(needle), "{needle}: {v:?}");
        }
    }

    #[test]
    fn a_normal_edge_enabling_a_feature_that_reaches_a_forwarding_one_is_rejected() {
        let other = table(&[("all", &["testing"]), ("testing", &["navori-macos?/fake"])]);
        let v = forwarded_fake_edge_violations(
            "navori-ffi",
            "navori-other",
            "build",
            &["all"],
            &other,
            &no_aliases(),
        );
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn a_dev_edge_enabling_a_forwarding_feature_passes() {
        let other = table(&[("testing", &["navori-secrets/fake"])]);
        assert!(
            forwarded_fake_edge_violations(
                "navori-ffi",
                "navori-other",
                "dev",
                &["testing"],
                &other,
                &no_aliases()
            )
            .is_empty()
        );
    }

    #[test]
    fn a_normal_edge_enabling_a_harmless_feature_passes() {
        let other = table(&[
            ("testing", &["navori-secrets/fake"]),
            ("extra", &["dep:bee"]),
            ("bee", &["dep:bee"]),
        ]);
        assert!(
            forwarded_fake_edge_violations(
                "navori-ffi",
                "navori-other",
                "normal",
                &["extra", "bee", "missing"],
                &other,
                &no_aliases()
            )
            .is_empty()
        );
        assert!(
            forwarded_fake_edge_violations(
                "navori-ffi",
                "navori-other",
                "normal",
                &[],
                &other,
                &no_aliases()
            )
            .is_empty()
        );
    }

    #[test]
    fn a_cyclic_dependency_feature_table_terminates_on_an_edge() {
        let other = table(&[("a", &["b"]), ("b", &["a"])]);
        assert!(
            forwarded_fake_edge_violations(
                "navori-ffi",
                "navori-other",
                "normal",
                &["a"],
                &other,
                &no_aliases()
            )
            .is_empty()
        );
    }

    #[test]
    fn a_renamed_dependency_forwarding_fake_from_default_is_rejected() {
        let t = table(&[("default", &["testing"]), ("testing", &["sec/fake"])]);
        let a = aliases(&[("sec", "navori-secrets")]);
        let v = default_forwarding_violations("navori-ffi", &t, &a);
        assert_eq!(v.len(), 1, "{v:?}");
        for needle in ["navori-ffi", "testing", "navori-secrets"] {
            assert!(v[0].contains(needle), "{needle}: {v:?}");
        }
    }

    #[test]
    fn a_renamed_forwarding_feature_outside_default_passes() {
        let t = table(&[("default", &[]), ("testing", &["sec?/fake"])]);
        let a = aliases(&[("sec", "navori-secrets")]);
        assert!(default_forwarding_violations("navori-ffi", &t, &a).is_empty());
        assert_eq!(
            fake_forwarding_features(&t, &a),
            [("testing".to_owned(), "navori-secrets".to_owned())]
        );
    }

    #[test]
    fn a_normal_edge_enabling_a_renamed_forwarding_feature_is_rejected() {
        let other = table(&[("testing", &["mac/fake"])]);
        let a = aliases(&[("mac", "navori-macos")]);
        let v = forwarded_fake_edge_violations(
            "navori-ffi",
            "navori-other",
            "normal",
            &["testing"],
            &other,
            &a,
        );
        assert_eq!(v.len(), 1, "{v:?}");
        for needle in ["navori-ffi", "navori-other", "testing", "navori-macos"] {
            assert!(v[0].contains(needle), "{needle}: {v:?}");
        }
    }

    #[test]
    fn a_renamed_unguarded_dependency_is_ignored() {
        let t = table(&[("default", &["testing"]), ("testing", &["cfg/fake"])]);
        let a = aliases(&[("cfg", "navori-config")]);
        assert!(default_forwarding_violations("navori-ffi", &t, &a).is_empty());
        assert!(fake_forwarding_features(&t, &a).is_empty());
    }

    #[test]
    fn an_alias_equal_to_another_crates_real_name_does_not_confuse_the_lookup() {
        // `navori-secrets = { package = "navori-config" }`: the name is an alias of an
        // unguarded crate, and the guarded crate is reached under `sec`.
        let t = table(&[
            ("default", &["a"]),
            ("a", &["navori-secrets/fake"]),
            ("b", &["sec/fake"]),
        ]);
        let a = aliases(&[("navori-secrets", "navori-config"), ("sec", "navori-macos")]);
        assert_eq!(
            fake_forwarding_features(&t, &a),
            [("b".to_owned(), "navori-macos".to_owned())]
        );
        assert!(default_forwarding_violations("navori-ffi", &t, &a).is_empty());
    }

    #[test]
    fn forwarding_straight_from_default_does_not_ask_to_make_default_opt_in() {
        let t = table(&[("default", &["navori-secrets/fake"])]);
        let v = default_forwarding_violations("navori-ffi", &t, &no_aliases());
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(!v[0].contains("make `default` opt-in"), "{v:?}");
        assert!(v[0].contains("stop forwarding"), "{v:?}");
        assert!(v[0].contains("dev-dependency"), "{v:?}");
    }
}
