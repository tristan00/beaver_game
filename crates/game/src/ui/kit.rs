//! The one widget kit (`docs/UI_SPEC.md` §5). Every colour, corner, font and control shape in the
//! interface comes from here: dark bark panels, raised rows and buttons, a pond-coloured mark for
//! what is active, gnawed corners, square-segment meters and hard offset shadows. Zone modules
//! (`topbar`, `toolbar`, `jobs`, `alerts`, `inspector`, `panels`) only compose these parts.
//!
//! Text always uses the shipped Atkinson Hyperlegible font (`font`, `font_bold`); `KitPlugin`
//! also repairs any stray `TextFont` that still points at the engine default font.
use bevy::asset::uuid_handle;
use bevy::prelude::*;
use bevy::text::FontSource;
use bevy::ui::UiGlobalTransform;
use bevy::window::PrimaryWindow;

use super::explain::Explained;
use super::icons::{Icon, IconAssets};
use super::screens::{Do, Tone};

// ───────────────────────────── colours ─────────────────────────────

const fn hex(r: u8, g: u8, b: u8) -> Color {
    Color::srgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

/// Panel fill.
pub const BARK: Color = hex(0x3F, 0x2E, 0x24);
/// Rows and buttons.
pub const RAISED: Color = hex(0x4D, 0x3A, 0x2D);
/// Outlines, shadow and text on a bright fill.
pub const RAIL: Color = hex(0x2A, 0x1F, 0x19);
/// Dividers and the hover state of raised parts.
pub const LINE: Color = hex(0x5B, 0x46, 0x36);
/// Primary text.
pub const PITH: Color = hex(0xF1, 0xE3, 0xC6);
/// Secondary text.
pub const DIM: Color = hex(0xBF, 0xA8, 0x8A);
/// Active and selected.
pub const POND: Color = hex(0x5F, 0xC7, 0xD6);
/// Good.
pub const MOSS: Color = hex(0x8D, 0xBF, 0x5A);
/// Warning.
pub const AMBER: Color = hex(0xF0, 0xA9, 0x3B);
/// Danger.
pub const BERRY: Color = hex(0xE0, 0x55, 0x6B);
/// Hover state of a primary (Moss) button.
pub const MOSS_HOT: Color = hex(0xA6, 0xD3, 0x74);
pub const CLEAR: Color = Color::srgba(0.0, 0.0, 0.0, 0.0);

// ───────────────────────────── metrics ─────────────────────────────

/// Diagonal-corner size of panels and of buttons.
pub const PANEL_CORNER: f32 = 8.0;
pub const BUTTON_CORNER: f32 = 6.0;
pub const BUTTON_H: f32 = 44.0;
/// A button inside a table row, which is shorter than a row's 40 px.
pub const BUTTON_H_ROW: f32 = 32.0;
pub const ROW_H: f32 = 40.0;
pub const PAD: f32 = 16.0;
pub const GRID: f32 = 8.0;
pub const SHADOW: f32 = 3.0;
pub const SEGMENT_GAP: f32 = 4.0;
pub const ACTIVE_BAR: f32 = 4.0;
/// Seconds the pointer rests before a tooltip shows.
pub const TIP_DELAY: f32 = 0.3;
/// Top of the zones under the top strip (clock, stores ribbon, time controls).
pub const ZONE_TOP: f32 = 70.0;

/// Body, title and smallest permitted text sizes.
pub const T_BODY: f32 = 14.0;
pub const T_TITLE: f32 = 16.0;
pub const T_MIN: f32 = 13.0;
pub const FONT: f32 = T_BODY;

// ───────────────────────────── font ─────────────────────────────

pub const FONT_REGULAR: Handle<Font> = uuid_handle!("6a1f6d0e-5d63-4c43-9d57-3f1d3f4a0b01");
pub const FONT_BOLD: Handle<Font> = uuid_handle!("6a1f6d0e-5d63-4c43-9d57-3f1d3f4a0b02");

static REGULAR_BYTES: &[u8] = include_bytes!("../../../../assets/fonts/AtkinsonHyperlegible-Regular.ttf");
static BOLD_BYTES: &[u8] = include_bytes!("../../../../assets/fonts/AtkinsonHyperlegible-Bold.ttf");

/// Body text in the shipped font; sizes below `T_MIN` are raised to it.
pub fn font(size: f32) -> TextFont {
    TextFont::from_font_size(size.max(T_MIN)).with_font(FONT_REGULAR)
}

pub fn font_bold(size: f32) -> TextFont {
    TextFont::from_font_size(size.max(T_MIN)).with_font(FONT_BOLD)
}

/// `BC_UI_ZONES=1` draws the alerts and inspector zones even when empty (screenshots of the layout).
pub fn show_empty_zones() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("BC_UI_ZONES").is_ok_and(|v| v != "0"))
}

