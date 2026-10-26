use super::{ICON, between, bundle, discover, os};
use crate::deploy::{
    Session, VERSION, attempt, binary, find, read, remove, say, tidy, wait_until, write,
};
use crate::settings::SyncConfig;
use percent_encoding::percent_decode_str;
use reqwest::multipart::{Form, Part};
use serde_json::Value;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;
use tokio::time::timeout;

pub const API: u16 = 8001;
const SDB: u16 = 26101;
const STUDIO: &str = "https://download.tizen.org/sdk/Installer/tizen-studio_6.1";
const LOGIN_PORT: u16 = 4794;
const LOGIN: &str = "https://account.samsung.com/mobile/account/check.do?serviceID=v285zxnl3h&actionID=StartOAuth2&accessToken=Y&redirect_uri=http://localhost:4794/signin/callback";
const ISSUER: &str = "https://svdca.samsungqbe.com/apis/v3";
const SIGNED_IN: &str = "Signed in: go back to the terminal.";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const ISSUE_TIMEOUT: Duration = Duration::from_secs(60);
const INSTALLED: &str = "successfully installed";
const KEY: &str = "UserCertificate";
const CONFIG: &str = include_str!("../../../resources/tv/tizen/config.xml");

pub async fn device(http: &reqwest::Client, ip: Ipv4Addr) -> Option<Value> {
    let info: Value =
        serde_json::from_str(&discover::get(http, ip, API, "/api/v2/").await?).ok()?;
    let device = info.get("device")?;
    (device["OS"] == "Tizen").then(|| device.clone())
}

pub async fn deploy(
    session: &Session,
    ip: Ipv4Addr,
    sync: Option<&SyncConfig>,
) -> Result<(), String> {
    let host = discover::local_ip(ip).ok_or("This computer has no address on the TV's network.")?;
    let studio = Studio::find(session).await?;
    let developer = format!(
        "On the TV: Apps, type 1 2 3 4 5, set Developer mode On with Host PC IP {host}, then restart the TV."
    );
    wait_until(&developer, || async move {
        developer_mode(&session.lan, ip, host).await.then_some(())
    })
    .await;
    let serial = format!("{ip}:{SDB}");
    let waiting = "Waiting for the TV to accept the connection (restart it if this lasts)…";
    wait_until(waiting, || async {
        studio.online(&serial).await.then_some(())
    })
    .await;
    let duid = session
        .output(studio.sdb().args(["-s", &serial, "shell", "0", "getduid"]))
        .await?;
    let certificate = Certificate::of(duid.trim());
    let public = bundle::build(session).await?;
    let app = between(CONFIG, "<tizen:application id=\"", "\"")
        .ok_or("config.xml has no application id.")?;
    attempt(
        studio
            .sdb()
            .args(["-s", &serial, "shell", "0", "was_kill", app]),
    )
    .await;
    let profile = studio.profile(session, &certificate).await?;
    let wgt = Package {
        session,
        studio: &studio,
        public: &public,
        sync,
    };
    let outcome = match &profile {
        Some(profile) => wgt.install(&serial, profile).await?,
        None => String::new(),
    };
    if !outcome.contains(INSTALLED) {
        let rejected = profile.is_none() || outcome.contains("ertificate");
        if profile.as_deref() == Some(certificate.name.as_str()) || !rejected {
            return Err(refused(&outcome));
        }
        certificate.create(session, &studio, &serial).await?;
        let outcome = wgt.install(&serial, &certificate.name).await?;
        if !outcome.contains(INSTALLED) {
            return Err(refused(&outcome));
        }
    }
    attempt(
        studio
            .sdb()
            .args(["-s", &serial, "shell", "0", "was_execute", app]),
    )
    .await;
    Ok(())
}

async fn developer_mode(http: &reqwest::Client, ip: Ipv4Addr, host: Ipv4Addr) -> bool {
    device(http, ip).await.is_some_and(|device| {
        device["developerMode"] == "1" && device["developerIP"] == host.to_string()
    })
}

