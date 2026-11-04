//! 电子表格工具栏图标（24×24 描边，仿在线表格工具栏）。名字统一以 `sh-` 开头，由 `ui::Assets` 兜底加载。

pub fn body(name: &str) -> Option<&'static str> {
    Some(match name {
        "sh-menu" => r##"<circle cx="12" cy="12" r="9"/><path d="M8 9h8"/><path d="M8 12h8"/><path d="M8 15h8"/>"##,
        "sh-undo" => r##"<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/>"##,
        "sh-redo" => r##"<path d="m15 14 5-5-5-5"/><path d="M20 9H9.5a5.5 5.5 0 0 0 0 11H13"/>"##,
        "sh-painter" => r##"<rect x="4" y="3" width="13" height="6" rx="1.5"/><path d="M17 6h3v5h-8v3"/><rect x="10.5" y="14" width="3" height="7" rx="1"/>"##,
        "sh-clear" => {
            r##"<path d="m7 21-4.3-4.3c-1-1-1-2.5 0-3.4l9.6-9.6c1-1 2.5-1 3.4 0l5.6 5.6c1 1 1 2.5 0 3.4L13 21"/><path d="M22 21H7"/><path d="m5 11 9 9"/>"##
        }
        "sh-insert" => r##"<circle cx="12" cy="12" r="9"/><path d="M12 8v8"/><path d="M8 12h8"/>"##,
        "sh-border" => r##"<rect x="4" y="4" width="16" height="16" rx="1"/><path d="M4 12h16"/><path d="M12 4v16"/>"##,
        "sh-b-all" => r##"<rect x="4" y="4" width="16" height="16"/><path d="M4 12h16"/><path d="M12 4v16"/>"##,
        "sh-b-outer" => r##"<rect x="4" y="4" width="16" height="16"/><path d="M12 7v1M12 11v2M12 16v1M7 12h1M11 12h2M16 12h1" opacity=".4"/>"##,
        "sh-b-inner" => r##"<path d="M4 12h16"/><path d="M12 4v16"/><rect x="4" y="4" width="16" height="16" stroke-dasharray="1 3" opacity=".4"/>"##,
        "sh-b-top" => r##"<path d="M4 4h16"/><path d="M4 8v12h16V8" stroke-dasharray="1 3" opacity=".4"/>"##,
        "sh-b-bottom" => r##"<path d="M4 20h16"/><path d="M4 16V4h16v12" stroke-dasharray="1 3" opacity=".4"/>"##,
        "sh-b-left" => r##"<path d="M4 4v16"/><path d="M8 4h12v16H8" stroke-dasharray="1 3" opacity=".4"/>"##,
        "sh-b-right" => r##"<path d="M20 4v16"/><path d="M16 4H4v16h12" stroke-dasharray="1 3" opacity=".4"/>"##,
        "sh-b-none" => {
            r##"<rect x="4" y="4" width="16" height="16" stroke-dasharray="1 3" opacity=".5"/><path d="M4 12h16M12 4v16" stroke-dasharray="1 3" opacity=".5"/>"##
        }
        "sh-a" => r##"<path d="m6 16 6-13 6 13"/><path d="M8.5 11h7"/>"##,
        "sh-bucket" => {
            r##"<path d="m17 10-7-7-7.2 7.2a1.6 1.6 0 0 0 0 2.3l4.7 4.7c.6.6 1.7.6 2.3 0Z"/><path d="m6 1 4 4"/><path d="M2.6 11h14"/><path d="M21 15.5c0 1-.8 1.8-1.8 1.8s-1.7-.8-1.7-1.8c0-1.2 1.7-3 1.7-3s1.8 1.8 1.8 3Z"/>"##
        }
        "sh-v-top" => r##"<path d="M5 3h14"/><path d="m8 11 4-4 4 4"/><path d="M12 7v14"/>"##,
        "sh-v-mid" => r##"<path d="M4 12h16"/><path d="M12 2v6"/><path d="m9 5 3 3 3-3"/><path d="M12 22v-6"/><path d="m9 19 3-3 3 3"/>"##,
        "sh-v-bottom" => r##"<path d="M5 21h14"/><path d="m8 13 4 4 4-4"/><path d="M12 17V3"/>"##,
        "sh-h-left" => r##"<path d="M4 6h16"/><path d="M4 12h10"/><path d="M4 18h16"/>"##,
        "sh-h-center" => r##"<path d="M4 6h16"/><path d="M7 12h10"/><path d="M4 18h16"/>"##,
        "sh-h-right" => r##"<path d="M4 6h16"/><path d="M10 12h10"/><path d="M4 18h16"/>"##,
        "sh-overflow" => r##"<path d="M4 4v16"/><path d="M8 12h12"/><path d="m16 8 4 4-4 4"/>"##,
        "sh-wrap" => r##"<path d="M4 4v16"/><path d="M8 7h8a4 4 0 0 1 0 8h-5"/><path d="m13 12-3 3 3 3"/>"##,
        "sh-merge" => {
            r##"<rect x="3" y="4" width="18" height="16" rx="1.5"/><path d="M12 4v4"/><path d="M12 16v4"/><path d="m7 12 3 0"/><path d="m14 12 3 0"/><path d="m8.5 10.5 1.5 1.5-1.5 1.5"/><path d="m15.5 10.5-1.5 1.5 1.5 1.5"/>"##
        }
        "sh-more" => {
            r##"<circle cx="5" cy="12" r="1.3" fill="#000"/><circle cx="12" cy="12" r="1.3" fill="#000"/><circle cx="19" cy="12" r="1.3" fill="#000"/>"##
        }
        "sh-find" => r##"<path d="M4 6h12"/><path d="M4 12h5"/><path d="M4 18h5"/><circle cx="15.5" cy="15" r="4"/><path d="m21 21-2.6-2.6"/>"##,
        "sh-filter" => r##"<path d="M4 5h16l-6 7.5V19l-4 2v-8.5z"/>"##,
        "sh-freeze" => {
            r##"<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18"/><path d="M9 3v18"/><path d="M15 13.5v5M12.8 16h4.4" opacity=".6"/>"##
        }
        "sh-cond" => r##"<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M7 8h4"/><path d="M7 12h7"/><path d="M7 16h10"/>"##,
        "sh-sort-asc" => r##"<path d="m3 8 4-4 4 4"/><path d="M7 4v16"/><path d="M14 6h7"/><path d="M14 12h5"/><path d="M14 18h3"/>"##,
        "sh-sort-desc" => r##"<path d="m3 16 4 4 4-4"/><path d="M7 20V4"/><path d="M14 6h3"/><path d="M14 12h5"/><path d="M14 18h7"/>"##,
        "sh-num" => r##"<path d="M4 9h16"/><path d="M4 15h16"/><path d="M10 3 8 21"/><path d="M16 3l-2 18"/>"##,
        "sh-percent" => r##"<path d="M19 5 5 19"/><circle cx="6.5" cy="6.5" r="2.5"/><circle cx="17.5" cy="17.5" r="2.5"/>"##,
        "sh-sigma" => r##"<path d="M18 7V5a1 1 0 0 0-1-1H6.5a.5.5 0 0 0-.4.8l4.5 6a2 2 0 0 1 0 2.4l-4.5 6a.5.5 0 0 0 .4.8H17a1 1 0 0 0 1-1v-2"/>"##,
        "sh-fx" => r##"<path d="M9 20c2 0 3-1 3.5-3.5L14 7.5C14.5 5 15.5 4 17.5 4"/><path d="M8 10h8"/>"##,
        "sh-eye-off" => {
            r##"<path d="M10.7 5.1A10.7 10.7 0 0 1 22 12a13 13 0 0 1-1.7 2.7"/><path d="M14.1 14.2a3 3 0 0 1-4.2-4.2"/><path d="M17.5 17.5A10.7 10.7 0 0 1 2 12a13 13 0 0 1 4.4-5.4"/><path d="m2 2 20 20"/>"##
        }
        "sh-eye" => r##"<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/>"##,
        "sh-grid" => r##"<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18"/><path d="M3 15h18"/><path d="M9 3v18"/><path d="M15 3v18"/>"##,
        "sh-keyboard" => r##"<rect x="2" y="5" width="20" height="14" rx="2"/><path d="M6 9h.01M10 9h.01M14 9h.01M18 9h.01M6 13h.01M18 13h.01M8 16h8"/>"##,
        "sh-exit" => r##"<path d="M18 6 6 18"/><path d="m6 6 12 12"/>"##,
        "sh-row-above" => r##"<rect x="3" y="11" width="18" height="10" rx="2"/><path d="M12 2v6"/><path d="M9 5h6"/>"##,
        "sh-row-below" => r##"<rect x="3" y="3" width="18" height="10" rx="2"/><path d="M12 16v6"/><path d="M9 19h6"/>"##,
        "sh-col-left" => r##"<rect x="11" y="3" width="10" height="18" rx="2"/><path d="M2 12h6"/><path d="M5 9v6"/>"##,
        "sh-col-right" => r##"<rect x="3" y="3" width="10" height="18" rx="2"/><path d="M16 12h6"/><path d="M19 9v6"/>"##,
        "sh-trash" => r##"<path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>"##,
        "sh-cut" => {
            r##"<circle cx="6" cy="6" r="3"/><path d="M8.12 8.12 12 12"/><path d="M20 4 8.12 15.88"/><circle cx="6" cy="18" r="3"/><path d="M14.8 14.8 20 20"/>"##
        }
        "sh-copy" => r##"<rect width="14" height="14" x="8" y="8" rx="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>"##,
        "sh-paste" => {
            r##"<rect x="8" y="2" width="8" height="4" rx="1"/><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/>"##
        }
        "sh-calendar" => r##"<path d="M8 2v4"/><path d="M16 2v4"/><rect width="18" height="18" x="3" y="4" rx="2"/><path d="M3 10h18"/>"##,
        "sh-clock" => r##"<circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/>"##,
        "sh-import" => r##"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/><path d="M12 15V3"/>"##,
        "sh-export" => r##"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m17 8-5-5-5 5"/><path d="M12 3v12"/>"##,
        "sh-list" => r##"<path d="M3 6h18"/><path d="M3 12h18"/><path d="M3 18h18"/>"##,
        "sh-height" => r##"<path d="M12 3v18"/><path d="m8 7 4-4 4 4"/><path d="m8 17 4 4 4-4"/>"##,
        "sh-width" => r##"<path d="M3 12h18"/><path d="m7 8-4 4 4 4"/><path d="m17 8 4 4-4 4"/>"##,
        "sh-dup" => r##"<rect x="8" y="8" width="13" height="13" rx="2"/><path d="M4 16V5a1 1 0 0 1 1-1h11"/><path d="M14.5 11.5v6M11.5 14.5h6"/>"##,
        "sh-rename" => r##"<path d="M12 20h9"/><path d="M16.4 3.6a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z"/>"##,
        "sh-left" => r##"<path d="m15 18-6-6 6-6"/>"##,
        "sh-right" => r##"<path d="m9 18 6-6-6-6"/>"##,
        "sh-dec-inc" => {
            r##"<path d="M3 17h.01"/><path d="M7 13a2 2 0 0 1 4 0v4a2 2 0 0 1-4 0Z"/><path d="M14 13a2 2 0 0 1 4 0v4a2 2 0 0 1-4 0Z"/><path d="M13 6h8"/><path d="m18 3 3 3-3 3"/>"##
        }
        "sh-dec-dec" => r##"<path d="M3 17h.01"/><path d="M8 13a2 2 0 0 1 4 0v4a2 2 0 0 1-4 0Z"/><path d="M21 6h-8"/><path d="m16 3-3 3 3 3"/>"##,
        "sh-link" => {
            r##"<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>"##
        }
        "sh-databar" => r##"<path d="M4 6h12"/><path d="M4 12h16"/><path d="M4 18h8"/>"##,
        "sh-scale" => {
            r##"<rect x="3" y="3" width="6" height="18" rx="1"/><rect x="9" y="3" width="6" height="18" rx="1" opacity=".6"/><rect x="15" y="3" width="6" height="18" rx="1" opacity=".3"/>"##
        }
        "sh-highlight" => r##"<rect x="3" y="3" width="18" height="18" rx="2"/><path d="m8 12 3 3 5-6"/>"##,
        "sh-check" => r##"<path d="M20 6 9 17l-5-5"/>"##,
        "sh-chevron" => r##"<path d="m6 9 6 6 6-6"/>"##,
        "sh-collapse" => r##"<path d="m18 15-6-6-6 6"/>"##,
        "sh-search" => r##"<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>"##,
        "sh-up" => r##"<path d="m18 15-6-6-6 6"/>"##,
        "sh-down" => r##"<path d="m6 9 6 6 6-6"/>"##,
        "sh-x" => r##"<path d="M18 6 6 18"/><path d="m6 6 12 12"/>"##,
        "sh-plus" => r##"<path d="M5 12h14"/><path d="M12 5v14"/>"##,
        "sh-i-up" => r##"<path fill="#000" stroke="none" d="M12 3 4 12h5v9h6v-9h5z"/>"##,
        "sh-i-down" => r##"<path fill="#000" stroke="none" d="M12 21 4 12h5V3h6v9h5z"/>"##,
        "sh-i-right" => r##"<path fill="#000" stroke="none" d="M21 12 12 4v5H3v6h9v5z"/>"##,
        "sh-i-angle-up" => r##"<path fill="#000" stroke="none" d="M19 5H8l3.5 3.5L4 16l4 4 7.5-7.5L19 16z"/>"##,
        "sh-i-angle-down" => r##"<path fill="#000" stroke="none" d="M19 19H8l3.5-3.5L4 8l4-4 7.5 7.5L19 8z"/>"##,
        "sh-i-circle" => r##"<circle fill="#000" stroke="none" cx="12" cy="12" r="8"/>"##,
        "sh-i-tri-up" => r##"<path fill="#000" stroke="none" d="M12 5 21 18H3z"/>"##,
        "sh-i-tri-down" => r##"<path fill="#000" stroke="none" d="M12 19 3 6h18z"/>"##,
        "sh-i-flat" => r##"<rect fill="#000" stroke="none" x="4" y="9" width="16" height="6" rx="1"/>"##,
        "sh-i-rhombus" => r##"<path fill="#000" stroke="none" d="M12 3 21 12 12 21 3 12z"/>"##,
        "sh-i-flag" => r##"<path fill="#000" stroke="none" d="M5 3h2v18H5z"/><path fill="#000" stroke="none" d="M7 4h11l-3 4 3 4H7z"/>"##,
        "sh-i-check" => r##"<path d="M4 12.5 9.5 18 20 6" stroke-width="3.2"/>"##,
        "sh-i-cross" => r##"<path d="M6 6l12 12M18 6 6 18" stroke-width="3.2"/>"##,
        "sh-i-excl" => r##"<path d="M12 4v10" stroke-width="3.4"/><circle fill="#000" stroke="none" cx="12" cy="19.5" r="2"/>"##,
        "sh-i-star" => r##"<path fill="#000" stroke="none" d="m12 2.5 2.9 6.2 6.6.8-4.9 4.6 1.3 6.6L12 17.4l-5.9 3.3 1.3-6.6-4.9-4.6 6.6-.8z"/>"##,
        "sh-i-heart" => r##"<path fill="#000" stroke="none" d="M12 21s-8-5.3-8-11a4.5 4.5 0 0 1 8-2.8A4.5 4.5 0 0 1 20 10c0 5.7-8 11-8 11z"/>"##,
        "sh-i-thumb-up" => {
            r##"<path fill="#000" stroke="none" d="M2 10h4v11H2zM8 10l4-8c1.7 0 3 1.3 3 3v3h5.5a2 2 0 0 1 2 2.3l-1.4 8.4A2 2 0 0 1 19.1 21H8z"/>"##
        }
        "sh-i-thumb-down" => {
            r##"<path fill="#000" stroke="none" d="M2 3h4v11H2zM8 14l4 8c1.7 0 3-1.3 3-3v-3h5.5a2 2 0 0 0 2-2.3l-1.4-8.4A2 2 0 0 0 19.1 3H8z"/>"##
        }
        "sh-minus" => r##"<path d="M5 12h14"/>"##,
        "sh-checkbox" => r##"<rect x="4" y="4" width="16" height="16" rx="2.5"/><path d="m8 12 3 3 5-6"/>"##,
        "sh-dropdown" => r##"<path d="M4 6h16"/><path d="M4 11h16"/><path d="m9 16 3 3 3-3"/>"##,
        "sh-date" => r##"<rect x="4" y="5" width="16" height="15" rx="2"/><path d="M4 10h16"/><path d="M9 3v4M15 3v4"/><path d="M8 14h2M14 14h2M8 17h2"/>"##,
        "sh-remind" => r##"<circle cx="12" cy="13" r="7"/><path d="M12 10v3l2 2"/><path d="M5 4 3 6M19 4l2 2"/>"##,
        "sh-image" => r##"<rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="10" r="2"/><path d="m21 16-5-5-9 9"/>"##,
        "sh-bars" => r##"<path d="M5 20v-6M10 20V9M15 20v-8M20 20V5"/>"##,
        "sh-tri-down" => r##"<path d="M6 9h12l-6 7z" fill="currentColor"/>"##,
        "sh-winloss" => r##"<path d="M3 12h18" opacity=".45"/><path d="M6 11V6M10 11V8M14 13v5M18 13v3" stroke-width="2.6"/>"##,
        "sh-line" => r##"<path d="m3 16 5-6 4 3 5-7 4 4"/>"##,
        "sh-spark" => r##"<path d="m3 17 5-6 4 4 5-8 4 5"/>"##,
        "sh-attach" => r##"<path d="m21 11-8.6 8.6a5 5 0 0 1-7-7l8.5-8.6a3.3 3.3 0 0 1 4.7 4.7L10 17.3a1.7 1.7 0 0 1-2.4-2.4l8-8"/>"##,
        "sh-note" => r##"<path d="M5 4h14v11l-5 5H5z"/><path d="M14 20v-5h5"/><path d="M8 9h8M8 12h5"/>"##,
        "sh-comment" => r##"<path d="M4 5h16v11H9l-5 4z"/><path d="M8 9h8M8 12h5"/>"##,
        "sh-unlink" => r##"<path d="M9 17H7a5 5 0 0 1 0-10h2"/><path d="M15 7h2a5 5 0 0 1 4 8"/><path d="M8 12h4"/><path d="m3 3 18 18"/>"##,
        "sh-clip" => r##"<path d="M4 4v16"/><path d="M20 4v16"/><path d="M8 12h8"/><path d="m13 9 3 3-3 3"/>"##,
        "sh-b-diag-down" => r##"<rect x="4" y="4" width="16" height="16" stroke-dasharray="1 3" opacity=".4"/><path d="m4 4 16 16"/>"##,
        "sh-b-diag-up" => r##"<rect x="4" y="4" width="16" height="16" stroke-dasharray="1 3" opacity=".4"/><path d="M4 20 20 4"/>"##,
        "sh-b-cv" => r##"<rect x="4" y="4" width="16" height="16" stroke-dasharray="1 3" opacity=".4"/><path d="M12 4v16"/>"##,
        "sh-b-ch" => r##"<rect x="4" y="4" width="16" height="16" stroke-dasharray="1 3" opacity=".4"/><path d="M4 12h16"/>"##,
        "sh-pen" => r##"<path d="M17 3l4 4L8 20H4v-4z"/>"##,
        "sh-split" => r##"<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M12 5v14"/><path d="m8 10-2 2 2 2M16 10l2 2-2 2"/>"##,
        "sh-validate" => r##"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 9h18"/><path d="m9 15 2 2 4-4"/>"##,
        "sh-clean" => r##"<path d="m14 4 6 6"/><path d="M3 21l7-2 9-9-5-5-9 9z"/><path d="M15 21h6"/>"##,
        "sh-xref" => {
            r##"<rect x="3" y="3" width="8" height="8" rx="1"/><rect x="13" y="13" width="8" height="8" rx="1"/><path d="M11 7h4a2 2 0 0 1 2 2v4"/><path d="m15 11 2 2 2-2"/>"##
        }
        "sh-view" => r##"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 9h18M9 9v11"/>"##,
        "sh-snap" => r##"<rect x="3" y="6" width="18" height="14" rx="2"/><circle cx="12" cy="13" r="3.5"/><path d="M8 6l2-3h4l2 3"/>"##,
        "sh-rows-fit" => {
            r##"<path d="M4 6h16M4 18h16"/><path d="m12 9-3 3 3 3"/><path d="M12 9l3 3-3 3" opacity="0"/><path d="M12 8v8"/><path d="m9 11 3-3 3 3M9 13l3 3 3-3"/>"##
        }
        "sh-cols-fit" => r##"<path d="M6 4v16M18 4v16"/><path d="M8 12h8"/><path d="m11 9-3 3 3 3M13 9l3 3-3 3"/>"##,
        _ => return None,
    })
}
