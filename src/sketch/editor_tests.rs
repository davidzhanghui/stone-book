//! 用无头 TestAppContext 驱动真实的草图编辑器：创建、绑定跟随、撤销、容器文字、逐点画线、缩放、成组、橡皮、落盘。

use super::geom::{self, v};
use super::model::{STICKY_FOOTER_H, STICKY_PADDING};
use super::view::{SketchEditor, Tool};
use gpui::TestAppContext;

fn rect_drag(ed: &mut SketchEditor, a: (f64, f64), b: (f64, f64), window: &mut gpui::Window, cx: &mut gpui::Context<SketchEditor>) {
    ed.tool = Tool::Rect;
    ed.press(v(a.0, a.1), false, false, 1, window, cx);
    ed.drag_to(v((a.0 + b.0) / 2., (a.1 + b.1) / 2.), false, false, cx);
    ed.drag_to(v(b.0, b.1), false, false, cx);
    ed.release(v(b.0, b.1), false, window, cx);
}

#[gpui::test]
fn draw_bind_move_undo_text(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(crate::theme::Theme::light()));
    let id = cx.update(|_| crate::objects::create("sketch", None));
    let oid = id.clone();
    let win = cx.add_window(move |_, cx| SketchEditor::new(oid.clone(), cx));
    win.update(cx, |ed, window, cx| {
        // 两个矩形
        rect_drag(ed, (0., 0.), (100., 60.), window, cx);
        rect_drag(ed, (300., 0.), (400., 60.), window, cx);
        assert_eq!(ed.doc.live().filter(|e| e.kind == "rectangle").count(), 2);
        assert_eq!(ed.tool, Tool::Select, "画完自动回到选择工具");
        let r1 = ed.doc.elements[0].id.clone();
        let r2 = ed.doc.elements[1].id.clone();
        assert_eq!((ed.doc.elements[0].width, ed.doc.elements[0].height), (100., 60.));

        // 箭头从 r1 内部拖到 r2 内部：两端绑定，端点落在轮廓外 gap 处
        ed.tool = Tool::Arrow;
        ed.press(v(50., 30.), false, false, 1, window, cx);
        ed.drag_to(v(200., 30.), false, false, cx);
        ed.drag_to(v(350., 30.), false, false, cx);
        ed.release(v(350., 30.), false, window, cx);
        let arrow = ed.doc.live().find(|e| e.kind == "arrow").unwrap().clone();
        assert_eq!(arrow.binding(true).unwrap().element_id, r1);
        assert_eq!(arrow.binding(false).unwrap().element_id, r2);
        let pts = geom::world_points(&arrow);
        assert!((pts[0].x - 104.).abs() < 0.5, "起点贴 r1 右边：{:?}", pts[0]);
        assert!((pts[1].x - 296.).abs() < 0.5, "终点贴 r2 左边：{:?}", pts[1]);
        assert!(ed.doc.get(&r2).unwrap().bound_elements().iter().any(|(i, _)| *i == arrow.id));

        // 拖动 r2（点左边框，避开贴在边上的箭头端点）往下 200：箭头终点跟随
        ed.tool = Tool::Select;
        ed.sel.clear();
        ed.press(v(300., 8.), false, false, 1, window, cx);
        ed.drag_to(v(300., 108.), false, false, cx);
        ed.drag_to(v(300., 208.), false, false, cx);
        ed.release(v(300., 208.), false, window, cx);
        assert_eq!(ed.doc.get(&r2).unwrap().y, 200.);
        let a2 = ed.doc.get(&arrow.id).unwrap();
        let end = *geom::world_points(a2).last().unwrap();
        let rb = geom::bounds(ed.doc.get(&r2).unwrap());
        assert!(rb.inflate(5.).contains(end) && !rb.inflate(-1.).contains(end), "终点跟随到 r2 轮廓：{end:?}");

        // 撤销：r2 回到原位
        ed.do_undo(cx);
        assert_eq!(ed.doc.get(&r2).unwrap().y, 0.);
        ed.do_redo(cx);
        assert_eq!(ed.doc.get(&r2).unwrap().y, 200.);

        // 双击 r1 边框：写容器文字
        ed.sel.clear();
        ed.press(v(0., 30.), false, false, 2, window, cx);
        let area = ed.editing.as_ref().expect("进入文字编辑").area.clone();
        area.update(cx, |a, cx| a.insert("你好世界", cx));
        ed.finish_edit(cx);
        let t = ed.doc.live().find(|e| e.is_text()).unwrap().clone();
        assert_eq!(t.container_id(), Some(r1.as_str()));
        assert_eq!(t.text.as_deref(), Some("你好世界"));
        let c = geom::center(ed.doc.get(&r1).unwrap());
        assert!((geom::center(&t).x - c.x).abs() < 1. && (geom::center(&t).y - c.y).abs() < 1., "文字在容器中心");

        // 移动 r1：容器文字一起动
        ed.sel.clear();
        ed.press(v(0., 10.), false, false, 1, window, cx);
        ed.drag_to(v(-50., 10.), false, false, cx);
        ed.release(v(-50., 10.), false, window, cx);
        let t2 = ed.doc.get(&t.id).unwrap();
        assert!((t2.x - (t.x - 50.)).abs() < 0.01);
    })
    .unwrap();
    // 已写回对象存储
    let saved = cx.update(|_| crate::objects::sketch(&id)).unwrap();
    assert_eq!(saved.live().count(), 4);
}

