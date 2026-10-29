use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use rsa::RsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::sha2::{Digest, Sha256};
use rsa::signature::{SignatureEncoding, Signer as _};
use std::io::{Cursor, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const DSIG: &str = "http://www.w3.org/2000/09/xmldsig#";
const EXCLUSIVE_C14N: &str = "http://www.w3.org/2001/10/xml-exc-c14n#";
const C14N_11: &str = "http://www.w3.org/2006/12/xml-c14n11";
const RSA_SHA256: &str = "http://www.w3.org/2001/04/xmldsig-more#rsa-sha256";
const DIGEST_SHA256: &str = "http://www.w3.org/2001/04/xmlenc#sha256";
const AUTHOR: &str = "author-signature.xml";
const DISTRIBUTOR: &str = "signature1.xml";
const LINE: usize = 76;
const URI_COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

pub struct Signer {
    key: SigningKey<Sha256>,
    pub(super) chain: Vec<Vec<u8>>,
}

impl Signer {
    pub fn new(key: &[u8], chain: Vec<Vec<u8>>) -> Result<Self, String> {
        let key = RsaPrivateKey::from_pkcs8_der(key)
            .map_err(|e| format!("The Samsung certificate key is not usable: {e}"))?;
        Ok(Self {
            key: SigningKey::new(key),
            chain,
        })
    }

    fn sign(&self, message: &[u8]) -> Result<String, String> {
        let signature = self
            .key
            .try_sign(message)
            .map_err(|e| format!("The package cannot be signed: {e}"))?;
        Ok(lines(&signature.to_bytes()))
    }
}

pub fn package(dir: &Path, author: &Signer, distributor: &Signer) -> Result<Vec<u8>, String> {
    let mut files = Vec::new();
    collect(dir, "", &mut files)?;
    files.sort();
    let signed = signature("AuthorSignature", "author", &files, author)?;
    files.push((AUTHOR.to_owned(), signed.into_bytes()));
    files.sort();
    let signed = signature("DistributorSignature", "distributor", &files, distributor)?;
    files.push((DISTRIBUTOR.to_owned(), signed.into_bytes()));
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, data) in &files {
        zip.start_file(name, options).map_err(|e| e.to_string())?;
        zip.write_all(data).map_err(|e| e.to_string())?;
    }
    let zip = zip.finish().map_err(|e| e.to_string())?;
    Ok(zip.into_inner())
}

fn collect(dir: &Path, prefix: &str, files: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("Cannot read {}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if name.starts_with('.') || [AUTHOR, DISTRIBUTOR].contains(&name.as_ref()) {
            continue;
        }
        let relative = format!("{prefix}{name}");
        if path.is_dir() {
            collect(&path, &format!("{relative}/"), files)?;
        } else {
            let data =
                std::fs::read(&path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
            files.push((relative, data));
        }
    }
    Ok(())
}

fn signature(
    id: &str,
    role: &str,
    files: &[(String, Vec<u8>)],
    signer: &Signer,
) -> Result<String, String> {
    let object = properties(id, role);
    let canonical_object = object.replacen("<Object ", &format!("<Object xmlns=\"{DSIG}\" "), 1);
    let mut references: String = files
        .iter()
        .map(|(name, data)| {
            reference(
                &utf8_percent_encode(name, URI_COMPONENT).to_string(),
                data,
                "",
            )
        })
        .collect();
    references.push_str(&reference(
        "#prop",
        canonical_object.as_bytes(),
        &format!("<Transforms>\n<Transform Algorithm=\"{C14N_11}\"></Transform>\n</Transforms>\n"),
    ));
    let signed_info = format!(
        "<SignedInfo>\n<CanonicalizationMethod Algorithm=\"{EXCLUSIVE_C14N}\"></CanonicalizationMethod>\n<SignatureMethod Algorithm=\"{RSA_SHA256}\"></SignatureMethod>\n{references}</SignedInfo>"
    );
    let canonical_info =
        signed_info.replacen("<SignedInfo>", &format!("<SignedInfo xmlns=\"{DSIG}\">"), 1);
    let value = signer.sign(canonical_info.as_bytes())?;
    let certificates: String = signer
        .chain
        .iter()
        .map(|der| format!("<X509Certificate>\n{}\n</X509Certificate>\n", lines(der)))
        .collect();
    Ok(format!(
        "<Signature xmlns=\"{DSIG}\" Id=\"{id}\">\n{signed_info}\n<SignatureValue>\n{value}\n</SignatureValue>\n<KeyInfo><X509Data>\n{certificates}</X509Data>\n</KeyInfo>\n{object}\n</Signature>\n"
    ))
}

fn reference(uri: &str, data: &[u8], transforms: &str) -> String {
    let value = STANDARD.encode(Sha256::digest(data));
    format!(
        "<Reference URI=\"{uri}\">\n{transforms}<DigestMethod Algorithm=\"{DIGEST_SHA256}\"></DigestMethod>\n<DigestValue>{value}</DigestValue>\n</Reference>\n"
    )
}

fn lines(data: &[u8]) -> String {
    let encoded = STANDARD.encode(data);
    let rows: Vec<_> = encoded
        .as_bytes()
        .chunks(LINE)
        .map(String::from_utf8_lossy)
        .collect();
    rows.join("\n")
}