fn refused(outcome: &str) -> String {
    let reason = outcome
        .lines()
        .find(|line| {
            ["fail", "error"]
                .iter()
                .any(|word| line.to_lowercase().contains(word))
        })
        .unwrap_or(outcome);
    format!("The TV refused the app: {reason}")
}

struct Studio {
    home: PathBuf,
    profiles: PathBuf,
}

impl Studio {
    async fn find(session: &Session) -> Result<Self, String> {
        let home = std::env::var_os("TIZEN_HOME").map_or_else(os::tizen_home, PathBuf::from);
        let installed = home.join("sdk.info").is_file();
        if !installed {
            say("Installing Tizen Studio (about 3 GB)…");
            os::install_tizen_studio(session, STUDIO, &home).await?;
            session
                .run(
                    Command::new(home.join("package-manager").join(os::PACKAGE_MANAGER)).args([
                        "install",
                        "--accept-license",
                        "cert-add-on",
                        "TV-SAMSUNG-Extension-Tools",
                    ]),
                )
                .await?;
        }
        let data = read(&home.join("sdk.info"))?
            .lines()
            .find_map(|line| line.strip_prefix("TIZEN_SDK_DATA_PATH="))
            .map(PathBuf::from)
            .ok_or("Tizen Studio has no data path in sdk.info.")?;
        let studio = Self {
            profiles: data.join("profile").join("profiles.xml"),
            home,
        };
        if !installed {
            let (home, sdb) = (studio.home.clone(), studio.sdb_path());
            session.on_exit(move || {
                tidy(&sdb, &["kill-server"]);
                remove(&home);
                remove(&data);
            });
        }
        Ok(studio)
    }

    fn cli(&self) -> Command {
        Command::new(
            self.home
                .join("tools/ide/bin")
                .join(format!("tizen{}", os::SCRIPT)),
        )
    }

    fn sdb(&self) -> Command {
        Command::new(self.sdb_path())
    }

    fn sdb_path(&self) -> PathBuf {
        binary(&self.home.join("tools"), "sdb")
    }

    fn keytool(&self) -> Command {
        Command::new(binary(
            &self.home.join("jdk").join(os::JDK_HOME).join("bin"),
            "keytool",
        ))
    }

    async fn online(&self, serial: &str) -> bool {
        attempt(self.sdb().args(["connect", serial])).await;
        self.sdb()
            .args(["-s", serial, "get-state"])
            .output()
            .await
            .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "device")
    }

    async fn profile(
        &self,
        session: &Session,
        certificate: &Certificate,
    ) -> Result<Option<String>, String> {
        let profiles = read(&self.profiles).unwrap_or_default();
        let known = profiles.contains(&format!("<profile name=\"{}\">", certificate.name));
        if known || certificate.register(session, self).await? {
            return Ok(Some(certificate.name.clone()));
        }
        Ok(between(&profiles, "<profiles active=\"", "\"")
            .filter(|active| !active.is_empty())
            .map(str::to_owned))
    }
}

struct Package<'a> {
    session: &'a Session,
    studio: &'a Studio,
    public: &'a Path,
    sync: Option<&'a SyncConfig>,
}

impl Package<'_> {
    async fn install(&self, serial: &str, profile: &str) -> Result<String, String> {
        let session = self.session;
        let stage = session.out.join("tizen");
        remove(&stage);
        bundle::stage(self.public, &stage, self.sync)?;
        let widget = format!("<widget version=\"{VERSION}\" ");
        write(
            &stage.join("config.xml"),
            CONFIG.replacen("<widget ", &widget, 1),
        )?;
        write(&stage.join("icon.png"), ICON)?;
        session
            .run(
                self.studio
                    .cli()
                    .args(["package", "-t", "wgt", "-s", profile, "-o"])
                    .arg(&session.out)
                    .arg("--")
                    .arg(&stage),
            )
            .await?;
        session
            .transcript(
                self.studio
                    .cli()
                    .args(["install", "-s", serial, "-n"])
                    .arg(session.out.join("StreamVault.wgt")),
            )
            .await
    }
}