#[gpui::test]
fn multipoint_line_resize_group_erase(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(crate::theme::Theme::light()));
    let id = cx.update(|_| crate::objects::create("sketch", None));
    let win = cx.add_window(move |_, cx| SketchEditor::new(id.clone(), cx));
    win.update(cx, |ed, window, cx| {
        // 逐点画线：单击起点、单击拐点、双击终点
        ed.tool = Tool::Line;
        ed.press(v(0., 300.), false, false, 1, window, cx);
        ed.release(v(0., 300.), false, window, cx);
        assert!(ed.multi.is_some(), "单击进入逐点模式");
        ed.press(v(100., 300.), false, false, 1, window, cx);
        ed.press(v(100., 400.), false, false, 1, window, cx);
        ed.press(v(100., 400.), false, false, 2, window, cx);
        assert!(ed.multi.is_none());
        let line = ed.doc.live().find(|e| e.kind == "line").unwrap().clone();
        assert_eq!(line.pts().len(), 3, "{:?} undo={}", line.pts(), ed.undo.len());
        assert_eq!((line.width, line.height), (100., 100.));

        // 缩放手柄：右下角拖大
        rect_drag(ed, (0., 0.), (100., 60.), window, cx);
        let r = ed.doc.live().find(|e| e.kind == "rectangle").unwrap().id.clone();
        ed.tool = Tool::Select;
        ed.sel = [r.clone()].into_iter().collect();
        ed.press(v(104., 64.), false, false, 1, window, cx);
        ed.drag_to(v(204., 124.), false, false, cx);
        ed.release(v(204., 124.), false, window, cx);
        let e = ed.doc.get(&r).unwrap();
        assert!((e.width - 204.).abs() < 1. && (e.height - 124.).abs() < 1., "{} x {}", e.width, e.height);

        // 成组：点其中一个就选中整组
        ed.select_all(cx);
        ed.group(cx);
        ed.sel.clear();
        ed.press(v(0., 10.), false, false, 1, window, cx);
        ed.release(v(0., 10.), false, window, cx);
        assert_eq!(ed.sel.len(), 2);

        // 再制：数量翻倍、引用全部换新
        ed.duplicate(cx);
        assert_eq!(ed.doc.live().count(), 4);

        // 橡皮擦过线：删除
        ed.sel.clear();
        ed.tool = Tool::Eraser;
        let before = ed.doc.live().count();
        // 只擦过原来那条线（再制的副本在右下 10px，曲线会向上鼓约 5px）
        ed.press(v(50., 270.), false, false, 1, window, cx);
        ed.drag_to(v(50., 297.), false, false, cx);
        ed.release(v(50., 297.), false, window, cx);
        assert_eq!(ed.doc.live().count(), before - 1);

        // 置底 / 置顶
        let last = ed.doc.elements.last().unwrap().id.clone();
        ed.sel = [last.clone()].into_iter().collect();
        ed.z_order(-2, cx);
        assert_eq!(ed.doc.elements[0].id, last);
        ed.z_order(2, cx);
        assert_eq!(ed.doc.elements.last().unwrap().id, last);
    })
    .unwrap();
}

