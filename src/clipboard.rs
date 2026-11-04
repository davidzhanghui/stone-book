//! 直接读写 NSPasteboard。GPUI 自带的剪贴板只暴露纯文本 / 图片，
//! 而「从网页复制、粘贴即还原」需要拿到 public.html，复制时也要同时写出 HTML。

use objc2::rc::Retained;
use objc2_app_kit::{NSPasteboard, NSPasteboardType, NSPasteboardTypeHTML, NSPasteboardTypePNG, NSPasteboardTypeString, NSPasteboardTypeTIFF};
use objc2_foundation::{NSArray, NSData, NSString};

/// 石渠内部格式：块的 JSON，粘贴回石渠时 100% 无损。
const STONEBROOK_TYPE: &str = "app.stonebrook.blocks";

pub struct ClipboardContent {
    pub stonebrook: Option<String>,
    pub html: Option<String>,
    pub text: Option<String>,
    /// (字节, 扩展名)
    pub image: Option<(Vec<u8>, &'static str)>,
}

fn pasteboard() -> Retained<NSPasteboard> {
    NSPasteboard::generalPasteboard()
}

fn string_for(pb: &NSPasteboard, ty: &NSPasteboardType) -> Option<String> {
    pb.stringForType(ty).map(|s| s.to_string())
}

fn data_for(pb: &NSPasteboard, ty: &NSPasteboardType) -> Option<Vec<u8>> {
    pb.dataForType(ty).map(|d: Retained<NSData>| d.to_vec())
}

pub fn read() -> ClipboardContent {
    let pb = pasteboard();
    let custom = NSString::from_str(STONEBROOK_TYPE);
    let (html, text, png, tiff) = unsafe {
        (
            string_for(&pb, NSPasteboardTypeHTML),
            string_for(&pb, NSPasteboardTypeString),
            data_for(&pb, NSPasteboardTypePNG),
            data_for(&pb, NSPasteboardTypeTIFF),
        )
    };
    let image = png.map(|b| (b, "png")).or_else(|| {
        tiff.and_then(|b| {
            // TIFF 转 PNG，体积小且通用
            let img = image::load_from_memory(&b).ok()?;
            let mut out = std::io::Cursor::new(Vec::new());
            img.write_to(&mut out, image::ImageFormat::Png).ok()?;
            Some((out.into_inner(), "png"))
        })
    });
    ClipboardContent { stonebrook: string_for(&pb, &custom), html, text, image }
}

pub fn write(text: &str, html: Option<&str>, stonebrook_json: Option<&str>) {
    let pb = pasteboard();
    pb.clearContents();
    let custom = NSString::from_str(STONEBROOK_TYPE);
    unsafe {
        let mut types: Vec<&NSPasteboardType> = vec![NSPasteboardTypeString];
        if html.is_some() {
            types.push(NSPasteboardTypeHTML);
        }
        if stonebrook_json.is_some() {
            types.push(&custom);
        }
        let arr = NSArray::from_slice(&types);
        pb.declareTypes_owner(&arr, None);
        pb.setString_forType(&NSString::from_str(text), NSPasteboardTypeString);
        if let Some(h) = html {
            pb.setString_forType(&NSString::from_str(h), NSPasteboardTypeHTML);
        }
        if let Some(j) = stonebrook_json {
            pb.setString_forType(&NSString::from_str(j), &custom);
        }
    }
}

/// 写入 PNG 图片（「复制为图片」）。
pub fn write_png(png: &[u8], text: &str) {
    let pb = pasteboard();
    pb.clearContents();
    unsafe {
        let types: Vec<&NSPasteboardType> = vec![NSPasteboardTypePNG, NSPasteboardTypeString];
        let arr = NSArray::from_slice(&types);
        pb.declareTypes_owner(&arr, None);
        let data = objc2_foundation::NSData::with_bytes(png);
        pb.setData_forType(Some(&data), NSPasteboardTypePNG);
        pb.setString_forType(&NSString::from_str(text), NSPasteboardTypeString);
    }
}
