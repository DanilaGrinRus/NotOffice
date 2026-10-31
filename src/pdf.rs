//! Raster-only PDF reader. No browser, action/link dispatch, JavaScript engine,
//! form events, embedded-file extraction, shell execution or network APIs.
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};

pub const MAX_PDF_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_PAGES: usize = 2000;
pub type PageResult = (usize, Result<Vec<u8>, String>);
type OpenResult = Result<Vec<[f32; 2]>, String>;

pub struct PdfSession {
    snapshot: std::sync::Arc<Vec<u8>>,
    /// Native Windows page size in device-independent units, including rotation.
    pub pages: Vec<[f32; 2]>,
    requests: SyncSender<usize>,
    pub responses: Receiver<PageResult>,
    temp_pdf: Option<PathBuf>,
    _sandbox: Option<std::sync::Arc<crate::pdf_sandbox::Sandbox>>,
}

impl PdfSession {
    pub fn open(path: &Path) -> Result<Self, String> {
        Self::from_snapshot(std::sync::Arc::new(read_bounded(path)?))
    }

    pub fn open_isolated(path: &Path) -> Result<Self, String> {
        if cfg!(test) {
            return Self::open(path);
        }
        Self::from_snapshot_isolated(std::sync::Arc::new(read_bounded(path)?))
    }

    pub fn from_snapshot_isolated(bytes: std::sync::Arc<Vec<u8>>) -> Result<Self, String> {
        if cfg!(test) {
            return Self::from_snapshot(bytes);
        }
        #[cfg(not(windows))]
        {
            let _ = &bytes;
            return Err("PDF raster viewing requires Windows 10/11".into());
        }
        #[cfg(windows)]
        {
            if bytes.len() as u64 > MAX_PDF_BYTES
                || !bytes[..bytes.len().min(1024)]
                    .windows(5)
                    .any(|header| header == b"%PDF-")
            {
                return Err("Invalid or oversized PDF snapshot".into());
            }
            let sandbox = std::sync::Arc::new(crate::pdf_sandbox::Sandbox::new(&bytes)?);
            let pages = sandbox.probe()?;
            let (requests, queue) = mpsc::sync_channel(1);
            let (output, responses) = mpsc::channel();
            let worker = sandbox.clone();
            std::thread::Builder::new()
                .name("pdf-isolated-reader".into())
                .spawn(move || {
                    while let Ok(index) = queue.recv() {
                        let result = worker.render(index);
                        if output.send((index, result)).is_err() {
                            break;
                        }
                    }
                })
                .map_err(|error| format!("PDF isolated reader failed: {error}"))?;
            Ok(Self {
                snapshot: bytes,
                pages,
                requests,
                responses,
                temp_pdf: None,
                _sandbox: Some(sandbox),
            })
        }
    }

    pub fn snapshot(&self) -> std::sync::Arc<Vec<u8>> {
        self.snapshot.clone()
    }

    pub fn from_snapshot(bytes: std::sync::Arc<Vec<u8>>) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_PDF_BYTES
            || !bytes[..bytes.len().min(1024)]
                .windows(5)
                .any(|h| h == b"%PDF-")
        {
            return Err("Invalid or oversized PDF snapshot".into());
        }
        let snapshot = bytes.clone();
        let (requests, queue) = mpsc::sync_channel(1);
        let (ready, opened): (mpsc::SyncSender<OpenResult>, mpsc::Receiver<OpenResult>) =
            mpsc::sync_channel(1);
        let (output, responses) = mpsc::channel();
        std::thread::Builder::new()
            .name("pdf-raster-reader".into())
            .spawn(move || {
                #[cfg(windows)]
                native_worker(bytes, ready, queue, output);
                #[cfg(not(windows))]
                {
                    let _ = (bytes, queue, output);
                    let _ = ready.send(Err("PDF raster viewing requires Windows 10/11".into()));
                }
            })
            .map_err(|error| format!("PDF worker failed: {error}"))?;
        let pages = opened
            .recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|error| format!("PDF open did not finish within 10 seconds: {error}"))??;
        Ok(Self {
            snapshot,
            pages,
            requests,
            responses,
            temp_pdf: None,
            _sandbox: None,
        })
    }

    pub fn request_page(&self, index: usize) -> Result<(), String> {
        if index >= self.pages.len() {
            return Err("PDF page index out of range".into());
        }
        self.requests
            .try_send(index)
            .map_err(|error| format!("PDF render queue: {error}"))
    }
}