#[gpui::test]
fn sticky_bend_elbow_lasso_bucket(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(crate::theme::Theme::light()));
    let id = cx.update(|_| crate::objects::create("sketch", None));
    let win = cx.add_window(move |_, cx| SketchEditor::new(id.clone(), cx));
    win.update(cx, |ed, window, cx| {
        // 便签：单击放一张 250×250，马上进入写字；字多了先缩字号再长高
        ed.tool = Tool::Sticky;
        ed.press(v(500., 500.), false, false, 1, window, cx);
        ed.release(v(500., 500.), false, window, cx);
        let note = ed.doc.live().find(|e| e.is_sticky()).unwrap().clone();
        assert_eq!((note.width, note.height), (250., 250.));
        assert_eq!((note.x, note.y), (375., 375.));
        let area = ed.editing.as_ref().expect("便签建好后直接写字").area.clone();
        area.update(cx, |a, cx| a.insert(&"便签里的一段很长很长的文字。".repeat(20), cx));
        ed.finish_edit(cx);
        let note = ed.doc.get(&note.id).unwrap().clone();
        let label = ed.doc.get(&note.bound_text_id().unwrap()).unwrap().clone();
        assert!(label.font_size_or() < 20. && label.font_size_or() >= 16., "先缩字号：{}", label.font_size_or());
        assert!(note.height > 250., "缩到最小字号还放不下就长高：{}", note.height);
        assert_eq!(note.get("baseHeight").and_then(|v| v.as_f64()), Some(250.));
        assert!(label.y + label.height <= note.y + note.height - STICKY_PADDING - STICKY_FOOTER_H + 0.5, "文字不压日期栏");
        // 撤销一次：便签和文字一起没了
        ed.do_undo(cx);
        assert!(ed.doc.live().all(|e| !e.is_sticky() && !e.is_text()));

        // 两点直线：拖中点 → 插入新节点，线弯过去
        ed.tool = Tool::Line;
        ed.press(v(0., 0.), false, false, 1, window, cx);
        ed.drag_to(v(100., 0.), false, false, cx);
        ed.drag_to(v(200., 0.), false, false, cx);
        ed.release(v(200., 0.), false, window, cx);
        let line = ed.doc.live().find(|e| e.kind == "line").unwrap().id.clone();
        assert_eq!(ed.handle_at(v(100., 0.)), Some(200), "中点手柄");
        ed.press(v(100., 0.), false, false, 1, window, cx);
        ed.drag_to(v(100., 40.), false, false, cx);
        ed.drag_to(v(100., 80.), false, false, cx);
        ed.release(v(100., 80.), false, window, cx);
        let pts = geom::world_points(ed.doc.get(&line).unwrap());
        assert_eq!(pts.len(), 3);
        assert!(pts[1].dist(v(100., 80.)) < 0.5, "{:?}", pts);
        // 线编辑：选中节点后删除
        ed.toggle_line_edit(cx);
        assert_eq!(ed.line_edit.as_deref(), Some(line.as_str()));
        ed.press(v(100., 80.), false, false, 1, window, cx);
        ed.release(v(100., 80.), false, window, cx);
        assert_eq!(ed.line_pt, Some(1));
        ed.delete_sel(cx);
        assert_eq!(ed.doc.get(&line).unwrap().pts().len(), 2);
        assert!(ed.doc.get(&line).is_some(), "删节点不删线");

        // 折线箭头：两端绑定、走正交线；目标移动后重算
        rect_drag(ed, (0., 300.), (100., 360.), window, cx);
        rect_drag(ed, (300., 400.), (400., 460.), window, cx);
        let r2 = ed.doc.elements.iter().rev().find(|e| e.kind == "rectangle").unwrap().id.clone();
        ed.cur.arrow_type = "elbow";
        ed.tool = Tool::Arrow;
        ed.press(v(50., 330.), false, false, 1, window, cx);
        ed.drag_to(v(200., 380.), false, false, cx);
        ed.drag_to(v(350., 430.), false, false, cx);
        ed.release(v(350., 430.), false, window, cx);
        let arrow = ed.doc.live().find(|e| e.kind == "arrow").unwrap().clone();
        assert!(arrow.elbowed() && arrow.binding(true).is_some() && arrow.binding(false).is_some());
        let orth = |pts: &[geom::V]| pts.windows(2).all(|w| (w[0].x - w[1].x).abs() < 0.01 || (w[0].y - w[1].y).abs() < 0.01);
        assert!(orth(&geom::world_points(&arrow)), "{:?}", geom::world_points(&arrow));
        ed.sel = [r2.clone()].into_iter().collect();
        ed.nudge(0., 100., cx);
        let a2 = ed.doc.get(&arrow.id).unwrap();
        let wp = geom::world_points(a2);
        assert!(orth(&wp));
        let rb = geom::bounds(ed.doc.get(&r2).unwrap());
        assert!(rb.inflate(6.).contains(*wp.last().unwrap()), "终点跟到移动后的矩形：{:?}", wp.last());

        // 套索：圈住第一个矩形
        ed.sel.clear();
        ed.tool = Tool::Lasso;
        ed.press(v(-20., 280.), false, false, 1, window, cx);
        for p in [v(130., 280.), v(130., 380.), v(-20., 380.), v(-20., 285.)] {
            ed.drag_to(p, false, false, cx);
        }
        ed.release(v(-20., 285.), false, window, cx);
        let r1 = ed.doc.live().find(|e| e.kind == "rectangle").unwrap().id.clone();
        assert!(ed.sel.contains(&r1) && !ed.sel.contains(&r2), "{:?}", ed.sel);

        // 油漆桶：用当前背景色填第二个矩形
        ed.sel.clear();
        ed.cur.bg = "#ffc9c9".into();
        ed.tool = Tool::Bucket;
        let c = geom::center(ed.doc.get(&r2).unwrap());
        ed.press(c, false, false, 1, window, cx);
        assert_eq!(ed.doc.get(&r2).unwrap().background_color, "#ffc9c9");
    })
    .unwrap();
}

