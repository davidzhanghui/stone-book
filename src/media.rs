//! 视频播放（P7-2）：AVPlayer 负责解码与音频输出，
//! AVPlayerItemVideoOutput 每帧取出 CVPixelBuffer（硬件解码、零拷贝），交给 GPUI 的 surface() 绘制。

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep};
use objc2_av_foundation::{AVAsset, AVAssetImageGenerator, AVPlayer, AVPlayerItemVideoOutput};
use objc2_core_foundation::CGSize;
use objc2_core_media::CMTime;
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

pub struct Player {
    player: Retained<AVPlayer>,
    output: Retained<AVPlayerItemVideoOutput>,
    last: Option<core_video::pixel_buffer::CVPixelBuffer>,
    pub playing: bool,
    pub muted: bool,
    pub rate: f32,
}

thread_local! {
    static PLAYERS: RefCell<HashMap<u64, Player>> = RefCell::new(HashMap::new());
    /// 上一帧画出来的播放器（文档以外的界面，如电子表格附件预览，也靠它免于被自动暂停）
    static SEEN: RefCell<std::collections::HashSet<u64>> = RefCell::new(Default::default());
}

/// 文档以外（电子表格等）的播放器 id 都带这一位，不会与文档块 id 冲突。
pub const EXTERNAL: u64 = 1 << 63;

/// 按文件路径生成文档以外的播放器 id。
pub fn external_id(key: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    h.finish() | EXTERNAL
}

/// 播放器本帧可见。
pub fn mark_visible(id: u64) {
    SEEN.with(|s| s.borrow_mut().insert(id));
}

pub const RATES: [f32; 5] = [0.5, 1.0, 1.25, 1.5, 2.0];

fn cmtime(secs: f64) -> CMTime {
    unsafe { CMTime::with_seconds(secs, 600) }
}

fn create(path: &str) -> Option<Player> {
    let mtm = MainThreadMarker::new()?;
    unsafe {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        let player = AVPlayer::playerWithURL(&url, mtm);
        let item = player.currentItem()?;
        // GPUI 的 surface 只接受 420f（双平面 YCbCr，全范围）
        let fmt_key: &NSString = &*(objc2_core_video::kCVPixelBufferPixelFormatTypeKey as *const _ as *const NSString);
        let metal_key: &NSString = &*(objc2_core_video::kCVPixelBufferMetalCompatibilityKey as *const _ as *const NSString);
        let fmt = NSNumber::new_u32(objc2_core_video::kCVPixelFormatType_420YpCbCr8BiPlanarFullRange);
        let yes = NSNumber::new_bool(true);
        let attrs: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(&[fmt_key, metal_key], &[&*fmt as &AnyObject, &*yes as &AnyObject]);
        let output = AVPlayerItemVideoOutput::initWithPixelBufferAttributes(AVPlayerItemVideoOutput::alloc(), Some(&attrs));
        item.addOutput(&output);
        Some(Player { player, output, last: None, playing: false, muted: false, rate: 1.0 })
    }
}

pub fn with<R>(id: u64, path: &str, f: impl FnOnce(&mut Player) -> R) -> Option<R> {
    PLAYERS.with(|p| {
        let mut p = p.borrow_mut();
        if let std::collections::hash_map::Entry::Vacant(e) = p.entry(id) {
            e.insert(create(path)?);
        }
        p.get_mut(&id).map(f)
    })
}

pub fn exists(id: u64) -> bool {
    PLAYERS.with(|p| p.borrow().contains_key(&id))
}

impl Player {
    pub fn play(&mut self) {
        unsafe {
            if self.duration() > 0. && self.position() >= self.duration() - 0.05 {
                self.player.seekToTime(cmtime(0.));
            }
            self.player.play();
            self.player.setRate(self.rate);
        }
        self.playing = true;
    }

    pub fn pause(&mut self) {
        unsafe { self.player.pause() };
        self.playing = false;
    }

    pub fn toggle(&mut self) {
        if self.playing {
            self.pause()
        } else {
            self.play()
        }
    }

    pub fn seek(&mut self, secs: f64) {
        // 默认容差会吸附到最近的关键帧（关键帧稀疏的视频会跳回开头），所以要求精确定位
        unsafe { self.player.seekToTime_toleranceBefore_toleranceAfter(cmtime(secs.max(0.)), cmtime(0.), cmtime(0.)) };
    }