fn properties(id: &str, role: &str) -> String {
    format!(
        "<Object Id=\"prop\"><SignatureProperties xmlns:dsp=\"http://www.w3.org/2009/xmldsig-properties\"><SignatureProperty Id=\"profile\" Target=\"#{id}\"><dsp:Profile URI=\"http://www.w3.org/ns/widgets-digsig#profile\"></dsp:Profile></SignatureProperty><SignatureProperty Id=\"role\" Target=\"#{id}\"><dsp:Role URI=\"http://www.w3.org/ns/widgets-digsig#role-{role}\"></dsp:Role></SignatureProperty><SignatureProperty Id=\"identifier\" Target=\"#{id}\"><dsp:Identifier></dsp:Identifier></SignatureProperty></SignatureProperties></Object>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::scratch_dir;
    use crate::deploy::tv::between;
    use crate::deploy::{remove, write};
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::pkcs8::EncodePrivateKey;
    use rsa::rand_core::OsRng;
    use rsa::signature::Verifier;
    use std::io::Read;
    use std::sync::LazyLock;
    use zip::ZipArchive;

    const CERTIFICATE: [u8; 200] = [7; 200];
    static KEY: LazyLock<RsaPrivateKey> =
        LazyLock::new(|| RsaPrivateKey::new(&mut OsRng, 2048).unwrap());

    fn signer() -> Signer {
        let key = KEY.to_pkcs8_der().unwrap();
        Signer::new(key.as_bytes(), vec![CERTIFICATE.to_vec()]).unwrap()
    }

    fn uris(xml: &str) -> Vec<&str> {
        xml.split("<Reference URI=\"")
            .skip(1)
            .filter_map(|reference| reference.split('"').next())
            .collect()
    }

    fn verify(xml: &str) {
        let info = between(xml, "<SignedInfo>", "</SignedInfo>").unwrap();
        let canonical = format!("<SignedInfo xmlns=\"{DSIG}\">{info}</SignedInfo>");
        let value = between(xml, "<SignatureValue>", "</SignatureValue>").unwrap();
        let value: String = value.split_whitespace().collect();
        let signature = Signature::try_from(STANDARD.decode(value).unwrap().as_slice()).unwrap();
        VerifyingKey::<Sha256>::new(KEY.to_public_key())
            .verify(canonical.as_bytes(), &signature)
            .unwrap();
    }

    #[test]
    fn the_prop_digest_is_the_one_samsungs_signer_publishes() {
        let signer = signer();
        let prop = |id, role| {
            let xml = signature(id, role, &[], &signer).unwrap();
            let reference = between(&xml, "<Reference URI=\"#prop\">", "</Reference>").unwrap();
            between(reference, "<DigestValue>", "</DigestValue>")
                .unwrap()
                .to_owned()
        };
        assert_eq!(
            prop("AuthorSignature", "author"),
            "lpo8tUDs054eLlBQXiDPVDVKfw30ZZdtkRs1jd7H5K8="
        );
        assert_eq!(
            prop("DistributorSignature", "distributor"),
            "u/jU3U4Zm5ihTMSjKGlGYbWzDfRkGphPPHx3gJIYEJ4="
        );
    }

    #[test]
    fn a_package_signs_every_file_for_the_author_then_the_distributor() {
        let dir = scratch_dir("widget_package");
        write(&dir.join("index.html"), "<html>").unwrap();
        write(&dir.join("js/app 1.js"), "x").unwrap();
        write(&dir.join(".DS_Store"), "junk").unwrap();
        write(&dir.join(DISTRIBUTOR), "stale").unwrap();
        let wgt = package(&dir, &signer(), &signer()).unwrap();
        remove(&dir);
        let mut zip = ZipArchive::new(Cursor::new(wgt)).unwrap();
        let names: Vec<String> = zip.file_names().map(|name| name.unwrap().into()).collect();
        assert_eq!(names, [AUTHOR, "index.html", "js/app 1.js", DISTRIBUTOR]);
        let mut text = |name: &str| {
            let mut text = String::new();
            zip.by_name(name)
                .unwrap()
                .read_to_string(&mut text)
                .unwrap();
            text
        };
        let (signed_by_author, signed_by_distributor) = (text(AUTHOR), text(DISTRIBUTOR));
        assert_eq!(
            uris(&signed_by_author),
            ["index.html", "js%2Fapp%201.js", "#prop"]
        );
        assert_eq!(
            uris(&signed_by_distributor),
            [AUTHOR, "index.html", "js%2Fapp%201.js", "#prop"]
        );
        let page = between(
            &signed_by_author,
            "<Reference URI=\"index.html\">",
            "</Reference>",
        );
        assert!(
            page.unwrap()
                .contains(&STANDARD.encode(Sha256::digest(b"<html>")))
        );
        assert!(signed_by_distributor.contains(&format!(
            "<X509Certificate>\n{}\n</X509Certificate>",
            lines(&CERTIFICATE)
        )));
        let base64 = |line: &&str| !line.starts_with('<');
        assert!(
            signed_by_author
                .lines()
                .filter(base64)
                .all(|line| line.len() <= LINE)
        );
        assert!(signed_by_distributor.contains("#role-distributor"));
        verify(&signed_by_author);
        verify(&signed_by_distributor);
    }
}
