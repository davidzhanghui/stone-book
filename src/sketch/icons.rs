//! 草图专用图标（24×24 描边图标，外层由 `ui::Assets` 包 `<svg stroke="#000" stroke-width="2">`）。
//! 造型参照 Excalidraw 属性面板 / 工具栏（tabler 风格）。名字统一以 `sk-` 开头。

pub fn body(name: &str) -> Option<&'static str> {
    Some(match name {
        // ── 填充 ──
        "sk-fill-hachure" => r##"<rect x="4" y="4" width="16" height="16" rx="2"/><path d="M4 12 12 4M4 20 20 4M12 20l8-8" stroke-width="1.6"/>"##,
        "sk-fill-cross" => {
            r##"<rect x="4" y="4" width="16" height="16" rx="2"/><path d="M4 12 12 4M4 20 20 4M12 20l8-8M12 4l8 8M4 4l16 16M4 12l8 8" stroke-width="1.3"/>"##
        }
        "sk-fill-solid" => r##"<rect x="4" y="4" width="16" height="16" rx="2" fill="#000"/>"##,
        "sk-fill-zigzag" => r##"<rect x="4" y="4" width="16" height="16" rx="2"/><path d="M6 9l3-3 3 3 3-3 3 3M6 15l3-3 3 3 3-3 3 3" stroke-width="1.5"/>"##,
        // ── 描边宽度 ──
        "sk-sw-thin" => r##"<path d="M5 12h14" stroke-width="1.25"/>"##,
        "sk-sw-bold" => r##"<path d="M5 12h14" stroke-width="2.5"/>"##,
        "sk-sw-extra" => r##"<path d="M5 12h14" stroke-width="4"/>"##,
        // ── 边框样式 ──
        "sk-ss-solid" => r##"<path d="M5 12h14"/>"##,
        "sk-ss-dashed" => r##"<path d="M5 12h3M10.5 12h3M16 12h3"/>"##,
        "sk-ss-dotted" => r##"<path d="M5 12h.01M8.5 12h.01M12 12h.01M15.5 12h.01M19 12h.01" stroke-width="2.2"/>"##,
        // ── 线条风格（粗糙度） ──
        "sk-rough-0" => r##"<path d="M4 14c3-3 5-3 7-1s4 2 7-2" stroke-width="1.6"/>"##,
        "sk-rough-1" => r##"<path d="M4 15c2-3 4-4 6-2s3 3 5 0 3-3 5-2M5 14.5c2-2.5 4-3.5 5.5-1.5" stroke-width="1.6"/>"##,
        "sk-rough-2" => {
            r##"<path d="M4 15c2-3 4-4 6-2s3 3 5 0 3-3 5-2M4.5 13.5c2-3 4-3.5 6-2s3 2.5 5 0 3-3 4.5-2.5M5 16c2-2 3.5-2.5 5-1.5" stroke-width="1.3"/>"##
        }
        // ── 边角 ──
        "sk-edge-sharp" => r##"<path d="M5 19V5h14" /><path d="M19 9v2M19 14v2M19 19h-2M14 19h-2M9 19h-2" stroke-width="1.5"/>"##,
        "sk-edge-round" => r##"<path d="M5 19v-9a5 5 0 0 1 5-5h9"/><path d="M19 9v2M19 14v2M19 19h-2M14 19h-2M9 19h-2" stroke-width="1.5"/>"##,
        // ── 箭头类型 ──
        "sk-arrow-sharp" => r##"<path d="M6 18 18 6M11 6h7v7"/>"##,
        "sk-arrow-curved" => r##"<path d="M6 19c0-7 4-12 12-12"/><path d="M14 4l4 3-4 3"/>"##,
        "sk-arrow-elbow" => r##"<path d="M5 18h6V8h8"/><path d="M16 5l3 3-3 3"/>"##,
        // ── 端点 ──
        "sk-head-none" => r##"<path d="M4 12h16"/>"##,
        "sk-head-arrow" => r##"<path d="M4 12h16M15 7l5 5-5 5"/>"##,
        "sk-head-triangle" => r##"<path d="M4 12h11"/><path d="M14 7l6 5-6 5z" fill="#000"/>"##,
        "sk-head-triangle_outline" => r##"<path d="M4 12h10"/><path d="M14 7l6 5-6 5z"/>"##,
        "sk-head-bar" => r##"<path d="M4 12h16M20 6v12"/>"##,
        "sk-head-dot" => r##"<path d="M4 12h12"/><circle cx="17" cy="12" r="3" fill="#000"/>"##,
        "sk-head-circle" => r##"<path d="M4 12h12"/><circle cx="17" cy="12" r="3" fill="#000"/>"##,
        "sk-head-circle_outline" => r##"<path d="M4 12h10"/><circle cx="17" cy="12" r="3"/>"##,
        "sk-head-diamond" => r##"<path d="M4 12h9"/><path d="M13 12l3.5-3.5L20 12l-3.5 3.5z" fill="#000"/>"##,
        "sk-head-diamond_outline" => r##"<path d="M4 12h9"/><path d="M13 12l3.5-3.5L20 12l-3.5 3.5z"/>"##,
        "sk-head-crowfoot_one" => r##"<path d="M4 12h16M17 7v10"/>"##,
        "sk-head-crowfoot_many" => r##"<path d="M4 12h16M14 12l6-5M14 12l6 5"/>"##,
        "sk-head-crowfoot_one_or_many" => r##"<path d="M4 12h16M14 12l6-5M14 12l6 5M12 7v10"/>"##,
        // ── 图层 ──
        "sk-send-back" => r##"<path d="M12 4v11M8 11l4 4 4-4M6 20h12"/>"##,
        "sk-send-backward" => r##"<path d="M12 5v14M7 14l5 5 5-5"/>"##,
        "sk-bring-forward" => r##"<path d="M12 19V5M7 10l5-5 5 5"/>"##,
        "sk-bring-front" => r##"<path d="M12 20V9M8 13l4-4 4 4M6 4h12"/>"##,
        // ── 操作 ──
        "sk-edit-line" => {
            r##"<path d="M6 18C8 9 12 7 18 6"/><rect x="3.5" y="16.5" width="4" height="4" rx="1"/><rect x="16.5" y="3.5" width="4" height="4" rx="1"/>"##
        }
        "sk-crop" => r##"<path d="M7 3v14h14"/><path d="M3 7h14v14"/>"##,
        // ── 字体 ──
        "sk-font-hand" => r##"<path d="M4 20h4L18.5 9.5a2.8 2.8 0 0 0-4-4L4 16v4"/><path d="M13.5 6.5l4 4"/>"##,
        "sk-font-normal" => r##"<path d="M7 20V9a5 5 0 0 1 10 0v11M7 13h10"/>"##,
        "sk-font-code" => r##"<path d="M8 8l-4 4 4 4M16 8l4 4-4 4M14 5l-4 14"/>"##,
        "sk-font-more" => r##"<path d="M5 20 12 4l7 16M8 13h8"/><path d="M8.5 20 12 11.5 15.5 20" stroke-width="1.2"/>"##,
        // ── 文字竖直对齐 ──
        "sk-valign-top" => r##"<path d="M5 4h14"/><rect x="9" y="8" width="6" height="11" rx="1.5"/>"##,
        "sk-valign-mid" => r##"<path d="M4 12h4M16 12h4"/><rect x="9" y="6" width="6" height="12" rx="1.5"/>"##,
        "sk-valign-bottom" => r##"<path d="M5 20h14"/><rect x="9" y="5" width="6" height="11" rx="1.5"/>"##,
        // ── 工具 ──
        "sk-sticky" => r##"<path d="M5 5h14v9l-5 5H5z"/><path d="M14 19v-5h5"/><path d="M8.5 9h7M8.5 12.5h4"/>"##,
        "sk-more" => r##"<circle cx="12" cy="5" r="1" fill="#000"/><circle cx="12" cy="12" r="1" fill="#000"/><circle cx="12" cy="19" r="1" fill="#000"/>"##,
        "sk-frame" => r##"<path d="M4 7h16M4 17h16M7 4v16M17 4v16"/>"##,
        "sk-draw-shape" => r##"<circle cx="9" cy="9" r="5"/><rect x="10" y="10" width="10" height="10" rx="2"/>"##,
        "sk-laser" => {
            r##"<path d="M10 14 4 20"/><path d="M14.5 9.5l-2 2 1 1 2-2zM13 6l1-3M18 11l3-1M17 7l2-2M15.5 4.5l.01 0M19.5 8.5l.01 0" stroke-width="1.6"/><path d="M8.5 15.5l-2-2 3-3 2 2z"/>"##
        }
        "sk-bucket" => {
            r##"<path d="M5 11.5 11.5 5l7 7-6.5 6.5a2 2 0 0 1-2.8 0L5 14.3a2 2 0 0 1 0-2.8z"/><path d="M5 12h13"/><path d="M20 15s1.5 2 1.5 3a1.5 1.5 0 0 1-3 0c0-1 1.5-3 1.5-3z"/>"##
        }
        "sk-lasso" => {
            r##"<path d="M7 17c-3-1.5-4-4-3-6.5C5.5 6.5 10 5 14 5.5c4.5.6 7 3 6.5 6s-4.5 4.5-9 4.5c-1.5 0-3-.3-4.5-.7"/><path d="M7 17c0 2 1 3 2.5 3.5"/><circle cx="7" cy="16.5" r="1.5"/>"##
        }
        "sk-mermaid" => r##"<path d="M4 5c0 6 4 9 8 9s8-3 8-9c-2 0-5 1-8 5-3-4-6-5-8-5z"/><path d="M12 14v6"/>"##,
        "sk-eyedropper" => r##"<path d="M11 7l6 6M4 20l1.5-.5L16 9l-1-1L4.5 18.5zM14 5l2-2a2.1 2.1 0 0 1 3 3l-2 2"/>"##,
        "sk-text" => r##"<path d="M6 6V5h12v1M12 5v14M9.5 19h5"/>"##,
        "sk-hash" => r##"<path d="M5 9h14M5 15h14M11 4 9 20M15 4l-2 16"/>"##,
        _ => return None,
    })
}
