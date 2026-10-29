use std::io::{Error, ErrorKind, Result};
use std::net::Ipv4Addr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

const PORT: u16 = 26101;
const VERSION: u32 = 0x0100_0000;
const MAX_DATA: usize = 1 << 20;
const SYNC_DATA: usize = 64 << 10;
const FILE_MODE: u32 = 0o100_644;
const BANNER: &[u8] = b"host::\0";
const HANDSHAKE: Duration = Duration::from_secs(5);
const ANSWER: Duration = Duration::from_secs(120);
const CNXN: u32 = command(b"CNXN");
const OPEN: u32 = command(b"OPEN");
const OKAY: u32 = command(b"OKAY");
const WRTE: u32 = command(b"WRTE");
const CLSE: u32 = command(b"CLSE");

const fn command(name: &[u8; 4]) -> u32 {
    u32::from_le_bytes(*name)
}

struct Message {
    command: u32,
    arg0: u32,
    arg1: u32,
    data: Vec<u8>,
}

struct Stream {
    local: u32,
    remote: u32,
}

pub struct Sdb {
    socket: TcpStream,
    max_data: usize,
    streams: u32,
}

impl Sdb {
    pub async fn connect(ip: Ipv4Addr) -> Result<Self> {
        Self::connect_to((ip, PORT)).await
    }

    async fn connect_to(address: impl tokio::net::ToSocketAddrs) -> Result<Self> {
        let socket = timeout(HANDSHAKE, TcpStream::connect(address)).await??;
        let mut sdb = Self {
            socket,
            max_data: MAX_DATA,
            streams: 0,
        };
        sdb.send(CNXN, VERSION, MAX_DATA as u32, BANNER).await?;
        loop {
            let message = sdb.receive(HANDSHAKE).await?;
            if message.command == CNXN {
                sdb.max_data = MAX_DATA.min(message.arg1 as usize);
                if sdb.max_data <= 8 {
                    return Err(Error::from(ErrorKind::InvalidData));
                }
                return Ok(sdb);
            }
        }
    }

    pub async fn shell(&mut self, command: &str) -> Result<String> {
        let stream = self.open(&format!("shell:{command}")).await?;
        let mut output = Vec::new();
        while let Some(data) = self.read(&stream).await? {
            output.extend(data);
        }
        Ok(String::from_utf8_lossy(&output).replace('\r', ""))
    }