pub struct KitPlugin;

impl Plugin for KitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TipState>()
            .add_systems(Startup, install_fonts)
            .add_systems(PostUpdate, (apply_corner, shipped_font_only).before(bevy::ui::UiSystems::Prepare));
    }
}

fn install_fonts(mut fonts: ResMut<Assets<Font>>) {
    let _ = fonts.insert(&FONT_REGULAR, Font::from_bytes(REGULAR_BYTES.to_vec()));
    let _ = fonts.insert(&FONT_BOLD, Font::from_bytes(BOLD_BYTES.to_vec()));
}

/// Moves text that would use the engine default font onto the shipped one, whether the text was
/// just spawned or its font was later set back to the default.
pub fn shipped_font_only(mut q: Query<&mut TextFont, Changed<TextFont>>) {
    for mut f in &mut q {
        if f.font == FontSource::Handle(Handle::default()) {
            f.font = FontSource::Handle(FONT_REGULAR);
        }
    }
}

// ───────────────────────────── corners ─────────────────────────────

/// Gnawed corners: only top-left and bottom-right are cut, the other two stay square.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Corner {
    Panel,
    Button,
}

impl Corner {
    pub fn size(self) -> f32 {
        match self {
            Corner::Panel => PANEL_CORNER,
            Corner::Button => BUTTON_CORNER,
        }
    }
    pub fn radius(self) -> BorderRadius {
        let r = self.size();
        BorderRadius::px(r, 0.0, r, 0.0)
    }
}

pub fn apply_corner(mut q: Query<(&Corner, &mut Node), Added<Corner>>) {
    for (c, mut n) in &mut q {
        n.border_radius = c.radius();
    }
}

// ───────────────────────────── widgets ─────────────────────────────

/// A node that reacts to a click. Higher `layer` wins where nodes overlap; `scrolled` nodes
/// only count while the cursor is inside the scroll area.
#[derive(Component, Clone)]
pub struct Clickable {
    pub act: Do,
    pub layer: u8,
    pub scrolled: bool,
    pub base: Color,
    pub hot: Color,
}

/// Nodes that swallow mouse input so the world underneath is not clicked or orbited.
#[derive(Component)]
pub struct Blocker;

/// The scrolling body of the detail panel.
#[derive(Component)]
pub struct ScrollArea;

pub type Kids<'a> = ChildSpawnerCommands<'a>;

pub fn hard_shadow() -> BoxShadow {
    BoxShadow::new(RAIL, px(SHADOW), px(SHADOW), px(0), px(0))
}

/// A bark panel with a hard shadow and gnawed corners, laid out as a column with 16 px padding.
/// `node` carries the placement and size.
pub fn panel(node: Node) -> impl Bundle {
    panel_with(Node { padding: UiRect::all(px(PAD)), flex_direction: FlexDirection::Column, row_gap: px(GRID), ..node })
}

