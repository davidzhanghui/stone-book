//! 明暗两套主题色。所有颜色集中在这里，界面代码只引用语义名。

use gpui::{rgb, rgba, App, Global, Hsla, WindowAppearance};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

#[derive(Clone)]
pub struct Theme {
    pub dark: bool,
    pub bg: Hsla,
    pub sidebar_bg: Hsla,
    pub panel_bg: Hsla,
    pub hover: Hsla,
    pub active: Hsla,
    pub border: Hsla,
    pub border_strong: Hsla,
    pub text: Hsla,
    pub text_secondary: Hsla,
    pub text_muted: Hsla,
    pub placeholder: Hsla,
    pub accent: Hsla,
    pub accent_soft: Hsla,
    pub selection: Hsla,
    pub cursor: Hsla,
    pub code_bg: Hsla,
    pub inline_code_bg: Hsla,
    pub inline_code_fg: Hsla,
    pub quote_bar: Hsla,
    pub link: Hsla,
    pub table_header: Hsla,
    pub find_match: Hsla,
    pub find_current: Hsla,
    pub overlay: Hsla,
    pub drop_line: Hsla,
    /// 文字颜色调色板（下标 1..=7）
    pub fg_palette: [Hsla; 8],
    /// 背景高亮调色板（下标 1..=7）
    pub bg_palette: [Hsla; 8],
    /// 深色背景（颜色面板第二行），对应 Marks::bg 8..15
    pub bg_strong: [Hsla; 8],
    /// 高亮块背景 / 边框
    pub callout_bg: [Hsla; 8],
    pub callout_border: [Hsla; 8],
    pub syntax: Syntax,
}

#[derive(Clone)]
pub struct Syntax {
    pub keyword: Hsla,
    pub string: Hsla,
    pub comment: Hsla,
    pub number: Hsla,
    pub function: Hsla,
    pub typ: Hsla,
}

impl Global for Theme {}

pub const PALETTE_NAMES: [&str; 8] = ["默认", "红色", "橙色", "黄色", "绿色", "蓝色", "紫色", "灰色"];

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}
fn ca(hex: u32) -> Hsla {
    rgba(hex).into()
}

impl Theme {
    /// 行内背景色：0..7 浅色，8..15 深色。
    pub fn text_bg(&self, i: u8) -> Hsla {
        if i >= 8 {
            self.bg_strong[(i - 8) as usize & 7]
        } else {
            self.bg_palette[i as usize]
        }
    }

    pub fn light() -> Self {
        Theme {
            dark: false,
            bg: c(0xffffff),
            sidebar_bg: c(0xf6f7f9),
            panel_bg: c(0xffffff),
            hover: ca(0x1f23290d),
            active: ca(0x3b6cf61f),
            border: c(0xe5e6eb),
            border_strong: c(0xd0d3d9),
            text: c(0x1f2329),
            text_secondary: c(0x646a73),
            text_muted: c(0x8f959e),
            placeholder: c(0xbbbfc4),
            accent: c(0x3b6cf6),
            accent_soft: ca(0x3b6cf61a),
            selection: ca(0x3b6cf633),
            cursor: c(0x1f2329),
            code_bg: c(0xf5f6f7),
            inline_code_bg: ca(0x1f23290f),
            inline_code_fg: c(0xd83931),
            quote_bar: c(0xbbbfc4),
            link: c(0x245bdb),
            table_header: c(0xf5f6f7),
            find_match: ca(0xffc60a66),
            find_current: ca(0xff8800aa),
            overlay: ca(0x00000033),
            drop_line: c(0x3b6cf6),
            fg_palette: [c(0x1f2329), c(0xd83931), c(0xde7802), c(0xdc9b04), c(0x2ea121), c(0x245bdb), c(0x6425d0), c(0x8f959e)],
            bg_strong: [c(0xdee0e3), c(0xbbbfc4), c(0xf76964), c(0xffa53d), c(0xffe928), c(0x62d256), c(0x94b4fa), c(0xc2a6f8)],
            bg_palette: [ca(0x00000000), c(0xfdddef), c(0xfeead2), c(0xfff5b8), c(0xd9f5d6), c(0xe1eaff), c(0xece2fe), c(0xeff0f1)],
            callout_bg: [c(0xf0f4ff), c(0xfef1f1), c(0xfff5eb), c(0xfefbe6), c(0xf0fbef), c(0xf0f4ff), c(0xf6f1fe), c(0xf5f6f7)],
            callout_border: [c(0xbacefd), c(0xfbbfbc), c(0xfed4a4), c(0xfff67a), c(0xb7edb1), c(0xbacefd), c(0xcdb2fa), c(0xdee0e3)],
            syntax: Syntax { keyword: c(0xa626a4), string: c(0x50a14f), comment: c(0xa0a1a7), number: c(0x986801), function: c(0x4078f2), typ: c(0xc18401) },
        }
    }

