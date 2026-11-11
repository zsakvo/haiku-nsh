//! Explicit offline acceptance fixture. No upstream requests or user data are used.
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
pub struct Server {
    pub url: String,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    pub fn start() -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let url = format!(
            "http://{}",
            listener.local_addr().map_err(|e| e.to_string())?
        );
        let audio = Arc::new(tone());
        let lyrics_calls = Arc::new(AtomicUsize::new(0));
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
                let mut request = [0; 8192];
                let count = stream.read(&mut request).unwrap_or(0);
                let request = String::from_utf8_lossy(&request[..count]).into_owned();
                if request.contains("/__stop") {
                    break;
                }
                let audio = audio.clone();
                let lyrics_calls = lyrics_calls.clone();
                let cover = cover();
                std::thread::spawn(move || {
                    let (body, mime) = if request.contains("/lyrics-count") {
                        (
                            lyrics_calls.load(Ordering::SeqCst).to_string().into_bytes(),
                            "text/plain",
                        )
                    } else if request.contains("/lyrics-error") {
                        (b"not valid JSON".to_vec(), "application/json")
                    } else if request.contains("/lyrics") {
                        lyrics_calls.fetch_add(1, Ordering::SeqCst);
                        let body = serde_json::json!({
                            "text": "[00:00]<00:00>潮汐<00:02>之间<00:04>\n[00:05]<00:05>听见<00:07>远方<00:09>\n[00:10]<00:10>风吹过<00:12>森林<00:14>\n[00:15]<00:15>向着<00:17>光亮<00:19>",
                        });
                        (body.to_string().into_bytes(), "application/json")
                    } else if request.contains("/cover.png") {
                        (cover, "image/png")
                    } else {
                        ((*audio).clone(), "audio/flac")
                    };
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(&body);
                });
            }
        });
        Ok(Self {
            url,
            thread: Some(thread),
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if let Ok(mut stream) = TcpStream::connect(self.url.trim_start_matches("http://")) {
            let _ = stream.write_all(b"GET /__stop HTTP/1.1\r\n\r\n");
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn crc(bytes: &[u8], bits: u32, polynomial: u32) -> u32 {
    let mut value = 0;
    for byte in bytes {
        value ^= (*byte as u32) << (bits - 8);
        for _ in 0..8 {
            value = if value & (1 << (bits - 1)) != 0 {
                (value << 1) ^ polynomial
            } else {
                value << 1
            };
        }
        value &= (1 << bits) - 1;
    }
    value
}
/// FLAC verbatim subframes: 60s of quiet 220Hz PCM, generated without an external encoder.
fn tone() -> Vec<u8> {
    let rate = 8000_u64;
    let samples = rate * 60;
    let mut out = b"fLaC\x80\0\0\x22".to_vec();
    out.extend_from_slice(&[0x10, 0, 0x10, 0, 0, 0, 0, 0, 0, 0]);
    out.extend_from_slice(&((rate << 44) | (15 << 36) | samples).to_be_bytes());
    out.extend_from_slice(&[0; 16]); // MD5 unknown (permitted by FLAC).
    let mut at = 0;
    let mut frame = 0_u32;
    while at < samples {
        let size = 4096_u64.min(samples - at);
        let mut bytes = vec![0xff, 0xf8, 0x70, 0x08];
        if frame < 128 {
            bytes.push(frame as u8);
        } else {
            bytes.extend_from_slice(&[(0xc0 | (frame >> 6)) as u8, (0x80 | (frame & 63)) as u8]);
        }
        bytes.extend_from_slice(&((size - 1) as u16).to_be_bytes());
        bytes.push(crc(&bytes, 8, 7) as u8);
        bytes.push(2); // VERBATIM, no wasted bits.
        for i in 0..size {
            let sample = (1200.
                * (std::f64::consts::TAU * 220. * (at + i) as f64 / rate as f64).sin())
                as i16;
            bytes.extend_from_slice(&sample.to_be_bytes());
        }
        bytes.extend_from_slice(&(crc(&bytes, 16, 0x8005) as u16).to_be_bytes());
        out.extend(bytes);
        at += size;
        frame += 1;
    }
    out
}
fn cover() -> Vec<u8> {
    use image::{ImageBuffer, ImageEncoder, Rgb};
    let image = ImageBuffer::from_fn(192, 192, |x, y| {
        // Fixture artwork is image data, independent of UI theme colors.
        Rgb([
            70 + (x / 3) as u8,
            100 + (y / 3) as u8,
            130 + ((x + y) / 8) as u8,
        ])
    });
    let mut bytes = vec![];
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(image.as_raw(), 192, 192, image::ExtendedColorType::Rgb8)
        .unwrap();
    bytes
}