/// The same panel look around a node whose own padding and direction are kept (rows of controls).
pub fn panel_with(node: Node) -> impl Bundle {
    (node, BackgroundColor(BARK), Corner::Panel, hard_shadow())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonKind {
    Normal,
    /// Moss fill, Rail text.
    Primary,
    /// Pond bar along the bottom.
    Active,
}

impl ButtonKind {
    pub fn fill(self) -> Color {
        if self == ButtonKind::Primary { MOSS } else { RAISED }
    }
    pub fn hot(self) -> Color {
        if self == ButtonKind::Primary { MOSS_HOT } else { LINE }
    }
    pub fn text(self) -> Color {
        if self == ButtonKind::Primary { RAIL } else { PITH }
    }
}

/// What `button` needs to build one control.
pub struct KitButton {
    pub icon: Option<Icon>,
    pub label: String,
    pub act: Do,
    pub kind: ButtonKind,
    pub layer: u8,
    pub tip: Option<String>,
    /// Row-sized (32 px) instead of the full 44 px.
    pub compact: bool,
}

impl KitButton {
    pub fn new(icon: Icon, label: impl Into<String>, act: Do) -> KitButton {
        KitButton { icon: Some(icon), label: label.into(), act, kind: ButtonKind::Normal, layer: 5, tip: None, compact: false }
    }
    pub fn kind(mut self, k: ButtonKind) -> Self {
        self.kind = k;
        self
    }
    pub fn layer(mut self, l: u8) -> Self {
        self.layer = l;
        self
    }
    pub fn tip(mut self, t: impl Into<String>) -> Self {
        self.tip = Some(t.into());
        self
    }
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }
}

