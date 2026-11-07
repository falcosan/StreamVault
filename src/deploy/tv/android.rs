use super::{ICON, between, bundle, os};
use crate::deploy::{
    Session, VERSION, ask, attempt, binary, create, parent, say, tidy, wait_until, write,
};
use crate::settings::SyncConfig;
use std::env::consts::ARCH;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use tokio::process::Command;

pub const ADB: u16 = 5555;
pub const REMOTE: u16 = 6466;
const API: &str = "36";
const BUILD_TOOLS: &str = "36.0.0";
const MIN_API: &str = "23";
const JAVA_RELEASE: &str = "17";
const DEBUG_PASSWORD: &str = "android";
const WEBVIEW_MIN: u32 = 96;
const CMDLINE_TOOLS: &str = "15641748";
const LICENSES: usize = 20;
const WEBVIEW: &str = "Current WebView package (name, version): (";
const MANIFEST: &str = include_str!("../../../resources/tv/android/AndroidManifest.xml");
const ACTIVITY: &str = include_str!("../../../resources/tv/android/MainActivity.java");
const BANNER: &str = include_str!("../../../resources/tv/android/banner.xml");
const READY: &str = "On the TV: enable Developer options (tap Build 7 times in About), turn on USB, network or wireless debugging (Fire TV: ADB debugging), and allow this computer when asked.";

pub async fn attached(session: &Session) -> Vec<(String, String)> {
    let adb = adb_in(&user_sdk());
    let Ok(output) = Command::new(&adb).args(["devices", "-l"]).output().await else {
        return Vec::new();
    };
    if String::from_utf8_lossy(&output.stderr).contains("daemon started successfully") {
        session.on_exit(move || tidy(&adb, &["kill-server"]));
    }
    devices(&String::from_utf8_lossy(&output.stdout))
}

fn devices(list: &str) -> Vec<(String, String)> {
    list.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let serial = fields.next()?;
            let model = fields
                .clone()
                .find_map(|field| field.strip_prefix("model:"));
            (fields.next()? == "device" && !serial.contains(':'))
                .then(|| (serial.to_owned(), model.unwrap_or(serial).to_owned()))
        })
        .collect()
}

fn service(list: &str, target: &str, kind: &str) -> Option<String> {
    list.lines().find_map(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let (service, address) = (fields.get(1)?, fields.get(2)?);
        (service.contains(&format!("_adb-tls-{kind}"))
            && address.starts_with(&format!("{target}:")))
        .then(|| (*address).to_owned())
    })
}

fn build_tools(root: &Path) -> PathBuf {
    root.join("build-tools").join(BUILD_TOOLS)
}

fn android_jar(root: &Path) -> PathBuf {
    root.join(format!("platforms/android-{API}/android.jar"))
}

pub async fn deploy(
    session: &Session,
    target: &str,
    sync: Option<&SyncConfig>,
) -> Result<(), String> {
    let package =
        between(MANIFEST, "package=\"", "\"").ok_or("AndroidManifest.xml has no package.")?;
    let jdk = jdk(session).await?;
    let sdk = Sdk::find(session, &jdk).await?;
    let serial = wait_until(READY, || sdk.online(target)).await;
    if let Some(webview) = sdk.webview(&serial).await
        && webview < WEBVIEW_MIN
    {
        say(format!(
            "This TV has WebView {webview}; StreamVault needs {WEBVIEW_MIN} or later: update Android System WebView from the Play Store."
        ));
    }
    let public = bundle::build(session).await?;
    let apk = Apk {
        session,
        sdk: &sdk,
        jdk: &jdk,
        public: &public,
    }
    .build(package, sync)
    .await?;
    session
        .run(sdk.adb().args(["-s", &serial, "install", "-r"]).arg(&apk))
        .await?;
    let activity = format!("{package}/.MainActivity");
    session
        .run(
            sdk.adb()
                .args(["-s", &serial, "shell", "am", "start", "-n", &activity]),
        )
        .await
}

fn user_sdk() -> PathBuf {
    std::env::var_os("ANDROID_HOME").map_or_else(os::android_home, PathBuf::from)
}