struct Certificate {
    duid: String,
    name: String,
    dir: PathBuf,
}

impl Certificate {
    fn of(duid: &str) -> Self {
        let name = format!("streamvault-{duid}");
        let dir = dirs::home_dir()
            .unwrap_or_default()
            .join("SamsungCertificate")
            .join(&name);
        Self {
            duid: duid.to_owned(),
            name,
            dir,
        }
    }

    fn file(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    async fn register(&self, session: &Session, studio: &Studio) -> Result<bool, String> {
        let (author, distributor) = (self.file("author.p12"), self.file("distributor.p12"));
        let Ok(password) = read(&self.file("password")) else {
            return Ok(false);
        };
        if !author.is_file() || !distributor.is_file() {
            return Ok(false);
        }
        session
            .run(
                studio
                    .cli()
                    .args(["security-profiles", "add", "-n", &self.name, "-a"])
                    .arg(author)
                    .args(["-p", &password, "-d"])
                    .arg(distributor)
                    .args(["-dp", &password]),
            )
            .await?;
        Ok(true)
    }

    async fn create(&self, session: &Session, studio: &Studio, serial: &str) -> Result<(), String> {
        say(format!(
            "Sign in to your Samsung account in the browser: Samsung issues the certificate for this TV ({}).",
            self.duid
        ));
        let login = sign_in().await?;
        let field = |key: &str| login[key].as_str().unwrap_or_default().to_owned();
        let (token, user, email) = (
            field("access_token"),
            field("userId"),
            field("inputEmailID"),
        );
        if token.is_empty() || user.is_empty() {
            return Err("The Samsung sign-in returned no access token.".to_owned());
        }
        remove(&self.dir);
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| format!("Cannot create {}: {e}", self.dir.display()))?;
        os::private(&self.dir)?;
        let password = uuid::Uuid::new_v4().simple().to_string();
        let keys = Keys {
            session,
            studio,
            password: &password,
        };
        let author = keys
            .signing_request(&self.file("author.p12"), "CN=StreamVault", None)
            .await?;
        let san = format!(
            "SAN=uri:URN:tizen:packageid=,uri:URN:tizen:deviceid={}",
            self.duid
        );
        let subject = format!("EMAILADDRESS={}, CN=TizenSDK", escape(&email));
        let distributor = keys
            .signing_request(&self.file("distributor.p12"), &subject, Some(&san))
            .await?;
        let samsung = Issuer {
            session,
            token: &token,
            user: &user,
        };
        samsung
            .issue("authors", &self.file("author.crt"), &author, &[])
            .await?;
        let profile = self.file("device-profile.xml");
        let distribution = [
            ("privilege_level", "Public"),
            ("developer_type", "Individual"),
        ];
        samsung
            .issue("distributors", &profile, &distributor, &distribution)
            .await?;
        match between(&read(&profile)?, "<Certificate>", "</Certificate>") {
            Some(der) => write(
                &self.file("distributor.crt"),
                format!("-----BEGIN CERTIFICATE-----\n{der}\n-----END CERTIFICATE-----\n"),
            )?,
            None => std::fs::rename(&profile, self.file("distributor.crt"))
                .map_err(|e| e.to_string())?,
        }
        let plugins = studio.home.join("tools/certificate-manager");
        let authorities = find(&plugins, &|name| {
            name.starts_with("org.tizen.common.cert_") && name.ends_with(".jar")
        })
        .ok_or(format!(
            "Tizen Studio has no Samsung certificate authority in {}.",
            plugins.display()
        ))?;
        let ca = session.scratch.join("ca");
        let roots = [
            "res/ca/vd_tizen_dev_author_ca.cer",
            "res/ca/vd_tizen_dev_public2.crt",
        ];
        session.unpack(&authorities, &ca, &roots).await?;
        keys.answer(
            &self.file("author.p12"),
            &ca.join(roots[0]),
            &self.file("author.crt"),
        )
        .await?;
        keys.answer(
            &self.file("distributor.p12"),
            &ca.join(roots[1]),
            &self.file("distributor.crt"),
        )
        .await?;
        write(&self.file("password"), &password)?;
        self.register(session, studio).await?;
        if profile.is_file() {
            session
                .run(
                    studio
                        .sdb()
                        .args(["-s", serial, "push"])
                        .arg(&profile)
                        .arg("/home/owner/share/tmp/sdk_tools/device-profile.xml"),
                )
                .await?;
        }
        Ok(())
    }
}

