use super::os;
use super::widget::Signer;
use crate::deploy::{Session, read, remove, say, write};
use p12_keystore::{
    Certificate as Stored, KeyStore, KeyStoreEntry, Pkcs12ImportPolicy, PrivateKey, PrivateKeyChain,
};
use percent_encoding::percent_decode_str;
use reqwest::multipart::{Form, Part};
use rsa::RsaPrivateKey;
use rsa::pkcs1v15::{Signature, SigningKey};
use rsa::pkcs8::EncodePrivateKey;
use rsa::rand_core::OsRng;
use rsa::sha2::Sha512;
use serde_json::Value;
use std::error::Error;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use x509_cert::attr::AttributeTypeAndValue;
use x509_cert::builder::{Builder, RequestBuilder};
use x509_cert::der::asn1::{Any, Ia5String, Ia5StringRef, SetOfVec, Utf8StringRef};
use x509_cert::der::oid::db::{rfc3280::EMAIL_ADDRESS, rfc4519::CN};
use x509_cert::der::pem::LineEnding;
use x509_cert::der::referenced::OwnedToRef;
use x509_cert::der::{DecodePem, Encode, EncodePem};
use x509_cert::ext::pkix::name::GeneralName;
use x509_cert::ext::pkix::{SubjectAltName, SubjectKeyIdentifier};
use x509_cert::name::{RdnSequence, RelativeDistinguishedName};
use x509_cert::spki::SubjectPublicKeyInfoOwned;

const LOGIN_PORT: u16 = 4794;
const LOGIN: &str = "https://account.samsung.com/mobile/account/check.do?serviceID=v285zxnl3h&actionID=StartOAuth2&accessToken=Y&redirect_uri=http://localhost:4794/signin/callback";
const ISSUER: &str = "https://svdca.samsungqbe.com/apis/v3";
const SIGNED_IN: &str = "Signed in: go back to the terminal.";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const ISSUE_TIMEOUT: Duration = Duration::from_secs(60);
const KEY_ALIAS: &str = "UserCertificate";
const KEY_BITS: usize = 2048;
const AUTHOR_CA: &str = include_str!("../../../../resources/tv/tizen/vd_tizen_dev_author_ca.cer");
const DISTRIBUTOR_CA: &str =
    include_str!("../../../../resources/tv/tizen/vd_tizen_dev_public2.crt");
const DISTRIBUTION: [(&str, &str); 2] = [
    ("privilege_level", "Public"),
    ("developer_type", "Individual"),
];

pub struct Signers {
    pub author: Signer,
    pub distributor: Signer,
}

pub struct Certificate {
    duid: String,
    dir: PathBuf,
}

impl Certificate {
    pub fn of(duid: &str) -> Self {
        let dir = dirs::home_dir()
            .unwrap_or_default()
            .join("SamsungCertificate")
            .join(format!("streamvault-{duid}"));
        Self {
            duid: duid.to_owned(),
            dir,
        }
    }

    pub fn load(&self) -> Result<Option<Signers>, String> {
        let stores = [
            std::fs::read(self.dir.join("author.p12")),
            std::fs::read(self.dir.join("distributor.p12")),
        ];
        let (Ok(password), [Ok(author), Ok(distributor)]) =
            (read(&self.dir.join("password")), stores)
        else {
            return Ok(None);
        };
        Ok(Some(Signers {
            author: signer(&author, &password)?,
            distributor: signer(&distributor, &password)?,
        }))
    }

    pub async fn create(
        &self,
        session: &Session,
        previous: Option<&Signers>,
    ) -> Result<Signers, String> {
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
        let samsung = Issuer {
            session,
            token: &token,
            user: &user,
        };
        let author_key = match previous {
            Some(signers) => signers.author.key().clone(),
            None => generate()?,
        };
        let author = samsung
            .issue("authors", &request(&author_key, "StreamVault", None)?, &[])
            .await?;
        let distributor_key = generate()?;
        let distributor = Some((email.as_str(), self.duid.as_str()));
        let distributor = samsung
            .issue(
                "distributors",
                &request(&distributor_key, "TizenSDK", distributor)?,
                &DISTRIBUTION,
            )
            .await?;
        let password = uuid::Uuid::new_v4().simple().to_string();
        let author = [der(&author)?, der(AUTHOR_CA)?];
        let distributor = [der(&distributor)?, der(DISTRIBUTOR_CA)?];
        let stores = [
            ("author.p12", store(&author_key, &author, &password)?),
            (
                "distributor.p12",
                store(&distributor_key, &distributor, &password)?,
            ),
        ];
        remove(&self.dir);
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| format!("Cannot create {}: {e}", self.dir.display()))?;
        os::private(&self.dir)?;
        for (name, data) in stores {
            write(&self.dir.join(name), data)?;
        }
        write(&self.dir.join("password"), &password)?;
        self.load()?
            .ok_or_else(|| format!("{} holds no certificate.", self.dir.display()))
    }
}