fn adb_in(sdk: &Path) -> PathBuf {
    binary(&sdk.join("platform-tools"), "adb")
}

fn version_code(version: &str) -> Option<u32> {
    let mut parts = version.split('.').map(|part| part.parse::<u32>().ok());
    Some(parts.next()?? * 10000 + parts.next()?? * 100 + parts.next()??)
}

fn webview_major(dump: &str) -> Option<u32> {
    let (_, version) = between(dump, WEBVIEW, ")")?.rsplit_once(", ")?;
    version.split('.').next()?.parse().ok()
}

async fn jdk(session: &Session) -> Result<PathBuf, String> {
    let candidates = std::env::var_os("JAVA_HOME")
        .map(PathBuf::from)
        .into_iter()
        .chain(os::system_java());
    for home in candidates {
        let mut javac = Command::new(binary(&home.join("bin"), "javac"));
        if attempt(javac.args(["--release", JAVA_RELEASE, "-version"])).await {
            return Ok(home);
        }
    }
    say("Installing a Java runtime…");
    let archive = session
        .download(
            &format!(
                "https://api.adoptium.net/v3/binary/latest/21/ga/{}/{ARCH}/jdk/hotspot/normal/eclipse",
                os::JDK_OS
            ),
            &format!("jdk.{}", os::ARCHIVE),
        )
        .await?;
    let dir = session.scratch.join("jdk");
    session.unpack(&archive, &dir, &[]).await?;
    let top = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.is_dir())
        .ok_or("The Java runtime archive is empty.")?;
    Ok(top.join(os::JDK_HOME))
}

fn java<'a>(command: &'a mut Command, jdk: &Path) -> &'a mut Command {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let paths = std::iter::once(jdk.join("bin")).chain(std::env::split_paths(&path));
    command
        .env("JAVA_HOME", jdk)
        .env("PATH", std::env::join_paths(paths).unwrap_or(path))
}

struct Sdk {
    root: PathBuf,
    adb: PathBuf,
}

impl Sdk {
    async fn find(session: &Session, jdk: &Path) -> Result<Self, String> {
        let user = user_sdk();
        let adb = adb_in(&user);
        if Self::complete(&user) {
            return Ok(Self { root: user, adb });
        }
        say("Installing the Android SDK tools…");
        let root = session.scratch.join("android");
        let url = format!(
            "https://dl.google.com/android/repository/commandlinetools-{}-{CMDLINE_TOOLS}_latest.zip",
            os::ANDROID_OS
        );
        let archive = session.download(&url, "cmdline-tools.zip").await?;
        let tools = root.join("cmdline-tools");
        session.unpack(&archive, &tools, &[]).await?;
        std::fs::rename(tools.join("cmdline-tools"), tools.join("latest"))
            .map_err(|e| e.to_string())?;
        let sdkmanager = || {
            let mut sdkmanager = Command::new(
                tools
                    .join("latest/bin")
                    .join(format!("sdkmanager{}", os::SCRIPT)),
            );
            java(&mut sdkmanager, jdk).arg(format!("--sdk_root={}", root.display()));
            sdkmanager
        };
        session
            .feed(sdkmanager().arg("--licenses"), &"y\n".repeat(LICENSES))
            .await?;
        let packages = root.join("packages.txt");
        write(
            &packages,
            format!("platform-tools\nbuild-tools;{BUILD_TOOLS}\nplatforms;android-{API}\n"),
        )?;
        session
            .run(sdkmanager().arg(format!("--package_file={}", packages.display())))
            .await?;
        if adb.is_file() {
            return Ok(Self { root, adb });
        }
        let adb = adb_in(&root);
        let server = adb.clone();
        session.on_exit(move || tidy(&server, &["kill-server"]));
        Ok(Self { root, adb })
    }

    fn complete(root: &Path) -> bool {
        build_tools(root)
            .join(format!("apksigner{}", os::SCRIPT))
            .is_file()
            && android_jar(root).is_file()
            && adb_in(root).is_file()
    }

    fn adb(&self) -> Command {
        Command::new(&self.adb)
    }

    fn tool(&self, name: &str) -> Command {
        Command::new(binary(&build_tools(&self.root), name))
    }

