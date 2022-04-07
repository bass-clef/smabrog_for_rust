use std::time::{Duration, Instant};
use super::*;

/// キャプチャ用のバッファ(動画ファイル[temp.*])を管理するクラス
pub struct CaptureFrameStore {
	writer: videoio::VideoWriter,
    reader: videoio::VideoCapture,

    prev_image: core::Mat,
    filled_by_frame: bool,
    pub recoded_frame: i32,

    recoding_start_time: Option<Instant>,
    recoding_end_time: Option<Instant>,
    recoding_need_frame: Option<i32>,
}
impl Default for CaptureFrameStore {
    fn default() -> Self {
        Self {
            writer: videoio::VideoWriter::default().unwrap(),
            reader: videoio::VideoCapture::default().unwrap(),

            prev_image: core::Mat::default(),
            filled_by_frame: false,
            recoded_frame: 0,

            recoding_start_time: None,
            recoding_end_time: None,
            recoding_need_frame: None,
        }
    }
}
impl CaptureFrameStore {
    // 録画開始時にする処理
    fn recoding_initialize(&mut self) -> opencv::Result<()> {
        self.recoding_start_time = None;
        self.recoding_end_time = None;
        self.recoding_need_frame = None;
        self.recoded_frame = 0;
        self.filled_by_frame = false;

        if self.reader.is_opened()? {
            // 開放しないと reader と writer で同じコーデックを使おうとする時に初期化できなくて怒られる
            self.reader.release()?;
        }

        if !codecs().get().open_writer_with_codec(&mut self.writer).unwrap_or(false) {
            return Err(opencv::Error::new( 0, "not found Codec for [*.mp4 or *.avi]. maybe: you install any Codec for your PC".to_string() ));
        }

        Ok(())
    }

    // 録画再生時前にする処理 (録画終了時に呼ばれる)
    fn replay_initialize(&mut self) -> opencv::Result<()> {
        if self.reader.is_opened()? {
            return Ok(());
        }

        if self.writer.is_opened()? {
            // 開放しないと reader と writer で同じコーデックを使おうとする時に初期化できなくて怒られる
            self.writer.release()?;
        }

        if !codecs().get().open_reader_with_codec(&mut self.reader).unwrap_or(false) {
            return Err(opencv::Error::new( 0, "not initialized video reader. maybe: playing temp video?".to_string() ));
        }

        self.filled_by_frame = true;

        Ok(())
    }

    // 録画再生終了時にする処理
    fn replay_finalize(&mut self) -> opencv::Result<()> {
        if !self.reader.is_opened()? {
            return Ok(());
        }

        self.reader.release()?;
        self.filled_by_frame = false;
        self.recoded_frame = -1;

        Ok(())
    }

    /// 録画が開始してるか
    pub fn is_recoding_started(&self) -> bool {
        if !self.writer.is_opened().unwrap() {
            return false;
        }

        if let Some(recoding_start_time) = self.recoding_start_time {
            return recoding_start_time <= Instant::now();
        }
        if let Some(recoding_need_frame) = self.recoding_need_frame {
            return 0 < recoding_need_frame;
        }
        
        // 開始時間が指定されていない or 必要なフレームが指定されていない
        false
    }

    /// 録画が終わってるか
    pub fn is_recoding_end(&self) -> bool {
        if !self.is_recoding_started() {
            return false;
        }

        if let Some(recoding_end_time) = self.recoding_end_time {
            return recoding_end_time <= Instant::now();
        }
        if let Some(recoding_need_frame) = self.recoding_need_frame {
            return recoding_need_frame <= self.recoded_frame;
        }
        
        // 終了時間より現在が後 or 必要なフレームが溜まった
        false
    }

    /// リプレイが終わってるか
    pub fn is_replay_end(&self) -> bool {
        -1 == self.recoded_frame
    }

    /// 録画が終わり、必要なフレームが満たされたか
    pub fn is_filled(&self) -> bool {
        self.filled_by_frame
    }

    /// now から end_time_duration 秒後まで録画しつづける
    pub fn start_recoding_by_time(&mut self, end_time_duration: Duration) {
        if self.is_recoding_started() {
            return;
        }

        self.recoding_initialize().unwrap();
        self.recoding_start_time = Some(Instant::now());
        self.recoding_end_time = Some(self.recoding_start_time.unwrap() + end_time_duration);
    }

    /// 必要なフレームがたまるまで録画しつづける
    pub fn start_recoding_by_frame(&mut self, need_frame: i32) {
        if self.is_recoding_started() {
            return;
        }

        self.recoding_initialize().unwrap();
        self.recoding_need_frame = Some(need_frame);
        self.recoded_frame = 0;
    }

    /// capture_image を[条件まで]ファイルへ録画する
    /// 条件 : start_recoding_by_time or start_recoding_by_frame
    pub fn recoding_frame(&mut self, capture_image: &core::Mat) -> opencv::Result<()> {
        if self.is_filled() {
            // フレームで満たされると reader を開いて replay_frame で空になるまで放置
            return Ok(());
        }
        if !self.is_recoding_started() {
            return Ok(());
        }

        // 動画を一度通すと BGR 同士で処理されなくなるので、色変換を行う。BGR to RGB
        let mut changed_color_capture_image = core::Mat::default();
        imgproc::cvt_color(&capture_image, &mut changed_color_capture_image, opencv::imgproc::COLOR_BGRA2RGBA, 0)?;

        self.writer.write(&changed_color_capture_image)?;
        self.recoded_frame += 1;

        if self.is_recoding_end() {
            self.replay_initialize()?;
        }
        Ok(())
    }

    /// 録画されたシーンを再生する, get_replay_frame: フレームを受け取るクロージャ
    /// replay_scene( |frame| { /* frame hogehoge */ } )?;
    pub fn replay_frame<F>(&mut self, mut get_replay_frame: F) -> opencv::Result<bool>
        where F: FnMut(&core::Mat) -> opencv::Result<bool>
    {
        if !self.reader.is_opened()? || !self.is_filled() || !self.reader.grab()? {
            // reader が開かれていない(録画されていない) or フレームで満たされていない or 準備が整ってない
            return Ok(false);
        }

        // 1 frame 取得
        self.reader.retrieve(&mut self.prev_image, 0)?;

        // フレームに対して行う処理
        if get_replay_frame(&self.prev_image)? {
            // true が帰ると replay を終了する
            self.recoded_frame = 1;
        }

        if 0 < self.recoded_frame {
            self.recoded_frame -= 1;
            if 0 == self.recoded_frame {
                self.replay_finalize()?;
            }
        }

        Ok(true)
    }
}
