use super::*;

/// コーデックを保持するクラス
pub struct Codec {
    pub backend: i32,
    fourcc: i32,
    file_name: Option<String>,
}
impl AsRef<Codec> for Codec {
    fn as_ref(&self) -> &Codec { self }
}
impl Default for Codec {
    fn default() -> Self {
        Self {
            backend: videoio::CAP_ANY,
            fourcc: -1,
            file_name: None,
        }
    }
}
impl Codec {
    /// インストールされているコーデックを照合して初期化
    pub fn find_codec(file_name: Option<String>) -> Self {
        log::info!("find installed codec... {:?}", videoio::get_writer_backends());
        let fourcc_list = [b"HEVC", b"H265", b"X264", b"FMP4", b"ESDS", b"MP4V", b"MJPG"];
        let extension_list = ["mp4", "avi"];
        let file_name = file_name.unwrap_or("temp".to_string());
        let mut full_file_name = format!("{}.avi", file_name);
        let mut writer: videoio::VideoWriter = videoio::VideoWriter::default().unwrap();
        let mut codec = Self::default();
        // 環境によってインストールされている バックエンド/コーデック/対応する拡張子 が違うので特定
        'find_backends: for backend in videoio::get_writer_backends().unwrap() {
            if videoio::CAP_IMAGES == backend as i32 {
                // CAP_IMAGES だけ opencv のほうでエラーが出るので除外
                // (Rust 側の Error文でないので抑制できないし、消せないし邪魔)
                // error: (-5:Bad argument) CAP_IMAGES: can't find starting number (in the name of file): temp.avi in function 'cv::icvExtractPattern'
                continue;
            }

            for fourcc in fourcc_list.to_vec() {
                for extension in extension_list.to_vec() {
                    codec.backend = backend as i32;
                    codec.fourcc = i32::from_ne_bytes(*fourcc);
                    full_file_name = format!("{}.{}", file_name, extension);
    
                    let ret = writer.open_with_backend(
                        &full_file_name, codec.backend, codec.fourcc,
                        15.0, core::Size{width: 640, height: 360}, true
                    ).unwrap_or(false);
                    if ret {
                        log::info!("codec initialized: {:?} ({:?}) to {:?}", backend, std::str::from_utf8(fourcc).unwrap(), &file_name);
                        break 'find_backends;
                    }

                    codec.backend = videoio::CAP_ANY;
                    codec.fourcc = -1;
                    full_file_name = format!("{}.avi", file_name);
                }
            }
        }
        writer.release().ok();

        codec.file_name = Some(full_file_name);
        codec
    }

    /// コーデック情報に基づいて VideoWriter を初期化する
    pub fn open_writer_with_codec(&self, writer: &mut videoio::VideoWriter) -> opencv::Result<bool> {
        writer.open_with_backend(
            &self.file_name.clone().unwrap(), self.backend, self.fourcc,
            15.0, core::Size{width: 640, height: 360}, true
        )
    }

    /// コーデック情報に基づいて VideoCapture を初期化する
    pub fn open_reader_with_codec(&self, reader: &mut videoio::VideoCapture) -> opencv::Result<bool> {
        reader.open_file(&self.file_name.clone().unwrap(), self.backend)
    }
}
/// シングルトンでコーデックを保持するため
pub struct WrappedCodec {
    codecs: Option<Codec>,
}
impl WrappedCodec {
    // 参照して返さないと、unwrap() で move 違反がおきてちぬ！
    pub fn get(&mut self) -> &Codec {
        if self.codecs.is_none() {
            self.codecs = Some(Codec::find_codec(None));
        }
        self.codecs.as_ref().unwrap()
    }
}
static mut CODECS: WrappedCodec = WrappedCodec {
    codecs: None
};
pub fn codecs() -> &'static mut WrappedCodec {
    unsafe { &mut CODECS }
}
