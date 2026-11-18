//! Unit tests for the widget kit: font, colours, corners and provenance tooltips.
use super::explain::Explained;
use super::icons::{Icon, IconAssets};
use super::kit::*;
use super::screens::Do;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::{Affine2, DVec2};
use bevy::color::ColorToComponents;
use bevy::prelude::*;
use bevy::text::FontSource;
use bevy::ui::UiGlobalTransform;
use bevy::window::PrimaryWindow;
use std::path::{Path, PathBuf};

fn src_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            src_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Every game source file except the kit itself and this test file.
fn ui_sources() -> Vec<(String, String)> {
    let mut files = Vec::new();
    src_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    files
        .into_iter()
        .filter(|p| !matches!(p.file_name().and_then(|n| n.to_str()), Some("kit.rs" | "kit_tests.rs")))
        .map(|p| (p.display().to_string(), std::fs::read_to_string(&p).unwrap()))
        .collect()
}

fn same(a: Color, hex: &str) -> bool {
    let (a, b) = (a.to_srgba(), Srgba::hex(hex).unwrap());
    (a.red - b.red).abs() < 1e-5 && (a.green - b.green).abs() < 1e-5 && (a.blue - b.blue).abs() < 1e-5
}

#[test]
fn no_source_text_uses_the_engine_default_font() {
    // Text styles are only built by `kit::font`; a bare `TextFont` would use the engine default.
    for (name, src) in ui_sources() {
        for banned in ["TextFont::from_font_size", "TextFont::default", "TextFont {", "Text2d"] {
            assert!(!src.contains(banned), "{name} builds text with `{banned}`; use kit::font or kit::font_bold");
        }
    }
}

#[test]
fn kit_fonts_are_the_shipped_files_at_a_legible_size() {
    let regular = font(14.0);
    assert_eq!(regular.font, FontSource::Handle(FONT_REGULAR));
    assert_eq!(font_bold(16.0).font, FontSource::Handle(FONT_BOLD));
    assert_ne!(FONT_REGULAR, FONT_BOLD);
    assert_eq!(font(9.0).font_size, TextFont::from_font_size(T_MIN).font_size, "nothing smaller than 13 px");
    // the embedded bytes are the shipped Atkinson Hyperlegible files
    for name in ["AtkinsonHyperlegible-Regular.ttf", "AtkinsonHyperlegible-Bold.ttf"] {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts").join(name);
        assert!(std::fs::metadata(&p).unwrap().len() > 10_000, "{name} ships");
    }
    assert!(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts/OFL.txt").exists(), "licence ships with the font");
}

#[test]
fn stray_default_font_text_is_moved_to_the_shipped_font() {
    let mut world = World::new();
    let stray = world.spawn(TextFont::default()).id();
    let own = world.spawn(font_bold(14.0)).id();
    world.run_system_once(shipped_font_only).unwrap();
    assert_eq!(world.get::<TextFont>(stray).unwrap().font, FontSource::Handle(FONT_REGULAR));
    assert_eq!(world.get::<TextFont>(own).unwrap().font, FontSource::Handle(FONT_BOLD));
}

#[test]
fn palette_is_the_spec_palette() {
    for (c, hex) in [
        (BARK, "3F2E24"),
        (RAISED, "4D3A2D"),
        (RAIL, "2A1F19"),
        (LINE, "5B4636"),
        (PITH, "F1E3C6"),
        (DIM, "BFA88A"),
        (POND, "5FC7D6"),
        (MOSS, "8DBF5A"),
        (AMBER, "F0A93B"),
        (BERRY, "E0556B"),
    ] {
        assert!(same(c, hex), "{hex}");
    }
}

fn corners(b: &BorderRadius) -> [Val; 4] {
    [b.top_left.x, b.top_right.x, b.bottom_right.x, b.bottom_left.x]
}

/// Builds each kit widget in a world and applies corners the way the game does.
fn built() -> World {
    let mut world = World::new();
    world.spawn(panel(Node::default()));
    for kind in [ButtonKind::Normal, ButtonKind::Primary, ButtonKind::Active] {
        let b = KitButton::new(Icon::Build, "Build", Do::ClosePanel).kind(kind);
        world.spawn(button_frame(&b, false));
    }
    let b = KitButton::new(Icon::Build, "Build", Do::ClosePanel).compact();
    world.spawn(button_frame(&b, false));
    world.spawn(row(0));
    world.spawn(row(1));
    world.run_system_once(apply_corner).unwrap();
    world
}

#[test]
fn panel_is_bark_with_gnawed_corners_and_a_hard_shadow() {
    let mut world = built();
    let mut q = world.query::<(&Node, &BackgroundColor, &Corner, &BoxShadow)>();
    let (node, bg, corner, shadow) = q.iter(&world).find(|r| *r.2 == Corner::Panel).unwrap();
    assert!(same(bg.0, "3F2E24"), "panel fill is Bark");
    assert_eq!(corners(&node.border_radius), [px(8), px(0), px(8), px(0)], "8 px on top-left and bottom-right only");
    assert_eq!(node.padding, UiRect::all(px(16)), "panel padding is 16 px");
    let s = &shadow.0[0];
    assert!(same(s.color, "2A1F19") && s.x_offset == px(3) && s.y_offset == px(3) && s.blur_radius == px(0), "hard 3 px Rail shadow");
    assert_eq!(*corner, Corner::Panel);
}

#[test]
fn buttons_are_raised_or_moss_44_px_with_6_px_gnawed_corners() {
    let mut world = built();
    let mut q = world.query::<(&Node, &BackgroundColor, &Corner, &Clickable)>();
    let all: Vec<_> = q.iter(&world).filter(|r| *r.2 == Corner::Button).collect();
    assert_eq!(all.len(), 4);
    let fills: Vec<bool> = all.iter().map(|(_, bg, ..)| same(bg.0, "4D3A2D")).collect();
    assert_eq!(fills.iter().filter(|f| **f).count(), 3, "normal, active and compact buttons are Raised");
    assert_eq!(all.iter().filter(|(_, bg, ..)| same(bg.0, "8DBF5A")).count(), 1, "primary is Moss");
    for (node, _, _, click) in all.iter() {
        assert_eq!(corners(&node.border_radius), [px(6), px(0), px(6), px(0)]);
        assert!(click.hot == LINE || click.hot == MOSS_HOT, "hover uses Line or the hot Moss");
    }
    assert_eq!(all.iter().filter(|(n, ..)| n.height == px(44)).count(), 3, "full buttons are 44 px tall");
    assert_eq!(ButtonKind::Primary.text(), RAIL, "primary text is Rail");
    assert_eq!(ButtonKind::Normal.text(), PITH);
}

#[test]
fn rows_are_40_px_and_alternate_raised_and_bark() {
    let mut world = built();
    let mut q = world.query::<(&Node, &BackgroundColor)>();
    let rows: Vec<_> = q.iter(&world).filter(|(n, _)| n.height == px(40)).collect();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|(_, bg)| same(bg.0, "4D3A2D")) && rows.iter().any(|(_, bg)| same(bg.0, "3F2E24")));
}