    pub fn position(&self) -> f64 {
        let s = unsafe { self.player.currentTime().seconds() };
        if s.is_finite() {
            s
        } else {
            0.
        }
    }

    pub fn duration(&self) -> f64 {
        let s = unsafe { self.player.currentItem().map(|i| i.duration().seconds()).unwrap_or(0.) };
        if s.is_finite() {
            s
        } else {
            0.
        }
    }

    pub fn set_rate(&mut self, r: f32) {
        self.rate = r;
        if self.playing {
            unsafe { self.player.setRate(r) };
        }
    }

    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        unsafe { self.player.setMuted(self.muted) };
    }

    /// 取当前帧（没有新帧时复用上一帧）。
    pub fn frame(&mut self) -> Option<core_video::pixel_buffer::CVPixelBuffer> {
        unsafe {
            let t = self.player.currentTime();
            if self.output.hasNewPixelBufferForItemTime(t) {
                if let Some(buf) = self.output.copyPixelBufferForItemTime_itemTimeForDisplay(t, std::ptr::null_mut()) {
                    use core_foundation::base::TCFType;
                    let raw = Retained::into_raw(buf) as *mut std::ffi::c_void;
                    self.last = Some(core_video::pixel_buffer::CVPixelBuffer::wrap_under_create_rule(raw as _));
                }
            }
        }
        self.tick();
        self.last.clone()
    }

    /// 播放到结尾时更新状态（音频没有视频帧，渲染时直接调用这个）。
    pub fn tick(&mut self) {
        if self.playing && self.duration() > 0. && self.position() >= self.duration() - 0.02 {
            self.playing = false;
        }
    }
}

/// 只允许一个视频同时播放。
pub fn pause_others(id: u64) {
    PLAYERS.with(|p| {
        for (k, pl) in p.borrow_mut().iter_mut() {
            if *k != id && pl.playing {
                pl.pause();
            }
        }
    });
}

/// 滚出可见区域的视频自动暂停（`visible` 之外，上一次调用以来 `mark_visible` 过的也算可见）。
pub fn pause_except(visible: &std::collections::HashSet<u64>) {
    let seen = SEEN.with(|s| std::mem::take(&mut *s.borrow_mut()));
    PLAYERS.with(|p| {
        for (k, pl) in p.borrow_mut().iter_mut() {
            if pl.playing && !visible.contains(k) && !seen.contains(k) {
                pl.pause();
            }
        }
    });
}

pub fn drop_player(id: u64) {
    PLAYERS.with(|p| {
        if let Some(mut pl) = p.borrow_mut().remove(&id) {
            pl.pause();
        }
    });
}

/// 生成封面帧（第 1 秒），返回 (PNG 路径, 宽, 高, 时长)。会阻塞，只在后台线程调用。
// 同步抽帧接口已被标为弃用（苹果建议用异步版避免卡住调用线程）；这里本来就在后台线程里，同步调用更简单。
#[allow(deprecated)]
pub fn poster(path: &str, out_dir: &Path) -> Option<(String, u32, u32, f64)> {
    unsafe {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        let asset = AVAsset::assetWithURL(&url);
        let gen = AVAssetImageGenerator::assetImageGeneratorWithAsset(&asset);
        gen.setAppliesPreferredTrackTransform(true);
        gen.setMaximumSize(CGSize::new(1920., 1920.));
        let cg = gen
            .copyCGImageAtTime_actualTime_error(cmtime(1.0), std::ptr::null_mut())
            .or_else(|_| gen.copyCGImageAtTime_actualTime_error(cmtime(0.), std::ptr::null_mut()))
            .ok()?;
        let rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cg);
        let w = rep.pixelsWide() as u32;
        let h = rep.pixelsHigh() as u32;
        let data = rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())?;
        let out = out_dir.join(format!("poster-{}.png", crate::webview::hash_key(&[path])));
        std::fs::write(&out, data.to_vec()).ok()?;
        let dur = {
            let s = asset.duration().seconds();
            if s.is_finite() {
                s
            } else {
                0.
            }
        };
        Some((out.to_string_lossy().to_string(), w, h, dur))
    }
}

pub fn fmt_time(s: f64) -> String {
    let s = s.max(0.) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

pub const VIDEO_EXTS: &[&str] = &["mp4", "mov", "m4v"];