fn generate() -> Result<RsaPrivateKey, String> {
    RsaPrivateKey::new(&mut OsRng, KEY_BITS).map_err(|e| e.to_string())
}

fn request(
    key: &RsaPrivateKey,
    name: &str,
    distributor: Option<(&str, &str)>,
) -> Result<String, String> {
    let build = || -> Result<String, Box<dyn Error>> {
        let mut subject = vec![AttributeTypeAndValue {
            oid: CN,
            value: Any::encode_from(&Utf8StringRef::new(name)?)?,
        }];
        if let Some((email, _)) = distributor {
            subject.push(AttributeTypeAndValue {
                oid: EMAIL_ADDRESS,
                value: Any::encode_from(&Ia5StringRef::new(email)?)?,
            });
        }
        let subject = subject
            .into_iter()
            .map(|attribute| SetOfVec::try_from(vec![attribute]).map(RelativeDistinguishedName))
            .collect::<Result<_, _>>()?;
        let signer = SigningKey::<Sha512>::new(key.clone());
        let mut builder = RequestBuilder::new(RdnSequence(subject), &signer)?;
        if let Some((_, duid)) = distributor {
            let names = [
                "URN:tizen:packageid=".to_owned(),
                format!("URN:tizen:deviceid={duid}"),
            ]
            .iter()
            .map(|uri| Ia5String::new(uri).map(GeneralName::UniformResourceIdentifier))
            .collect::<Result<_, _>>()?;
            builder.add_extension(&SubjectAltName(names))?;
        }
        let public = SubjectPublicKeyInfoOwned::from_key(key.to_public_key())?;
        builder.add_extension(&SubjectKeyIdentifier::try_from(public.owned_to_ref())?)?;
        Ok(builder.build::<Signature>()?.to_pem(LineEnding::LF)?)
    };
    build().map_err(|e| format!("The certificate request failed: {e}"))
}

