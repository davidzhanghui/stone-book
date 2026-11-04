//! 内嵌的 KaTeX 0.16.11 发行文件（JS / CSS / woff2 字体），首次使用时释放到数据目录。

pub const VERSION: &str = "0.16.11";

pub const FILES: &[(&str, &[u8])] = &[
    ("katex.min.js", include_bytes!("../assets/katex/katex.min.js")),
    ("katex.min.css", include_bytes!("../assets/katex/katex.min.css")),
    ("fonts/KaTeX_AMS-Regular.woff2", include_bytes!("../assets/katex/fonts/KaTeX_AMS-Regular.woff2")),
    ("fonts/KaTeX_Caligraphic-Bold.woff2", include_bytes!("../assets/katex/fonts/KaTeX_Caligraphic-Bold.woff2")),
    ("fonts/KaTeX_Caligraphic-Regular.woff2", include_bytes!("../assets/katex/fonts/KaTeX_Caligraphic-Regular.woff2")),
    ("fonts/KaTeX_Fraktur-Bold.woff2", include_bytes!("../assets/katex/fonts/KaTeX_Fraktur-Bold.woff2")),
    ("fonts/KaTeX_Fraktur-Regular.woff2", include_bytes!("../assets/katex/fonts/KaTeX_Fraktur-Regular.woff2")),
    ("fonts/KaTeX_Main-Bold.woff2", include_bytes!("../assets/katex/fonts/KaTeX_Main-Bold.woff2")),
    ("fonts/KaTeX_Main-BoldItalic.woff2", include_bytes!("../assets/katex/fonts/KaTeX_Main-BoldItalic.woff2")),
    ("fonts/KaTeX_Main-Italic.woff2", include_bytes!("../assets/katex/fonts/KaTeX_Main-Italic.woff2")),
    ("fonts/KaTeX_Main-Regular.woff2", include_bytes!("../assets/katex/fonts/KaTeX_Main-Regular.woff2")),
    ("fonts/KaTeX_Math-BoldItalic.woff2", include_bytes!("../assets/katex/fonts/KaTeX_Math-BoldItalic.woff2")),
    ("fonts/KaTeX_Math-Italic.woff2", include_bytes!("../assets/katex/fonts/KaTeX_Math-Italic.woff2")),
    ("fonts/KaTeX_SansSerif-Bold.woff2", include_bytes!("../assets/katex/fonts/KaTeX_SansSerif-Bold.woff2")),
    ("fonts/KaTeX_SansSerif-Italic.woff2", include_bytes!("../assets/katex/fonts/KaTeX_SansSerif-Italic.woff2")),
    ("fonts/KaTeX_SansSerif-Regular.woff2", include_bytes!("../assets/katex/fonts/KaTeX_SansSerif-Regular.woff2")),
    ("fonts/KaTeX_Script-Regular.woff2", include_bytes!("../assets/katex/fonts/KaTeX_Script-Regular.woff2")),
    ("fonts/KaTeX_Size1-Regular.woff2", include_bytes!("../assets/katex/fonts/KaTeX_Size1-Regular.woff2")),
    ("fonts/KaTeX_Size2-Regular.woff2", include_bytes!("../assets/katex/fonts/KaTeX_Size2-Regular.woff2")),
    ("fonts/KaTeX_Size3-Regular.woff2", include_bytes!("../assets/katex/fonts/KaTeX_Size3-Regular.woff2")),
    ("fonts/KaTeX_Size4-Regular.woff2", include_bytes!("../assets/katex/fonts/KaTeX_Size4-Regular.woff2")),
    ("fonts/KaTeX_Typewriter-Regular.woff2", include_bytes!("../assets/katex/fonts/KaTeX_Typewriter-Regular.woff2")),
];