#[gpui::test]
fn multipoint_four_clicks(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(crate::theme::Theme::light()));
    let id = cx.update(|_| crate::objects::create("sketch", None));
    let win = cx.add_window(move |_, cx| SketchEditor::new(id.clone(), cx));
    win.update(cx, |ed, window, cx| {
        ed.tool = Tool::Line;
        for (x, y, n) in [(0., 360., 1), (120., 300., 1), (240., 380., 1), (360., 310., 1), (360., 310., 2)] {
            ed.press(v(x, y), false, false, n, window, cx);
            ed.release(v(x, y), false, window, cx);
        }
        let line = ed.doc.live().find(|e| e.kind == "line").unwrap();
        assert_eq!(geom::world_points(line).len(), 4, "{:?}", geom::world_points(line));
    })
    .unwrap();
}

#[gpui::test]
fn crop_image_keeps_scale(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(crate::theme::Theme::light()));
    let id = cx.update(|_| crate::objects::create("sketch", None));
    let win = cx.add_window(move |_, cx| SketchEditor::new(id.clone(), cx));
    let logo = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/logo-256.png").to_string();
    win.update(cx, |ed, window, cx| {
        ed.insert_image(logo.clone(), Some(v(128., 128.)), cx);
        let img = ed.doc.live().find(|e| e.kind == "image").unwrap().clone();
        assert_eq!((img.x, img.y, img.width, img.height), (0., 0., 256., 256.));
        ed.toggle_crop(cx);
        // 右下角手柄往左上拖 56：显示区 200×200，原图比例不变
        let h = ed.handle_at(v(256., 256.));
        assert_eq!(h, Some(304));
        ed.press(v(256., 256.), false, false, 1, window, cx);
        ed.drag_to(v(200., 200.), false, false, cx);
        ed.release(v(200., 200.), false, window, cx);
        let e = ed.doc.get(&img.id).unwrap();
        assert_eq!((e.x, e.y, e.width, e.height), (0., 0., 200., 200.));
        let c = e.get("crop").unwrap();
        assert_eq!(c["width"].as_f64(), Some(200.));
        assert_eq!(c["naturalWidth"].as_f64(), Some(256.));
        // 左上角往里拖：x/y 偏移进裁剪
        ed.press(v(0., 0.), false, false, 1, window, cx);
        ed.drag_to(v(50., 40.), false, false, cx);
        ed.release(v(50., 40.), false, window, cx);
        let e = ed.doc.get(&img.id).unwrap();
        let c = e.get("crop").unwrap();
        assert_eq!((c["x"].as_f64(), c["y"].as_f64()), (Some(50.), Some(40.)));
        assert_eq!((e.x, e.y, e.width, e.height), (50., 40., 150., 160.));
        // SVG 导出用裁剪 viewBox
        let svg = super::svg::to_svg(&ed.doc, &[], super::svg::SvgOpts::default());
        assert!(svg.contains("viewBox=\"50 40 150 160\""), "{}", &svg[..svg.len().min(600)]);
    })
    .unwrap();
}