    fn script(&self, name: &str, jdk: &Path) -> Command {
        let mut script =
            Command::new(build_tools(&self.root).join(format!("{name}{}", os::SCRIPT)));
        java(&mut script, jdk);
        script
    }

    async fn online(&self, target: &str) -> Option<String> {
        let network = target.parse::<Ipv4Addr>().is_ok();
        let candidates = [
            Some(target.to_owned()),
            network.then(|| format!("{target}:{ADB}")),
            self.service(target, "connect").await,
        ];
        for candidate in candidates.into_iter().flatten() {
            if candidate.contains(':') {
                attempt(self.adb().args(["connect", &candidate])).await;
            }
            let state = self
                .adb()
                .args(["-s", &candidate, "get-state"])
                .output()
                .await;
            if state.is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "device")
            {
                return Some(candidate);
            }
        }
        if let Some(pairing) = self.service(target, "pairing").await
            && let Some(code) = ask("Pairing code shown on the TV:").await
        {
            attempt(self.adb().args(["pair", &pairing, &code])).await;
        }
        None
    }

    async fn service(&self, target: &str, kind: &str) -> Option<String> {
        let output = self.adb().args(["mdns", "services"]).output().await.ok()?;
        service(&String::from_utf8_lossy(&output.stdout), target, kind)
    }

    async fn webview(&self, serial: &str) -> Option<u32> {
        let output = self
            .adb()
            .args(["-s", serial, "shell", "dumpsys", "webviewupdate"])
            .output()
            .await
            .ok()?;
        webview_major(&String::from_utf8_lossy(&output.stdout))
    }
}

struct Apk<'a> {
    session: &'a Session,
    sdk: &'a Sdk,
    jdk: &'a Path,
    public: &'a Path,
}