impl Drop for PdfSession {
    fn drop(&mut self) {
        if let Some(path) = self.temp_pdf.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|error| format!("PDF read failed: {error}"))?;
    if file.metadata().map_err(|error| error.to_string())?.len() > MAX_PDF_BYTES {
        return Err("PDF exceeds 64 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_PDF_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_PDF_BYTES {
        return Err("PDF grew beyond 64 MiB while reading".into());
    }
    if !bytes[..bytes.len().min(1024)]
        .windows(5)
        .any(|header| header == b"%PDF-")
    {
        return Err("Not a PDF: missing %PDF- header".into());
    }
    Ok(bytes)
}

#[cfg(any(windows, test))]
fn raster_size(size: [f32; 2]) -> Result<[u32; 2], String> {
    if size
        .iter()
        .any(|side| !side.is_finite() || *side <= 0.0 || *side > 100_000.0)
    {
        return Err("Invalid or excessive PDF page dimensions".into());
    }
    let scale = 2.0_f32.min(2000.0 / size[0].max(size[1]));
    Ok([
        (size[0] * scale).ceil().clamp(1.0, 2000.0) as u32,
        (size[1] * scale).ceil().clamp(1.0, 2000.0) as u32,
    ])
}

#[cfg(windows)]
fn native_worker(
    bytes: std::sync::Arc<Vec<u8>>,
    ready: SyncSender<Result<Vec<[f32; 2]>, String>>,
    queue: Receiver<usize>,
    output: mpsc::Sender<PageResult>,
) {
    use windows::Data::Pdf::{PdfDocument, PdfPageRenderOptions};
    use windows::Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream};
    use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe {
                RoUninitialize();
            }
        }
    }
    let loaded = (|| -> Result<_, String> {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(|error| error.to_string())?;
        let apartment = Apartment;
        let input = InMemoryRandomAccessStream::new().map_err(|error| error.to_string())?;
        let writer = DataWriter::CreateDataWriter(&input).map_err(|error| error.to_string())?;
        writer
            .WriteBytes(&bytes)
            .map_err(|error| error.to_string())?;
        writer
            .StoreAsync()
            .and_then(|operation| operation.join())
            .map_err(|error| error.to_string())?;
        writer.DetachStream().map_err(|error| error.to_string())?;
        input.Seek(0).map_err(|error| error.to_string())?;
        let document = PdfDocument::LoadFromStreamAsync(&input)
            .and_then(|operation| operation.join())
            .map_err(|error| {
                format!("PDF load failed (damaged/encrypted/unsupported PDF): {error}")
            })?;
        let count = document.PageCount().map_err(|error| error.to_string())? as usize;
        if count == 0 || count > MAX_PAGES {
            return Err("PDF must have 1–2000 pages".into());
        }
        let mut pages = Vec::with_capacity(count);
        for index in 0..count {
            let page = document
                .GetPage(index as u32)
                .map_err(|error| error.to_string())?;
            let size = page.Size().map_err(|error| error.to_string())?;
            page.Close().map_err(|error| error.to_string())?;
            raster_size([size.Width, size.Height])?;
            pages.push([size.Width, size.Height]);
        }
        Ok((apartment, input, document, pages))
    })();
    let (_apartment, _input, document, pages) = match loaded {
        Ok(loaded) => loaded,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    if ready.send(Ok(pages.clone())).is_err() {
        return;
    }
    for index in queue {
        let result = (|| -> Result<Vec<u8>, String> {
            let size = pages.get(index).ok_or("PDF page index out of range")?;
            let dimensions = raster_size(*size)?;
            let page = document
                .GetPage(index as u32)
                .map_err(|error| error.to_string())?;
            let rendered = (|| -> windows::core::Result<Vec<u8>> {
                let options = PdfPageRenderOptions::new()?;
                options.SetDestinationWidth(dimensions[0])?;
                options.SetDestinationHeight(dimensions[1])?;
                let stream = InMemoryRandomAccessStream::new()?;
                page.RenderWithOptionsToStreamAsync(&stream, &options)?
                    .join()?;
                let length = stream.Size()?;
                if length > 32 * 1024 * 1024 {
                    return Err(windows::core::Error::from(windows::core::HRESULT(
                        0x8007000E_u32 as i32,
                    )));
                }
                let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
                let loaded = reader.LoadAsync(length as u32)?.join()?;
                if u64::from(loaded) != length {
                    return Err(windows::core::Error::from(windows::core::HRESULT(
                        0x80070026_u32 as i32,
                    )));
                }
                let mut png = vec![0; loaded as usize];
                reader.ReadBytes(&mut png)?;
                Ok(png)
            })();
            let _ = page.Close();
            rendered.map_err(|error| format!("PDF page render failed: {error}"))
        })();
        if output.send((index, result)).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reject_wrong_header_oversized_files_and_invalid_raster_geometry() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("bad.pdf");
        std::fs::write(&path, b"not a PDF").unwrap();
        assert!(PdfSession::open(&path).is_err());
        File::create(&path)
            .unwrap()
            .set_len(MAX_PDF_BYTES + 1)
            .unwrap();
        assert!(PdfSession::open(&path).is_err());
        assert!(raster_size([f32::NAN, 10.0]).is_err());
        assert!(raster_size([0.0, 10.0]).is_err());
        assert_eq!(raster_size([3000.0, 6000.0]).unwrap(), [1000, 2000]);
    }

    #[test]
    fn raster_rounding_never_exceeds_the_decoder_or_cache_dimension_limit() {
        for whole in 1..100_000 {
            let extent = whole as f32 + 0.1;
            let size = raster_size([extent * 0.75, extent]).unwrap();
            assert!(
                size[0] <= 2000 && size[1] <= 2000,
                "extent={extent}, raster={size:?}"
            );
        }
    }
}