/// The button's own node: fill, size, corner, click target.
pub fn button_frame(b: &KitButton, scrolled: bool) -> impl Bundle {
    (
        Node {
            height: px(if b.compact { BUTTON_H_ROW } else { BUTTON_H }),
            padding: UiRect::axes(px(12), px(0)),
            column_gap: px(GRID),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(b.kind.fill()),
        Corner::Button,
        Clickable { act: b.act.clone(), layer: b.layer, scrolled, base: b.kind.fill(), hot: b.kind.hot() },
    )
}

/// Spawns a button: icon left, then the label; an active one gets the Pond bar on its bottom edge.
pub fn spawn_button(p: &mut Kids, icons: &IconAssets, b: KitButton) -> Entity {
    spawn_button_in(p, icons, b, false)
}

pub fn spawn_button_in(p: &mut Kids, icons: &IconAssets, b: KitButton, scrolled: bool) -> Entity {
    let mut e = p.spawn(button_frame(&b, scrolled));
    if let Some(t) = &b.tip {
        e.insert(Tip::note(t.clone()));
    }
    let kind = b.kind;
    e.with_children(|c| {
        if let Some(i) = b.icon {
            icon_node(c, icons, i, if b.compact { 20.0 } else { 28.0 });
        }
        if !b.label.is_empty() {
            label(c, b.label.clone(), T_BODY, kind.text());
        }
        if kind == ButtonKind::Active {
            c.spawn((
                Node { position_type: PositionType::Absolute, left: px(0), right: px(0), bottom: px(0), height: px(ACTIVE_BAR), ..default() },
                BackgroundColor(POND),
            ));
        }
    });
    e.id()
}

/// Background of table or list row `i`: two alternating fills.
pub fn row_fill(i: usize) -> Color {
    if i % 2 == 0 { RAISED } else { BARK }
}

/// A 40 px row.
pub fn row(i: usize) -> impl Bundle {
    (
        Node { height: px(ROW_H), padding: UiRect::horizontal(px(GRID)), align_items: AlignItems::Center, flex_shrink: 0.0, ..default() },
        BackgroundColor(row_fill(i)),
    )
}

pub fn icon_node(p: &mut Kids, icons: &IconAssets, icon: Icon, size: f32) {
    p.spawn((Node { width: px(size), height: px(size), flex_shrink: 0.0, ..default() }, ImageNode::new(icons.get(icon))));
}

pub fn label(p: &mut Kids, s: impl Into<String>, size: f32, color: Color) {
    p.spawn((Text::new(s), font(size), TextColor(color), TextLayout::no_wrap()));
}

pub fn label_bold(p: &mut Kids, s: impl Into<String>, size: f32, color: Color) {
    p.spawn((Text::new(s), font_bold(size), TextColor(color), TextLayout::no_wrap()));
}

/// A figure with provenance: hovering it shows the parts it adds up to (`Tip::of`).
pub fn number(p: &mut Kids, n: &Explained, size: f32, color: Color) {
    p.spawn((Text::new(n.show()), font(size), TextColor(color), TextLayout::no_wrap(), Tip::of(n)));
}

pub fn tone_color(t: Tone) -> Color {
    use Tone::*;
    match t {
        Normal => PITH,
        Good => MOSS,
        Warn => AMBER,
        Bad => BERRY,
        Dim => DIM,
    }
}

/// A meter of square segments: `frac` of the `segments` are filled with `color`.
pub fn meter(p: &mut Kids, frac: f32, segments: usize, width: f32, height: f32, color: Color) {
    let n = segments.max(1);
    let lit = (frac.clamp(0.0, 1.0) * n as f32).round() as usize;
    let seg_w = ((width - SEGMENT_GAP * (n as f32 - 1.0)) / n as f32).max(1.0);
    p.spawn(Node { column_gap: px(SEGMENT_GAP), height: px(height), flex_shrink: 0.0, ..default() }).with_children(|m| {
        for i in 0..n {
            m.spawn((Node { width: px(seg_w), height: px(height), flex_shrink: 0.0, ..default() }, BackgroundColor(if i < lit { color } else { RAIL })));
        }
    });
}

/// A meter 5 segments wide (headline figures).
pub fn bar(p: &mut Kids, frac: f32, width: f32, height: f32, color: Color) {
    meter(p, frac, 5, width.max(5.0 * 5.0 + 4.0 * SEGMENT_GAP), height.max(6.0), color);
}

// ───────────────────────────── hit testing ─────────────────────────────

fn contains(node: &ComputedNode, t: &UiGlobalTransform, p: Vec2) -> bool {
    node.size.x > 1.0 && node.contains_point(*t, p)
}

/// The action under `cursor` (physical pixels), if any.
pub fn hit_test(
    cursor: Vec2,
    clickables: &Query<(&Clickable, &ComputedNode, &UiGlobalTransform)>,
    scroll: &Query<(&ComputedNode, &UiGlobalTransform), With<ScrollArea>>,
) -> Option<(Do, u8)> {
    let in_scroll = scroll.iter().any(|(n, t)| contains(n, t, cursor));
    clickables
        .iter()
        .filter(|(c, n, t)| (!c.scrolled || in_scroll) && contains(n, t, cursor))
        .max_by_key(|(c, _, _)| c.layer)
        .map(|(c, _, _)| (c.act.clone(), c.layer))
}

/// Tints clickable nodes under the cursor.
pub fn hover_tint(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut q: Query<(&Clickable, &ComputedNode, &UiGlobalTransform, &mut BackgroundColor)>,
    scroll: Query<(&ComputedNode, &UiGlobalTransform), With<ScrollArea>>,
) {
    let cursor = windows.single().ok().and_then(|w| w.physical_cursor_position());
    let in_scroll = cursor.is_some_and(|c| scroll.iter().any(|(n, t)| contains(n, t, c)));
    for (c, n, t, mut bg) in &mut q {
        let hot = cursor.is_some_and(|p| (!c.scrolled || in_scroll) && contains(n, t, p));
        let want = if hot { c.hot } else { c.base };
        if bg.0 != want {
            bg.0 = want;
        }
    }
}

// ───────────────────────────── tooltips ─────────────────────────────

/// Text shown while hovering a node: a title and its terms.
#[derive(Component, Clone, PartialEq, Debug)]
pub struct Tip {
    pub title: String,
    pub lines: Vec<(String, String)>,
}

impl Tip {
    pub fn note(title: impl Into<String>) -> Tip {
        Tip { title: title.into(), lines: Vec::new() }
    }
    /// "Label = value" over each term that adds up to it.
    pub fn of(n: &Explained) -> Tip {
        let fmt = |v: f32| format!("{v:+.*}{}", n.dec.max(if n.parts.iter().any(|p| p.value.fract().abs() > 0.004) { 1 } else { 0 }) as usize, n.unit);
        Tip {
            title: format!("{}: {}", n.label, n.show()),
            lines: n.parts.iter().map(|p| (p.label.clone(), fmt(p.value))).collect(),
        }
    }
}

#[derive(Component)]
pub struct TipRoot;

/// The tooltip waiting to show: what is under the pointer and for how long it has rested.
#[derive(Resource, Default)]
pub struct TipState {
    pub tip: Option<Tip>,
    pub at: Vec2,
    pub rested: f32,
    pub shown: Option<Tip>,
}

pub fn spawn_tip_root(commands: &mut Commands) {
    commands.spawn((
        TipRoot,
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            flex_direction: FlexDirection::Column,
            padding: UiRect::axes(px(12), px(8)),
            border: UiRect::all(px(2)),
            row_gap: px(2),
            max_width: px(320),
            ..default()
        },
        BackgroundColor(BARK),
        BorderColor::all(RAIL),
        Corner::Panel,
        hard_shadow(),
        GlobalZIndex(40),
    ));
}