struct Keys<'a> {
    session: &'a Session,
    studio: &'a Studio,
    password: &'a str,
}

impl Keys<'_> {
    fn keytool(&self, store: &Path) -> Command {
        let mut keytool = self.studio.keytool();
        keytool
            .args(["-noprompt", "-storetype", "PKCS12", "-keystore"])
            .arg(store)
            .args(["-storepass", self.password, "-keypass", self.password]);
        keytool
    }

    async fn signing_request(
        &self,
        store: &Path,
        subject: &str,
        extension: Option<&str>,
    ) -> Result<PathBuf, String> {
        self.session
            .run(
                self.keytool(store)
                    .args([
                        "-genkeypair",
                        "-alias",
                        KEY,
                        "-keyalg",
                        "RSA",
                        "-keysize",
                        "2048",
                    ])
                    .args(["-sigalg", "SHA512withRSA", "-dname", subject]),
            )
            .await?;
        let csr = store.with_extension("csr");
        let mut request = self.keytool(store);
        request
            .args([
                "-certreq",
                "-alias",
                KEY,
                "-sigalg",
                "SHA512withRSA",
                "-file",
            ])
            .arg(&csr);
        if let Some(extension) = extension {
            request.args(["-ext", extension]);
        }
        self.session.run(&mut request).await?;
        write(
            &csr,
            read(&csr)?.replace("NEW CERTIFICATE REQUEST", "CERTIFICATE REQUEST"),
        )?;
        Ok(csr)
    }

    async fn answer(
        &self,
        store: &Path,
        authority: &Path,
        certificate: &Path,
    ) -> Result<(), String> {
        let session = self.session;
        session
            .run(
                self.keytool(store)
                    .args(["-importcert", "-alias", "ca", "-file"])
                    .arg(authority),
            )
            .await?;
        session
            .run(
                self.keytool(store)
                    .args(["-importcert", "-alias", KEY, "-file"])
                    .arg(certificate),
            )
            .await?;
        session
            .run(self.keytool(store).args(["-delete", "-alias", "ca"]))
            .await
    }
}

struct Issuer<'a> {
    session: &'a Session,
    token: &'a str,
    user: &'a str,
}

impl Issuer<'_> {
    async fn issue(
        &self,
        endpoint: &str,
        answer: &Path,
        csr: &Path,
        fields: &[(&'static str, &'static str)],
    ) -> Result<(), String> {
        let request =
            std::fs::read(csr).map_err(|e| format!("Cannot read {}: {e}", csr.display()))?;
        let name = csr
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let part = Part::bytes(request)
            .file_name(name)
            .mime_str("application/octet-stream")
            .map_err(|e| e.to_string())?;
        let mut form = Form::new()
            .text("access_token", self.token.to_owned())
            .text("user_id", self.user.to_owned())
            .text("platform", "VD")
            .part("csr", part);
        for (key, value) in fields {
            form = form.text(*key, *value);
        }
        let response = self
            .session
            .http
            .post(format!("{ISSUER}/{endpoint}"))
            .multipart(form)
            .timeout(ISSUE_TIMEOUT)
            .send()
            .await
            .map_err(|e| format!("Samsung cannot be reached: {e}"))?;
        let status = response.status();
        let text = response.text().await.map_err(|e| e.to_string())?;
        write(answer, &text)?;
        if status == reqwest::StatusCode::OK {
            return Ok(());
        }
        let reason = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|answer| answer["error"]["description"].as_str().map(str::to_owned))
            .unwrap_or_else(|| text.chars().take(300).collect());
        Err(format!("Samsung refused the certificate: {reason}"))
    }
}