impl Apk<'_> {
    async fn build(&self, package: &str, sync: Option<&SyncConfig>) -> Result<PathBuf, String> {
        let session = self.session;
        let code = version_code(VERSION)
            .ok_or_else(|| format!("{VERSION} is not a major.minor.patch version."))?;
        let work = session.out.join("android");
        let platform = android_jar(&self.sdk.root);
        bundle::stage(self.public, &work.join("assets/www"), sync)?;
        write(&work.join("res/drawable-xxxhdpi/icon.png"), ICON)?;
        write(&work.join("res/drawable/banner.xml"), BANNER)?;
        write(&work.join("AndroidManifest.xml"), MANIFEST)?;
        write(&work.join("MainActivity.java"), ACTIVITY)?;
        let keystore = self.keystore().await?;
        let (unsigned, aligned, apk) = (
            work.join("unsigned.apk"),
            work.join("aligned.apk"),
            session.out.join("StreamVault.apk"),
        );
        session
            .run(
                self.sdk
                    .tool("aapt2")
                    .arg("compile")
                    .arg("--dir")
                    .arg(work.join("res"))
                    .arg("-o")
                    .arg(work.join("res.zip")),
            )
            .await?;
        session
            .run(
                self.sdk
                    .tool("aapt2")
                    .arg("link")
                    .arg("-o")
                    .arg(&unsigned)
                    .arg("-I")
                    .arg(&platform)
                    .arg("-R")
                    .arg(work.join("res.zip"))
                    .arg("-A")
                    .arg(work.join("assets"))
                    .arg("--manifest")
                    .arg(work.join("AndroidManifest.xml"))
                    .args(["--min-sdk-version", MIN_API, "--target-sdk-version", API])
                    .args([
                        "--version-code",
                        &code.to_string(),
                        "--version-name",
                        VERSION,
                    ]),
            )
            .await?;
        let classes = work.join("classes");
        session
            .run(
                self.jdk_tool("javac")
                    .args(["--release", JAVA_RELEASE, "-classpath"])
                    .arg(&platform)
                    .arg("-d")
                    .arg(&classes)
                    .arg(work.join("MainActivity.java")),
            )
            .await?;
        let mut compiled: Vec<PathBuf> = std::fs::read_dir(classes.join(package.replace('.', "/")))
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|entry| entry.path())
            .collect();
        compiled.sort();
        let dex = work.join("dex");
        create(&dex)?;
        session
            .run(
                self.sdk
                    .script("d8", self.jdk)
                    .args(["--release", "--min-api", MIN_API, "--lib"])
                    .arg(&platform)
                    .arg("--output")
                    .arg(&dex)
                    .args(&compiled),
            )
            .await?;
        session
            .run(
                self.jdk_tool("jar")
                    .args(["--update", "--file"])
                    .arg(&unsigned)
                    .arg("-C")
                    .arg(&dex)
                    .arg("classes.dex"),
            )
            .await?;
        session
            .run(
                self.sdk
                    .tool("zipalign")
                    .args(["-f", "4"])
                    .arg(&unsigned)
                    .arg(&aligned),
            )
            .await?;
        session
            .run(
                self.sdk
                    .script("apksigner", self.jdk)
                    .args(["sign", "--ks"])
                    .arg(&keystore)
                    .arg("--ks-pass")
                    .arg(format!("pass:{DEBUG_PASSWORD}"))
                    .arg("--out")
                    .arg(&apk)
                    .arg(&aligned),
            )
            .await?;
        Ok(apk)
    }

    async fn keystore(&self) -> Result<PathBuf, String> {
        let dir = dirs::home_dir().unwrap_or_default().join(".android");
        let keystore = dir.join("debug.keystore");
        if keystore.is_file() {
            return Ok(keystore);
        }
        parent(&keystore)?;
        self.session
            .run(
                self.jdk_tool("keytool")
                    .args(["-genkeypair", "-keystore"])
                    .arg(&keystore)
                    .args([
                        "-storepass",
                        DEBUG_PASSWORD,
                        "-keypass",
                        DEBUG_PASSWORD,
                        "-alias",
                        "androiddebugkey",
                    ])
                    .args(["-keyalg", "RSA", "-keysize", "2048", "-validity", "10000"])
                    .args(["-dname", "CN=Android Debug,O=Android,C=US"]),
            )
            .await?;
        Ok(keystore)
    }

    fn jdk_tool(&self, name: &str) -> Command {
        Command::new(binary(&self.jdk.join("bin"), name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_code_orders_releases() {
        assert_eq!(version_code("1.1.4"), Some(10104));
        assert_eq!(version_code("2.0.13"), Some(20013));
        assert_eq!(version_code("1.1"), None);
        assert_eq!(version_code("1.x.0"), None);
    }

    #[test]
    fn usb_devices_are_listed_with_their_model() {
        let list = "List of devices attached\n\
            R58M12345   device usb:1-1 product:sabrina model:Chromecast device:sabrina transport_id:3\n\
            emulator-5554   offline transport_id:1\n\
            192.168.1.20:5555   device product:x model:Shield transport_id:2\n\
            ZY223   device transport_id:4\n";
        assert_eq!(
            devices(list),
            [
                ("R58M12345".to_owned(), "Chromecast".to_owned()),
                ("ZY223".to_owned(), "ZY223".to_owned()),
            ]
        );
    }

    #[test]
    fn a_wireless_debugging_service_is_found_by_address_and_kind() {
        let list = "List of discovered mdns services\n\
            adb-R58M-AbCd\t_adb-tls-pairing._tcp\t192.168.1.20:41235\n\
            adb-R58M-AbCd\t_adb-tls-connect._tcp\t192.168.1.20:37123\n\
            adb-Other-Ef\t_adb-tls-connect._tcp\t192.168.1.200:40000\n";
        assert_eq!(
            service(list, "192.168.1.20", "connect").as_deref(),
            Some("192.168.1.20:37123")
        );
        assert_eq!(
            service(list, "192.168.1.20", "pairing").as_deref(),
            Some("192.168.1.20:41235")
        );
        assert_eq!(service(list, "192.168.1.2", "connect"), None);
    }

    #[test]
    fn the_webview_major_version_is_read_from_dumpsys() {
        let dump = "Fallback logic enabled: true\nCurrent WebView package (name, version): (com.google.android.webview, 143.0.7499.40)\n";
        assert_eq!(webview_major(dump), Some(143));
        assert_eq!(webview_major("no webview"), None);
    }
}