    pub fn dark() -> Self {
        Theme {
            dark: true,
            bg: c(0x1b1c1f),
            sidebar_bg: c(0x232428),
            panel_bg: c(0x2a2b30),
            hover: ca(0xffffff12),
            active: ca(0x5b8cff2e),
            border: c(0x34363b),
            border_strong: c(0x45484f),
            text: c(0xe3e4e6),
            text_secondary: c(0xa6a9ae),
            text_muted: c(0x7c8087),
            placeholder: c(0x5c6067),
            accent: c(0x5b8cff),
            accent_soft: ca(0x5b8cff26),
            selection: ca(0x5b8cff4d),
            cursor: c(0xe3e4e6),
            code_bg: c(0x26272b),
            inline_code_bg: ca(0xffffff14),
            inline_code_fg: c(0xff7b72),
            quote_bar: c(0x5c6067),
            link: c(0x6c9bff),
            table_header: c(0x26272b),
            find_match: ca(0xffc60a4d),
            find_current: ca(0xff8800aa),
            overlay: ca(0x00000066),
            drop_line: c(0x5b8cff),
            fg_palette: [c(0xe3e4e6), c(0xf76964), c(0xf5a339), c(0xf7d14b), c(0x63c45a), c(0x6c9bff), c(0xb18bf5), c(0x8a8e95)],
            bg_strong: [ca(0x8f959e66), ca(0x8f959ea0), ca(0xf76964a0), ca(0xffa53da0), ca(0xffe928a0), ca(0x62d256a0), ca(0x6c9bffa0), ca(0xb18bf5a0)],
            bg_palette: [ca(0x00000000), ca(0xf7696440), ca(0xf5a33940), ca(0xf7d14b40), ca(0x63c45a40), ca(0x6c9bff40), ca(0xb18bf540), ca(0x8a8e9540)],
            callout_bg: [ca(0x5b8cff1f), ca(0xf769641f), ca(0xf5a3391f), ca(0xf7d14b1f), ca(0x63c45a1f), ca(0x5b8cff1f), ca(0xb18bf51f), ca(0xffffff0f)],
            callout_border: [ca(0x5b8cff55), ca(0xf7696455), ca(0xf5a33955), ca(0xf7d14b55), ca(0x63c45a55), ca(0x5b8cff55), ca(0xb18bf555), ca(0xffffff22)],
            syntax: Syntax { keyword: c(0xc678dd), string: c(0x98c379), comment: c(0x7f848e), number: c(0xd19a66), function: c(0x61afef), typ: c(0xe5c07b) },
        }
    }

    pub fn for_mode(mode: ThemeMode, appearance: WindowAppearance) -> Self {
        let dark = match mode {
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
            ThemeMode::System => {
                matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark)
            }
        };
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }
}

pub fn theme(cx: &App) -> &Theme {
    cx.global::<Theme>()
}