fn store(key: &RsaPrivateKey, chain: &[Vec<u8>], password: &str) -> Result<Vec<u8>, String> {
    let chain = chain
        .iter()
        .map(|der| Stored::from_der(der).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let key = key.to_pkcs8_der().map_err(|e| e.to_string())?;
    let key = PrivateKey::from_der(key.as_bytes()).map_err(|e| e.to_string())?;
    let mut keys = KeyStore::new();
    keys.add_entry(
        KEY_ALIAS,
        KeyStoreEntry::PrivateKeyChain(PrivateKeyChain::new(KEY_ALIAS, key, chain)),
    );
    keys.writer(password).write().map_err(|e| e.to_string())
}

fn signer(store: &[u8], password: &str) -> Result<Signer, String> {
    let keys = KeyStore::from_pkcs12(store, password, Pkcs12ImportPolicy::Strict)
        .map_err(|e| format!("The Samsung certificate cannot be read: {e}"))?;
    let (_, chain) = keys
        .private_key_chain()
        .ok_or("The Samsung certificate has no key.")?;
    Signer::new(
        chain.key().as_der(),
        chain
            .certs()
            .iter()
            .map(|cert| cert.as_der().to_vec())
            .collect(),
    )
}

fn der(pem: &str) -> Result<Vec<u8>, String> {
    x509_cert::Certificate::from_pem(pem)
        .and_then(|certificate| certificate.to_der())
        .map_err(|e| {
            format!(
                "Samsung answered without a certificate ({e}): {}",
                preview(pem)
            )
        })
}

fn preview(text: &str) -> String {
    text.chars().take(300).collect()
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
        csr: &str,
        fields: &[(&'static str, &'static str)],
    ) -> Result<String, String> {
        let part = Part::bytes(csr.as_bytes().to_vec())
            .file_name(format!("{endpoint}.csr"))
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
        if status == reqwest::StatusCode::OK {
            return Ok(text);
        }
        let reason = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|answer| answer["error"]["description"].as_str().map(str::to_owned))
            .unwrap_or_else(|| preview(&text));
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

#[cfg(test)]
mod tests {
    use super::*;
    use p12_keystore::{EncryptionAlgorithm, MacAlgorithm};
    use rsa::pkcs1v15::VerifyingKey;
    use rsa::signature::Verifier;
    use std::sync::LazyLock;
    use x509_cert::builder::{CertificateBuilder, Profile};
    use x509_cert::der::Decode;
    use x509_cert::der::oid::db::rfc5912::SHA_512_WITH_RSA_ENCRYPTION;
    use x509_cert::name::Name;
    use x509_cert::request::CertReq;
    use x509_cert::serial_number::SerialNumber;
    use x509_cert::time::Validity;

    const CODE: &str = "%7B%22access_token%22%3A%22abc%22%2C%22userId%22%3A%22u1%22%2C%22inputEmailID%22%3A%22a%2Bb%40c.d%22%7D";
    const LOGIN_JSON: &str = r#"{"access_token":"abc","userId":"u1","inputEmailID":"a+b@c.d"}"#;

    static KEY: LazyLock<RsaPrivateKey> = LazyLock::new(|| generate().unwrap());

    fn issued_by(authority: &str) -> Vec<u8> {
        let authority = x509_cert::Certificate::from_der(&der(authority).unwrap()).unwrap();
        let profile = Profile::Leaf {
            issuer: authority.tbs_certificate.subject,
            enable_key_agreement: false,
            enable_key_encipherment: false,
        };
        let signer = SigningKey::<Sha512>::new(KEY.clone());
        let public = SubjectPublicKeyInfoOwned::from_key(KEY.to_public_key()).unwrap();
        CertificateBuilder::new(
            profile,
            SerialNumber::from(1u32),
            Validity::from_now(Duration::from_secs(60)).unwrap(),
            "CN=StreamVault".parse::<Name>().unwrap(),
            public,
            &signer,
        )
        .unwrap()
        .build::<Signature>()
        .unwrap()
        .to_der()
        .unwrap()
    }

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

    fn decoded(csr: &str) -> String {
        let csr = CertReq::from_pem(csr).unwrap();
        let signature = Signature::try_from(csr.signature.raw_bytes()).unwrap();
        VerifyingKey::<Sha512>::new(KEY.to_public_key())
            .verify(&csr.info.to_der().unwrap(), &signature)
            .unwrap();
        assert_eq!(csr.algorithm.oid, SHA_512_WITH_RSA_ENCRYPTION);
        String::from_utf8_lossy(&csr.to_der().unwrap()).into_owned()
    }

    #[test]
    fn only_the_distributor_request_names_the_account_and_the_tv() {
        let distributor = decoded(&request(&KEY, "TizenSDK", Some(("a+b@c.d", "7XCI"))).unwrap());
        for part in [
            "TizenSDK",
            "a+b@c.d",
            "URN:tizen:packageid=",
            "URN:tizen:deviceid=7XCI",
        ] {
            assert!(distributor.contains(part), "{part}");
        }
        let author = decoded(&request(&KEY, "StreamVault", None).unwrap());
        assert!(author.contains("StreamVault") && !author.contains("URN:tizen"));
    }

    #[test]
    fn only_a_certificate_counts_as_samsungs_answer() {
        assert!(der(AUTHOR_CA).is_ok() && der(DISTRIBUTOR_CA).is_ok());
        let refusal = der("{\"error\":1}").unwrap_err();
        assert!(refusal.starts_with("Samsung answered without a certificate"));
        assert!(refusal.ends_with("{\"error\":1}"));
    }

    #[test]
    fn a_store_gives_back_its_signer_with_the_samsung_chain() {
        let chain = [issued_by(AUTHOR_CA), der(AUTHOR_CA).unwrap()];
        let written = store(&KEY, &chain, "secret").unwrap();
        let loaded = signer(&written, "secret").unwrap();
        assert_eq!((&loaded.chain[..], loaded.key()), (&chain[..], &*KEY));
        assert!(signer(&written, "wrong").is_err());
    }

    #[test]
    fn stores_from_the_old_java_keytool_still_load() {
        let leaf = issued_by(DISTRIBUTOR_CA);
        let key = KEY.to_pkcs8_der().unwrap();
        let chain = PrivateKeyChain::new(
            "Time 1",
            PrivateKey::from_der(key.as_bytes()).unwrap(),
            [Stored::from_der(&leaf).unwrap()],
        );
        let mut keys = KeyStore::new();
        keys.add_entry(KEY_ALIAS, KeyStoreEntry::PrivateKeyChain(chain));
        for algorithm in [
            EncryptionAlgorithm::PbeWithShaAnd3KeyTripleDesCbc,
            EncryptionAlgorithm::PbeWithShaAnd40BitRc4Cbc,
        ] {
            let legacy = keys
                .writer("secret")
                .encryption_algorithm(algorithm)
                .mac_algorithm(MacAlgorithm::HmacSha1)
                .write()
                .unwrap();
            assert_eq!(
                signer(&legacy, "secret").unwrap().chain,
                std::slice::from_ref(&leaf)
            );
        }
    }
}
