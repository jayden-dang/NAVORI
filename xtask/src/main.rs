//! `cargo xtask <command>` — build, bundle, sign, install and check Navori.
//!
//! Profiles: `dev` → "Navori Dev.app" (`dev.navori.app.debug`, debug build);
//!           `release` → "Navori.app" (`dev.navori.app`, release build).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand, ValueEnum};

mod perf;

const SIGNING_IDENTITY: &str = "Navori Local Signing";
const MIN_MACOS: &str = "15.0";

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
        Cmd::Outdated => outdated(&root),
        Cmd::Icon => icon(&root),
        Cmd::Ci => ci(&root),
        Cmd::Perf { record } => {
            if !perf::run(&root, record)? {
                bail!("perf gate failed");
            }
            Ok(())
        }
    }
}

fn workspace_root() -> Result<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map(Path::to_path_buf)
        .context("xtask must live one level below the workspace root")
}

/// Runs a command, echoing it, failing on non-zero exit.
fn sh(cmd: &mut Command) -> Result<()> {
    eprintln!("$ {}", render(cmd));
    let status = cmd
        .status()
        .with_context(|| format!("cannot run {}", render(cmd)))?;
    ensure!(
        status.success(),
        "command failed ({status}): {}",
        render(cmd)
    );
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

        let is_module = path.contains("/modules/");
        let is_ai = path.contains("/ai/");
        for dep in &internal {
            let dep_is_module = dep.starts_with("navori-mod-");
            let rule = if *dep == "navori-ffi" {
                Some("nothing may depend on navori-ffi (it is the app's composition root)")
            } else if name == "navori-protocol" {
                Some("navori-protocol must stay dependency-free")
            } else if name == "navori-cli"
                && !["navori-config", "navori-ipc", "navori-protocol"].contains(dep)
            {
                Some("the CLI is a thin client: only config, ipc and protocol")
            } else if is_module && dep_is_module {
                Some("modules must not depend on other modules (use the event bus)")
            } else if is_module && dep.starts_with("navori-ai") {
                Some("modules must not depend on AI crates")
            } else if is_ai && dep_is_module {
                Some("AI crates see modules only through ActionSpec / ContextFragment")
            } else if !is_module && !is_ai && name != "navori-ffi" && dep_is_module {
                Some("foundation crates must not depend on modules")
            } else {
                None
            };
            if let Some(rule) = rule {
                violations.push(format!("{name} → {dep}: {rule}"));
            }
        }
    }
    if violations.is_empty() {
        println!("✓ dependency rules hold for {} packages", packages.len());
        Ok(())
    } else {
        bail!("dependency rule violations:\n  {}", violations.join("\n  "))
    }
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
    build_core(root, Profile::Dev.default_flavor())?;
    sh(Command::new("swift")
        .args(["build", "--arch", "arm64", "--package-path"])
        .arg(root.join("apps/macos")))?;
    println!("✓ ci passed");
    Ok(())
}