#[test]
fn an_active_button_has_a_pond_bar_and_a_button_has_icon_then_label() {
    let mut world = World::new();
    world.insert_resource(IconAssets::default());
    world
        .run_system_once(|mut commands: Commands, icons: Res<IconAssets>| {
            commands.spawn(Node::default()).with_children(|p| {
                spawn_button(p, &icons, KitButton::new(Icon::Build, "Build", Do::ClosePanel).kind(ButtonKind::Active));
            });
        })
        .unwrap();
    let mut q = world.query::<(&Node, &BackgroundColor)>();
    let bars: Vec<_> = q.iter(&world).filter(|(n, bg)| n.position_type == PositionType::Absolute && same(bg.0, "5FC7D6")).collect();
    assert_eq!(bars.len(), 1, "one Pond bar");
    assert_eq!(bars[0].0.height, px(4));
    let mut texts = world.query::<(&Text, &TextColor)>();
    assert!(texts.iter(&world).any(|(t, c)| t.0 == "Build" && c.0 == PITH));
    assert_eq!(world.query::<&ImageNode>().iter(&world).count(), 1, "icon beside the label");
}

#[test]
fn meters_are_square_segments_with_4_px_gaps() {
    let mut world = World::new();
    world
        .run_system_once(|mut commands: Commands| {
            commands.spawn(Node::default()).with_children(|p| meter(p, 0.5, 10, 100.0, 8.0, MOSS));
        })
        .unwrap();
    let mut q = world.query::<(&Node, &BackgroundColor)>();
    let segs: Vec<_> = q.iter(&world).filter(|(n, _)| n.height == px(8) && n.width != Val::Auto).collect();
    assert_eq!(segs.len(), 10);
    assert_eq!(segs.iter().filter(|(_, bg)| bg.0 == MOSS).count(), 5, "half lit");
    assert!(world.query::<&Node>().iter(&world).any(|n| n.column_gap == px(4)), "4 px gaps");
}