    pub async fn push(&mut self, path: &str, data: &[u8]) -> Result<()> {
        let stream = self.open("sync:").await?;
        let target = format!("{path},{FILE_MODE}");
        self.write(&stream, &frame(b"SEND", target.as_bytes()))
            .await?;
        for chunk in data.chunks(SYNC_DATA.min(self.max_data - 8)) {
            self.write(&stream, &frame(b"DATA", chunk)).await?;
        }
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| u32::try_from(time.as_secs()).unwrap_or(u32::MAX));
        self.write(&stream, &[*b"DONE", time.to_le_bytes()].concat())
            .await?;
        let mut status = Vec::new();
        self.fill(&stream, &mut status, 8).await?;
        if !status.starts_with(b"OKAY") {
            let (words, _) = status.as_chunks::<4>();
            let end = 8 + u32::from_le_bytes(words[1]) as usize;
            self.fill(&stream, &mut status, end).await?;
            return Err(Error::other(format!(
                "the TV refused {path}: {}",
                String::from_utf8_lossy(&status[8..end])
            )));
        }
        self.write(&stream, &frame(b"QUIT", &[])).await
    }

    async fn open(&mut self, service: &str) -> Result<Stream> {
        self.streams += 1;
        let local = self.streams;
        let request = [service.as_bytes(), b"\0"].concat();
        self.send(OPEN, local, 0, &request).await?;
        loop {
            let message = self.next(local).await?;
            match message.command {
                OKAY => {
                    return Ok(Stream {
                        local,
                        remote: message.arg0,
                    });
                }
                CLSE => {
                    return Err(Error::new(
                        ErrorKind::ConnectionRefused,
                        format!("the TV refused {service}"),
                    ));
                }
                _ => {}
            }
        }
    }

    async fn write(&mut self, stream: &Stream, data: &[u8]) -> Result<()> {
        for chunk in data.chunks(self.max_data) {
            self.send(WRTE, stream.local, stream.remote, chunk).await?;
            loop {
                match self.next(stream.local).await?.command {
                    OKAY => break,
                    CLSE => return Err(Error::from(ErrorKind::ConnectionAborted)),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    async fn read(&mut self, stream: &Stream) -> Result<Option<Vec<u8>>> {
        loop {
            let message = self.next(stream.local).await?;
            match message.command {
                WRTE => {
                    self.send(OKAY, stream.local, stream.remote, &[]).await?;
                    return Ok(Some(message.data));
                }
                CLSE => return Ok(None),
                _ => {}
            }
        }
    }

    async fn fill(&mut self, stream: &Stream, data: &mut Vec<u8>, length: usize) -> Result<()> {
        while data.len() < length {
            let more = self.read(stream).await?;
            data.extend(more.ok_or(Error::from(ErrorKind::UnexpectedEof))?);
        }
        Ok(())
    }

    async fn next(&mut self, local: u32) -> Result<Message> {
        loop {
            let message = self.receive(ANSWER).await?;
            if message.arg1 == local {
                return Ok(message);
            }
        }
    }

    async fn send(&mut self, command: u32, arg0: u32, arg1: u32, data: &[u8]) -> Result<()> {
        let checksum = data
            .iter()
            .fold(0u32, |sum, byte| sum.wrapping_add(u32::from(*byte)));
        let length = u32::try_from(data.len()).map_err(Error::other)?;
        let header = [command, arg0, arg1, length, checksum, !command].map(u32::to_le_bytes);
        self.socket
            .write_all(&[header.concat().as_slice(), data].concat())
            .await
    }

    async fn receive(&mut self, wait: Duration) -> Result<Message> {
        timeout(wait, async {
            let mut header = [0; 24];
            self.socket.read_exact(&mut header).await?;
            let (words, _) = header.as_chunks::<4>();
            let field = |index: usize| u32::from_le_bytes(words[index]);
            let (command, length) = (field(0), field(3) as usize);
            if field(5) != !command || length > MAX_DATA {
                return Err(Error::from(ErrorKind::InvalidData));
            }
            let mut data = vec![0; length];
            self.socket.read_exact(&mut data).await?;
            Ok(Message {
                command,
                arg0: field(1),
                arg1: field(2),
                data,
            })
        })
        .await?
    }
}

fn frame(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let length = u32::try_from(data.len()).unwrap_or(u32::MAX);
    [id.as_slice(), &length.to_le_bytes(), data].concat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    struct Device {
        socket: TcpStream,
    }

    impl Device {
        async fn receive(&mut self) -> Message {
            let mut header = [0; 24];
            self.socket.read_exact(&mut header).await.unwrap();
            let (words, _) = header.as_chunks::<4>();
            let field = |i: usize| u32::from_le_bytes(words[i]);
            let mut data = vec![0; field(3) as usize];
            self.socket.read_exact(&mut data).await.unwrap();
            let checksum = data.iter().map(|b| u32::from(*b)).sum::<u32>();
            assert_eq!((field(4), field(5)), (checksum, !field(0)));
            Message {
                command: field(0),
                arg0: field(1),
                arg1: field(2),
                data,
            }
        }

        async fn send(&mut self, command: u32, arg0: u32, arg1: u32, data: &[u8]) {
            let length = data.len() as u32;
            let header = [command, arg0, arg1, length, 0, !command].map(u32::to_le_bytes);
            let packet = [header.concat().as_slice(), data].concat();
            self.socket.write_all(&packet).await.unwrap();
        }

        async fn accept(listener: &TcpListener, max_data: u32) -> Self {
            let (socket, _) = listener.accept().await.unwrap();
            let mut device = Self { socket };
            let hello = device.receive().await;
            assert_eq!((hello.command, hello.data.as_slice()), (CNXN, BANNER));
            device.send(CNXN, VERSION, max_data, b"device::\0").await;
            device
        }

        async fn open(&mut self, service: &str) -> u32 {
            let open = self.receive().await;
            assert_eq!(
                (open.command, open.data),
                (OPEN, [service.as_bytes(), b"\0"].concat())
            );
            self.send(OKAY, 7, open.arg0, &[]).await;
            open.arg0
        }

        async fn take(&mut self, host: u32) -> Vec<u8> {
            let write = self.receive().await;
            assert_eq!((write.command, write.arg0, write.arg1), (WRTE, host, 7));
            self.send(OKAY, 7, host, &[]).await;
            write.data
        }
    }

    async fn pair(max_data: u32) -> (Sdb, Device) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let (sdb, device) = tokio::join!(
            Sdb::connect_to(address),
            Device::accept(&listener, max_data)
        );
        (sdb.unwrap(), device)
    }

    #[tokio::test]
    async fn a_shell_command_returns_everything_the_tv_prints_until_it_closes() {
        let (mut sdb, mut device) = pair(4096).await;
        let tv = tokio::spawn(async move {
            let host = device.open("shell:0 getduid").await;
            for part in [&b"7XCI7KNS"[..], b"2VUXI\r\n"] {
                device.send(WRTE, 7, host, part).await;
                assert_eq!(device.receive().await.command, OKAY);
            }
            device.send(CLSE, 7, host, &[]).await;
        });
        assert_eq!(sdb.shell("0 getduid").await.unwrap(), "7XCI7KNS2VUXI\n");
        tv.await.unwrap();
    }

    #[tokio::test]
    async fn a_push_streams_the_file_through_sync_within_the_tv_payload_limit() {
        let (mut sdb, mut device) = pair(64).await;
        let file: Vec<u8> = (0..300u16).map(|i| i as u8).collect();
        let sent = file.clone();
        let tv = tokio::spawn(async move {
            let host = device.open("sync:").await;
            let mut stream = Vec::new();
            while !stream.windows(4).any(|window| window == b"DONE") {
                let chunk = device.take(host).await;
                assert!(chunk.len() <= 64);
                stream.extend(chunk);
            }
            device.send(WRTE, 7, host, b"OKAY\0\0\0\0").await;
            assert_eq!(device.receive().await.command, OKAY);
            assert_eq!(device.take(host).await, b"QUIT\0\0\0\0");
            stream
        });
        sdb.push("/tmp/app.wgt", &file).await.unwrap();
        let stream = tv.await.unwrap();
        let target = format!("/tmp/app.wgt,{FILE_MODE}");
        let mut expected = frame(b"SEND", target.as_bytes());
        for chunk in sent.chunks(56) {
            expected.extend(frame(b"DATA", chunk));
        }
        assert_eq!(&stream[..expected.len()], expected);
        assert_eq!(stream.len(), expected.len() + 8);
    }

    #[tokio::test]
    async fn a_refused_push_reports_the_tv_reason() {
        let (mut sdb, mut device) = pair(4096).await;
        let tv = tokio::spawn(async move {
            let host = device.open("sync:").await;
            let mut stream = Vec::new();
            while !stream.windows(4).any(|window| window == b"DONE") {
                stream.extend(device.take(host).await);
            }
            device
                .send(WRTE, 7, host, &frame(b"FAIL", b"read-only"))
                .await;
            assert_eq!(device.receive().await.command, OKAY);
        });
        let error = sdb.push("/tmp/app.wgt", b"wgt").await.unwrap_err();
        assert_eq!(error.to_string(), "the TV refused /tmp/app.wgt: read-only");
        tv.await.unwrap();
    }

    #[tokio::test]
    async fn a_refused_service_is_an_error() {
        let (mut sdb, mut device) = pair(4096).await;
        let tv = tokio::spawn(async move {
            let open = device.receive().await;
            device.send(CLSE, 0, open.arg0, &[]).await;
        });
        let error = sdb.shell("0 unknown").await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ConnectionRefused);
        tv.await.unwrap();
    }
}