async fn sign_in() -> Result<Value, String> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, LOGIN_PORT))
        .await
        .map_err(|_| format!("Port {LOGIN_PORT} is in use; free it and run this again."))?;
    os::open(LOGIN)?;
    let reply = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{SIGNED_IN}",
        SIGNED_IN.len()
    );
    loop {
        let Ok((mut stream, _)) = listener.accept().await else {
            continue;
        };
        let Ok(Some((line, body))) = timeout(REQUEST_TIMEOUT, read_request(&mut stream)).await
        else {
            continue;
        };
        if stream.write_all(reply.as_bytes()).await.is_err() {
            say("The browser did not get the sign-in confirmation.");
        }
        if let Some(code) = login_code(&line, &body) {
            return serde_json::from_str(&code)
                .map_err(|e| format!("The Samsung sign-in answer is not valid: {e}"));
        }
    }
}

async fn read_request(stream: &mut TcpStream) -> Option<(String, String)> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await.ok()?;
    let mut length = 0;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).await.ok()? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().ok()?;
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await.ok()?;
    Some((
        line.trim_end().to_owned(),
        String::from_utf8_lossy(&body).into_owned(),
    ))
}

fn login_code(line: &str, body: &str) -> Option<String> {
    let source = if line.starts_with("GET ") { line } else { body };
    let code = source
        .split(['?', '&', ' '])
        .find_map(|part| part.strip_prefix("code="))?;
    percent_decode_str(&code.replace('+', " "))
        .decode_utf8()
        .ok()
        .map(Into::into)
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        if "\\,+\"<>;".contains(c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE: &str = "%7B%22access_token%22%3A%22abc%22%2C%22userId%22%3A%22u1%22%2C%22inputEmailID%22%3A%22a%2Bb%40c.d%22%7D";
    const LOGIN_JSON: &str = r#"{"access_token":"abc","userId":"u1","inputEmailID":"a+b@c.d"}"#;

    #[test]
    fn the_sign_in_code_comes_from_the_form_or_the_address() {
        let posted = login_code(
            "POST /signin/callback HTTP/1.1",
            &format!("state=x&code={CODE}"),
        );
        assert_eq!(posted.as_deref(), Some(LOGIN_JSON));
        let linked = login_code(
            &format!("GET /signin/callback?code={CODE}&state=x HTTP/1.1"),
            "",
        );
        assert_eq!(linked.as_deref(), Some(LOGIN_JSON));
        assert_eq!(login_code("GET /favicon.ico HTTP/1.1", ""), None);
        assert_eq!(login_code("POST / HTTP/1.1", "xcode=1"), None);
    }

    #[tokio::test]
    async fn a_posted_sign_in_is_read_whole() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let body = format!("code={CODE}");
        let client = tokio::spawn(async move {
            let mut stream = TcpStream::connect(address).await.unwrap();
            let request = format!(
                "POST /signin/callback HTTP/1.1\r\nHost: localhost\r\ncontent-length: {}\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(request.as_bytes()).await.unwrap();
        });
        let (mut stream, _) = listener.accept().await.unwrap();
        let (line, body) = read_request(&mut stream).await.unwrap();
        client.await.unwrap();
        assert_eq!(line, "POST /signin/callback HTTP/1.1");
        assert_eq!(login_code(&line, &body).as_deref(), Some(LOGIN_JSON));
    }

    #[test]
    fn subject_values_are_escaped() {
        assert_eq!(escape("a+b@c.d"), "a\\+b@c.d");
        assert_eq!(escape("plain@example.com"), "plain@example.com");
    }

    #[test]
    fn a_refusal_shows_the_failing_line() {
        assert_eq!(
            refused("Installing\nInstall failed: signature\nDone"),
            "The TV refused the app: Install failed: signature"
        );
        assert_eq!(refused("odd"), "The TV refused the app: odd");
    }
}