#[test]
fn text_whose_font_is_later_reset_to_the_default_is_moved_back() {
    let mut world = World::new();
    let fix = world.register_system(shipped_font_only); // one system, so change detection is real
    let t = world.spawn(font(14.0)).id();
    world.run_system(fix).unwrap();
    world.get_mut::<TextFont>(t).unwrap().font = FontSource::Handle(Handle::default());
    world.run_system(fix).unwrap();
    assert_eq!(world.get::<TextFont>(t).unwrap().font, FontSource::Handle(FONT_REGULAR));
}

// ───────────────────────── provenance tooltips ─────────────────────────

fn window_with_cursor(world: &mut World, at: Option<DVec2>) {
    let mut w = Window::default();
    w.set_physical_cursor_position(at);
    world.spawn((w, PrimaryWindow));
}

fn rig(cursor: DVec2) -> (World, Explained) {
    let mut world = World::new();
    world.init_resource::<Time>();
    world.init_resource::<TipState>();
    window_with_cursor(&mut world, Some(cursor));
    world.run_system_once(|mut c: Commands| spawn_tip_root(&mut c)).unwrap();
    let n = Explained::sum("Food stored", " logs", vec![Explained::part("Larder", 30.0), Explained::part("Cache", 12.0), Explained::part("Carried", -4.0)]);
    // a number drawn by the kit, placed at (100,100) with a 60x20 box
    let shown = n.clone();
    world
        .run_system_once(move |mut commands: Commands| {
            commands.spawn(Node::default()).with_children(|p| number(p, &shown, 14.0, PITH));
        })
        .unwrap();
    let mut q = world.query::<(&Tip, &mut ComputedNode, &mut UiGlobalTransform)>();
    for (_, mut c, mut t) in q.iter_mut(&mut world) {
        c.size = Vec2::new(60.0, 20.0);
        *t = Affine2::from_translation(Vec2::new(100.0, 100.0)).into();
    }
    (world, n)
}

fn texts_under_tip(world: &mut World) -> Vec<String> {
    let mut root = world.query_filtered::<Entity, With<TipRoot>>();
    let root = root.single(world).unwrap();
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if let Some(t) = world.get::<Text>(e) {
            out.push(t.0.clone());
        }
        if let Some(c) = world.get::<Children>(e) {
            stack.extend(c.iter());
        }
    }
    out
}

fn tick(world: &mut World, secs: f32) {
    world.resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(secs));
    world.run_system_once(tooltip).unwrap();
}

#[test]
fn hovering_a_number_with_provenance_lists_its_sources_after_the_delay() {
    let (mut world, n) = rig(DVec2::new(100.0, 100.0));
    tick(&mut world, 0.05);
    let display = |w: &mut World| w.query_filtered::<&Node, With<TipRoot>>().single(w).unwrap().display;
    assert_eq!(display(&mut world), Display::None, "not yet: the pointer has not rested");
    tick(&mut world, 0.4);
    assert_eq!(display(&mut world), Display::Flex, "tooltip shows after resting");
    let texts = texts_under_tip(&mut world);
    assert!(texts.contains(&format!("Food stored: {}", n.show())), "title names the number: {texts:?}");
    for part in &n.parts {
        assert!(texts.contains(&part.label), "source `{}` listed: {texts:?}", part.label);
    }
    assert!(texts.contains(&"+30 logs".to_string()) && texts.contains(&"-4 logs".to_string()), "each source's value: {texts:?}");
}

#[test]
fn moving_off_the_number_hides_the_tooltip() {
    let (mut world, _) = rig(DVec2::new(100.0, 100.0));
    tick(&mut world, 0.05);
    tick(&mut world, 0.4);
    let mut w = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
    w.single_mut(&mut world).unwrap().set_physical_cursor_position(Some(DVec2::new(400.0, 400.0)));
    tick(&mut world, 0.4);
    assert_eq!(world.query_filtered::<&Node, With<TipRoot>>().single(&world).unwrap().display, Display::None);
}

#[test]
fn a_number_without_the_pointer_on_it_shows_nothing() {
    let (mut world, _) = rig(DVec2::new(900.0, 900.0));
    tick(&mut world, 0.05);
    tick(&mut world, 0.5);
    assert_eq!(world.query_filtered::<&Node, With<TipRoot>>().single(&world).unwrap().display, Display::None);
}