/// Shows the tooltip of the node under the cursor once the pointer has rested `TIP_DELAY`.
#[allow(clippy::type_complexity)]
pub fn tooltip(
    time: Res<Time>,
    windows: Query<&Window, With<PrimaryWindow>>,
    tips: Query<(&Tip, &ComputedNode, &UiGlobalTransform)>,
    mut root: Query<(Entity, &mut Node), With<TipRoot>>,
    mut state: ResMut<TipState>,
    mut commands: Commands,
) {
    let Ok(w) = windows.single() else { return };
    let Ok((e, mut node)) = root.single_mut() else { return };
    let hit = w.physical_cursor_position().and_then(|c| {
        tips.iter()
            .filter(|(_, n, t)| contains(n, t, c))
            // innermost (smallest) node wins, so a number beats the row or card around it
            .min_by(|a, b| (a.1.size.x * a.1.size.y).total_cmp(&(b.1.size.x * b.1.size.y)))
            .map(|(tip, ..)| tip.clone())
    });
    let pos = w.cursor_position().unwrap_or(Vec2::ZERO);
    if hit != state.tip || pos.distance(state.at) > 6.0 {
        state.rested = 0.0;
        state.at = pos;
    } else {
        state.rested += time.delta_secs();
    }
    state.tip = hit.clone();
    let show = hit.filter(|_| state.rested >= TIP_DELAY && w.cursor_position().is_some());
    match show {
        Some(tip) => {
            if state.shown.as_ref() != Some(&tip) {
                commands.entity(e).despawn_children();
                commands.entity(e).with_children(|p| tip_body(p, &tip));
                state.shown = Some(tip.clone());
            }
            node.display = Display::Flex;
            node.left = px((pos.x + 14.0).min(w.width() - 330.0).max(4.0));
            node.top = px((pos.y + 18.0).min(w.height() - 56.0 - 20.0 * tip.lines.len() as f32).max(4.0));
        }
        None => {
            node.display = Display::None;
            state.shown = None;
        }
    }
}

fn tip_body(p: &mut Kids, tip: &Tip) {
    label_bold(p, tip.title.clone(), T_BODY, PITH);
    for (l, v) in &tip.lines {
        p.spawn(Node { column_gap: px(16), justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|r| {
            label(r, l.clone(), T_MIN, DIM);
            label(r, v.clone(), T_MIN, PITH);
        });
    }
}
