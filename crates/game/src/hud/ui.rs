#![allow(deprecated)]
use super::{Blocks, HudAction};
use crate::icons::{icon_node, Icon, Icons};
pub use crate::skins::Skin;
use bevy::sprite::{BorderRect, SliceScaleMode, TextureSlicer};
use bevy::ui::widget::NodeImageMode;
use bevy::prelude::*;
use bevy::text::{FontSize, FontWeight};
use bevy::ui::{FocusPolicy, RelativeCursorPosition};
use bevy::window::PrimaryWindow;

pub const PANEL: Color = Color::srgba(0.106, 0.09, 0.075, 0.94);
pub const BARK: Color = Color::srgb(0.59, 0.41, 0.24);
pub const INK: Color = Color::srgb(0.94, 0.91, 0.86);
pub const MUTED: Color = Color::srgb(0.72, 0.67, 0.6);
pub const FAINT: Color = Color::srgba(1.0, 1.0, 1.0, 0.07);
pub const LINE: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);
pub const GOOD: Color = Color::srgb(0.39, 0.79, 0.55);
pub const WARN: Color = Color::srgb(0.94, 0.75, 0.29);
pub const BAD: Color = Color::srgb(0.94, 0.45, 0.38);
pub const WATER: Color = Color::srgb(0.29, 0.64, 0.87);
pub const ACCENT: Color = Color::srgb(0.91, 0.52, 0.23);
pub const ON_ACCENT: Color = Color::srgb(0.13, 0.09, 0.05);
pub const SELECT: Color = Color::srgb(0.97, 0.95, 0.9);
/// The dark groove a gauge is cut into.
pub const GROOVE: Color = Color::srgba(0.0, 0.0, 0.0, 0.38);

/// The HUD's type scale. Every text is drawn at one of these; any other size asked for is
/// taken to the nearest.
pub mod ts {
    pub const SMALL: f32 = 11.0;
    pub const BODY: f32 = 12.0;
    pub const VALUE: f32 = 13.0;
    pub const HEAD: f32 = 14.0;
    pub const TITLE: f32 = 16.0;
    pub const PAGE: f32 = 20.0;
    pub const DISPLAY: f32 = 30.0;
    pub const ALL: [f32; 7] = [SMALL, BODY, VALUE, HEAD, TITLE, PAGE, DISPLAY];
    pub fn snap(size: f32) -> f32 {
        ALL.into_iter().min_by(|a, b| (a - size).abs().total_cmp(&(b - size).abs())).unwrap_or(BODY)
    }
}

pub fn font(size: f32, bold: bool) -> TextFont {
    TextFont { font_size: FontSize::Px(ts::snap(size)), weight: if bold { FontWeight::BOLD } else { FontWeight::NORMAL }, ..default() }
}

/// Where full pages sit: right of the lens rail, under the top bar, and above the bottom row,
/// so the minimap, legends, build bar and journal always stay in view.
pub const PAGE_LEFT: f32 = 76.0;
pub const PAGE_TOP: f32 = 84.0;
pub const PAGE_RIGHT: f32 = 20.0;
pub const PAGE_BOTTOM: f32 = 190.0;

/// A page's panel in the page area: `fill` stretches it over the area (a beaver's story, a
/// family page); otherwise it keeps its own width and grows down to the area's foot at most.
pub fn page_area(mut panel: VNode, fill: bool) -> VNode {
    let mut area = VNode::new(Node {
        position_type: PositionType::Absolute,
        left: Val::Px(PAGE_LEFT),
        right: Val::Px(PAGE_RIGHT),
        top: Val::Px(PAGE_TOP),
        bottom: Val::Px(PAGE_BOTTOM),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::FlexStart,
        ..default()
    });
    panel.node.position_type = PositionType::Relative;
    panel.node.max_height = Val::Percent(100.0);
    if fill {
        panel.node.width = Val::Percent(100.0);
        panel.node.height = Val::Percent(100.0);
    }
    area.children.push(panel);
    area
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct TipLine {
    pub icon: Option<(Icon, Color)>,
    pub label: String,
    pub value: String,
    pub color: Option<Color>,
    pub indent: bool,
    pub nested: Option<Box<Tip>>,
    pub key: Option<String>,
    /// A line that does something when clicked: the fix for an alert.
    pub action: Option<HudAction>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Tip {
    pub title: Option<(Option<(Icon, Color)>, String, String)>,
    pub lines: Vec<TipLine>,
}

impl Tip {
    pub fn titled(icon: Option<(Icon, Color)>, title: impl Into<String>) -> Tip {
        Tip { title: Some((icon, title.into(), String::new())), lines: Vec::new() }
    }
    pub fn name(s: impl Into<String>) -> Tip {
        Tip::titled(None, s)
    }
    pub fn with_value(mut self, v: impl Into<String>) -> Tip {
        if let Some(t) = self.title.as_mut() {
            t.2 = v.into();
        }
        self
    }
    pub fn key(mut self, k: impl Into<String>) -> Tip {
        self.lines.push(TipLine { key: Some(k.into()), ..default() });
        self
    }
    pub fn line(mut self, label: impl Into<String>, value: impl Into<String>) -> Tip {
        self.lines.push(TipLine { label: label.into(), value: value.into(), ..default() });
        self
    }
    pub fn push(&mut self, l: TipLine) -> &mut Tip {
        self.lines.push(l);
        self
    }
}

impl TipLine {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> TipLine {
        TipLine { label: label.into(), value: value.into(), ..default() }
    }
    pub fn icon(mut self, i: Icon, c: Color) -> TipLine {
        self.icon = Some((i, c));
        self
    }
    pub fn color(mut self, c: Color) -> TipLine {
        self.color = Some(c);
        self
    }
    pub fn sub(mut self) -> TipLine {
        self.indent = true;
        self
    }
    pub fn nested(mut self, t: Tip) -> TipLine {
        self.nested = Some(Box::new(t));
        self
    }
    pub fn act(mut self, a: HudAction) -> TipLine {
        self.action = Some(a);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Box,
    Text { s: String, size: f32, color: Color, bold: bool },
    Icon { icon: Icon, px: f32, color: Color },
    Image(Handle<Image>),
}

impl Kind {
    fn same_shape(&self, o: &Kind) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(o)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tag {
    #[default]
    None,
    Minimap,
    TipLayer(u8),
    /// A list the mouse wheel scrolls.
    Scroll,
}

#[derive(Clone, Debug)]
pub struct VNode {
    pub node: Node,
    pub bg: Color,
    pub border: Option<BorderColor>,
    pub kind: Kind,
    /// A carved texture drawn behind the node's children, stretched as a nine-slice.
    pub skin: Option<(Skin, Color)>,
    pub action: Option<HudAction>,
    pub tip: Option<Tip>,
    pub solid: bool,
    pub tag: Tag,
    pub depth: u8,
    /// A beaver whose inspector shows while the pointer is on this node.
    pub peek: Option<sim::colony::BeaverId>,
    /// Shows its tip but lets every click through to the map (a label on the map).
    pub click_through: bool,
    /// A pin over the map: takes a left click, but a right click goes on to the map beneath.
    pub pin: bool,
    /// Vertically scrollable viewport with a themed scrollbar when content overflows.
    pub scroll_y: bool,
    pub children: Vec<VNode>,
}

impl VNode {
    pub fn new(node: Node) -> VNode {
        VNode { node, bg: Color::NONE, border: None, kind: Kind::Box, skin: None, action: None, tip: None, solid: false, tag: Tag::None, depth: 0, peek: None, click_through: false, pin: false, scroll_y: false, children: Vec::new() }
    }
    pub fn skin(&mut self, s: Skin) -> &mut Self {
        self.skin = Some((s, Color::WHITE));
        self.bg = Color::NONE;
        self.border = None;
        self
    }
    pub fn faded(&mut self) -> &mut Self {
        if let Some((s, _)) = self.skin {
            self.skin = Some((s, Color::srgba(1.0, 1.0, 1.0, 0.4)));
        }
        self
    }
    pub fn bg(&mut self, c: Color) -> &mut Self {
        self.bg = c;
        self
    }
    pub fn border(&mut self, b: BorderColor) -> &mut Self {
        self.border = Some(b);
        self
    }
    pub fn tip(&mut self, t: Tip) -> &mut Self {
        self.tip = Some(t);
        self.solid = true;
        self
    }
    pub fn action(&mut self, a: HudAction) -> &mut Self {
        self.action = Some(a);
        self.solid = true;
        self
    }
    pub fn solid(&mut self) -> &mut Self {
        self.solid = true;
        self
    }
    /// Show this beaver's inspector while hovered.
    pub fn peek(&mut self, b: sim::colony::BeaverId) -> &mut Self {
        self.peek = Some(b);
        self.solid = true;
        self
    }
    /// Let clicks through to the map; the tip still shows.
    pub fn click_through(&mut self) -> &mut Self {
        self.click_through = true;
        self
    }
    /// A pin over the map: right clicks go on to the map beneath.
    pub fn pin(&mut self) -> &mut Self {
        self.pin = true;
        self
    }
    pub fn tag(&mut self, t: Tag) -> &mut Self {
        self.tag = t;
        self
    }
    /// Vertically scrollable container with an automatic carved-bark scrollbar when overflowing.
    pub fn scroll_y(&mut self) -> &mut Self {
        self.scroll_y = true;
        self.solid = true;
        self
    }
    pub fn style(&mut self, f: impl FnOnce(&mut Node)) -> &mut Self {
        f(&mut self.node);
        self
    }
    fn interactive(&self) -> bool {
        self.solid || self.action.is_some() || self.tip.is_some() || self.peek.is_some()
    }
    fn same_shape(&self, o: &VNode) -> bool {
        self.kind.same_shape(&o.kind)
            && self.skin.is_some() == o.skin.is_some()
            && self.interactive() == o.interactive()
            && self.click_through == o.click_through
            && self.pin == o.pin
            && self.scroll_y == o.scroll_y
            && self.tag == o.tag
            && self.children.len() == o.children.len()
            && self.children.iter().zip(&o.children).all(|(a, b)| a.same_shape(b))
    }
    fn set_depth(&mut self, d: u8) {
        self.depth = d;
        for c in &mut self.children {
            c.set_depth(d);
        }
    }
}

#[derive(Default)]
pub struct Ui {
    pub nodes: Vec<VNode>,
}

pub fn row_node(gap: f32) -> Node {
    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(gap), ..default() }
}

pub fn col_node(gap: f32) -> Node {
    Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(gap), ..default() }
}

impl Ui {
    pub fn add(&mut self, v: VNode) -> &mut VNode {
        self.nodes.push(v);
        self.nodes.last_mut().unwrap()
    }
    pub fn node(&mut self, node: Node, f: impl FnOnce(&mut Ui)) -> &mut VNode {
        let mut inner = Ui::default();
        f(&mut inner);
        let mut v = VNode::new(node);
        v.children = inner.nodes;
        self.add(v)
    }
    pub fn row(&mut self, gap: f32, f: impl FnOnce(&mut Ui)) -> &mut VNode {
        self.node(row_node(gap), f)
    }
    pub fn col(&mut self, gap: f32, f: impl FnOnce(&mut Ui)) -> &mut VNode {
        self.node(col_node(gap), f)
    }
    pub fn spacer(&mut self) -> &mut VNode {
        self.add(VNode::new(Node { flex_grow: 1.0, ..default() }))
    }
    pub fn text(&mut self, s: impl Into<String>, size: f32, color: Color) -> &mut VNode {
        let mut v = VNode::new(Node::default());
        v.kind = Kind::Text { s: s.into(), size, color, bold: false };
        self.add(v)
    }
    pub fn bold(&mut self, s: impl Into<String>, size: f32, color: Color) -> &mut VNode {
        let mut v = VNode::new(Node::default());
        v.kind = Kind::Text { s: s.into(), size, color, bold: true };
        self.add(v)
    }
    pub fn icon(&mut self, icon: Icon, px: f32, color: Color) -> &mut VNode {
        let mut v = VNode::new(Node { width: Val::Px(px), height: Val::Px(px), flex_shrink: 0.0, ..default() });
        v.kind = Kind::Icon { icon, px, color };
        self.add(v)
    }
    pub fn image(&mut self, h: Handle<Image>, node: Node) -> &mut VNode {
        let mut v = VNode::new(node);
        v.kind = Kind::Image(h);
        self.add(v)
    }
    /// A pebble button: an eight-sided stone around one icon.
    pub fn pebble(&mut self, icon: Icon, px: f32, action: HudAction, look: Peb) -> &mut VNode {
        let side = (px * 1.9).round().max(22.0);
        let tint = if look == Peb::Primary { ON_ACCENT } else if look == Peb::Good { GOOD } else if look == Peb::Faded { INK.with_alpha(0.3) } else { INK };
        let v = self.node(Node { width: Val::Px(side), height: Val::Px(side), justify_content: JustifyContent::Center, align_items: AlignItems::Center, flex_shrink: 0.0, ..default() }, |u| {
            u.icon(icon, px, tint);
        });
        look.apply(v);
        v.action(action)
    }
    pub fn keycap(&mut self, k: impl Into<String>) -> &mut VNode {
        self.bold(k, 11.0, MUTED)
    }
    /// A line carved across a plate: a dark groove with a faint highlight under it.
    pub fn rule(&mut self) -> &mut VNode {
        self.node(Node { flex_direction: FlexDirection::Column, width: Val::Percent(100.0), margin: UiRect::vertical(Val::Px(3.0)), flex_shrink: 0.0, ..default() }, |u| {
            u.add(VNode::new(Node { height: Val::Px(1.0), width: Val::Percent(100.0), ..default() })).bg(GROOVE);
            u.add(VNode::new(Node { height: Val::Px(1.0), width: Val::Percent(100.0), ..default() })).bg(Color::srgba(1.0, 1.0, 1.0, 0.06));
        })
    }
    /// Any amount out of a whole, counted in ticks rather than drawn as a thin bar.
    pub fn bar(&mut self, frac: f32, w: f32, h: f32, fill: Color) -> &mut VNode {
        let n = ((w / 14.0).round() as usize).clamp(3, 12);
        self.ticks(frac, n, w, h.max(6.0), fill)
    }
    pub fn ticks(&mut self, frac: f32, n: usize, w: f32, h: f32, fill: Color) -> &mut VNode {
        let on = (frac.clamp(0.0, 1.0) * n as f32).round() as usize;
        let gap = if w / n as f32 > 10.0 { 3.0 } else { 2.0 };
        let each = ((w - gap * (n as f32 - 1.0)) / n as f32).max(2.0);
        self.row(gap, |r| {
            for i in 0..n {
                r.add(VNode::new(Node { width: Val::Px(each), height: Val::Px(h), border_radius: BorderRadius::all(Val::Px(1.5)), flex_shrink: 0.0, ..default() })).bg(if i < on { fill } else { Color::srgba(1.0, 1.0, 1.0, 0.1) });
            }
        })
    }
    /// Condition as a row of five pips.
    pub fn pips(&mut self, frac: f32, col: Color) -> &mut VNode {
        let on = (frac.clamp(0.0, 1.0) * 5.0).round() as usize;
        self.row(4.0, |r| {
            for i in 0..5 {
                r.add(VNode::new(Node { width: Val::Px(12.0), height: Val::Px(8.0), border_radius: BorderRadius::all(Val::Px(1.5)), flex_shrink: 0.0, ..default() })).bg(if i < on { col } else { Color::srgba(1.0, 1.0, 1.0, 0.12) });
            }
        })
    }
    /// A beaver as a token; `pip` is a small status dot under it.
    pub fn token(&mut self, icon: Icon, px: f32, pip: Option<Color>) -> &mut VNode {
        self.node(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(3.0), flex_shrink: 0.0, ..default() }, |t| {
            t.icon(icon, px, FUR);
            if let Some(c) = pip {
                t.add(VNode::new(Node { width: Val::Px(6.0), height: Val::Px(6.0), border_radius: BorderRadius::all(Val::Px(3.0)), ..default() })).bg(c);
            }
        })
    }
    /// An open place a beaver could fill.
    pub fn empty_token(&mut self, px: f32) -> &mut VNode {
        let v = self.add(VNode::new(Node { width: Val::Px(px), height: Val::Px(px), border: UiRect::all(Val::Px(1.5)), border_radius: BorderRadius::all(Val::Px(px / 2.0)), flex_shrink: 0.0, ..default() }));
        v.border = Some(BorderColor::all(MUTED.with_alpha(0.55)));
        v
    }
    /// The small amber "!" on the button the panel's state asks for.
    pub fn hint_badge(&mut self) -> &mut VNode {
        let v = self.node(
            Node { position_type: PositionType::Absolute, top: Val::Px(-7.0), right: Val::Px(-7.0), width: Val::Px(17.0), height: Val::Px(17.0), border_radius: BorderRadius::all(Val::Px(8.5)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            |b| {
                b.bold("!", 12.0, ON_ACCENT);
            },
        );
        v.bg(ACCENT)
    }
    /// One of a row of large buttons: icon, word and cost. Without an action it stays in its
    /// place, faded, and its tooltip says why.
    pub fn big_button(&mut self, b: BigButton) -> &mut VNode {
        let ok = b.action.is_some();
        let ink = if ok { INK } else { INK.with_alpha(0.35) };
        let v = self.node(
            Node { flex_grow: 1.0, flex_basis: Val::Px(0.0), height: Val::Px(54.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::Center, row_gap: Val::Px(2.0), ..default() },
            |x| {
                x.icon(b.icon, 16.0, ink);
                x.bold(b.word, 12.0, ink);
                x.row(3.0, |c| {
                    if let Some((i, col, t)) = &b.cost {
                        c.icon(*i, 12.0, if ok { *col } else { col.with_alpha(0.35) });
                        c.bold(t.clone(), 12.0, ink);
                    }
                });
                if b.hint {
                    x.hint_badge();
                }
            },
        );
        v.skin(if b.on { Skin::PebbleOn } else { Skin::Pebble });
        if !ok {
            v.faded();
        }
        v.tip(b.tip);
        if let Some(a) = b.action {
            v.action(a);
        }
        v
    }
    /// A small button: an icon, a word or both. Without an action it stays, faded, and its
    /// tooltip says why.
    pub fn small_button(&mut self, icon: Option<Icon>, word: Option<&str>, action: Option<HudAction>, tip: Tip, look: Small) -> &mut VNode {
        let ok = action.is_some();
        let col = match look {
            Small::Danger | Small::Armed => BAD,
            _ => INK,
        };
        let col = if ok { col } else { col.with_alpha(0.35) };
        let hint = look == Small::Hint;
        let v = self.row(5.0, |r| {
            if let Some(i) = icon {
                r.icon(i, 14.0, col);
            }
            if let Some(w) = word {
                r.bold(w, 12.0, col);
            }
            if hint {
                r.hint_badge();
            }
        });
        v.node.height = Val::Px(28.0);
        v.node.min_width = Val::Px(28.0);
        v.node.padding = UiRect::axes(Val::Px(if word.is_some() { 9.0 } else { 6.0 }), Val::Px(0.0));
        v.node.justify_content = JustifyContent::Center;
        v.node.flex_shrink = 0.0;
        v.skin(match look {
            Small::On => Skin::PebbleOn,
            Small::Armed => Skin::TokenBad,
            _ => Skin::Pebble,
        });
        if !ok {
            v.faded();
        }
        v.tip(tip);
        if let Some(a) = action {
            v.action(a);
        }
        v
    }
    /// A reading: icon, number, and an optional quiet word after it.
    pub fn stat(&mut self, icon: Icon, icon_col: Color, value: impl Into<String>, word: Option<&str>, col: Color, tip: Tip) -> &mut VNode {
        let value = value.into();
        self.row(4.0, |x| {
            x.icon(icon, 14.0, icon_col);
            x.bold(value, 13.0, col);
            if let Some(w) = word {
                x.text(w, 12.0, MUTED);
            }
        })
        .tip(tip)
    }
    /// The one close button, top right of every panel and page.
    pub fn close_button(&mut self, action: HudAction, esc: bool) -> &mut VNode {
        let tip = Tip::name("Close");
        self.pebble(Icon::Cross, 11.0, action, Peb::Bare).tip(if esc { tip.key("Esc") } else { tip })
    }
    /// Back to the page this one was opened from, named.
    pub fn back_button(&mut self, to: &str) -> &mut VNode {
        self.small_button(Some(Icon::Back), Some(to), Some(HudAction::PageBack), Tip::name("Back").key("Esc"), Small::Idle)
    }
    /// A portrait: one large icon in a dark cut-corner frame.
    pub fn portrait(&mut self, icon: Icon, col: Color) -> &mut VNode {
        let v = self.node(Node { width: Val::Px(52.0), height: Val::Px(52.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, flex_shrink: 0.0, ..default() }, |p| {
            p.icon(icon, 32.0, col);
        });
        v.skin(Skin::Portrait);
        v
    }
    /// A themed vertical scrollbar indicator for row-paged lists (`first`, `visible`, `total`).
    pub fn slice_scrollbar(&mut self, first: usize, visible: usize, total: usize, height: f32) {
        if total <= visible || height <= 1.0 {
            return;
        }
        let w = super::scrollbar::TRACK_W;
        let thumb = ((visible as f32 / total as f32) * height).clamp(24.0f32.min(height), height);
        let at = (first as f32 / (total - visible) as f32).clamp(0.0, 1.0) * (height - thumb);
        let track = self.node(
            Node {
                width: Val::Px(w),
                height: Val::Px(height),
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(w * 0.5)),
                ..default()
            },
            |bar| {
                bar.add(VNode::new(Node { height: Val::Px(at), flex_shrink: 0.0, ..default() }));
                bar.add(VNode::new(Node {
                    width: Val::Px(w - 2.0),
                    height: Val::Px(thumb),
                    border_radius: BorderRadius::all(Val::Px((w - 2.0) * 0.5)),
                    flex_shrink: 0.0,
                    ..default()
                }))
                .bg(BARK.with_alpha(0.84));
            },
        );
        track.bg(GROOVE).border(BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.08)));
    }
}

pub const FUR: Color = Color::srgb(0.73, 0.47, 0.25);
pub const MUD: Color = Color::srgb(0.56, 0.43, 0.32);
pub const REST: Color = Color::srgb(0.56, 0.72, 0.88);

/// A large button's contents (see `Ui::big_button`).
pub struct BigButton<'a> {
    pub icon: Icon,
    pub word: &'a str,
    pub cost: Option<(Icon, Color, String)>,
    /// None: shown faded, the tooltip saying why.
    pub action: Option<HudAction>,
    pub tip: Tip,
    /// The fix the panel's state points at: marked with the "!".
    pub hint: bool,
    pub on: bool,
}

/// The looks a small button can take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Small {
    Idle,
    On,
    /// The fix the panel's state points at: marked with the "!".
    Hint,
    /// Takes something away.
    Danger,
    /// Takes something away on the next click.
    Armed,
}

/// The looks a pebble button can take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Peb {
    Idle,
    On,
    Primary,
    Good,
    /// Shown but not usable.
    Faded,
    /// No stone, just the icon.
    Bare,
}

impl Peb {
    fn apply(self, v: &mut VNode) {
        match self {
            Peb::Idle => {
                v.skin(Skin::Pebble);
            }
            Peb::On => {
                v.skin(Skin::PebbleOn);
            }
            Peb::Primary => {
                v.skin(Skin::PebblePrimary);
            }
            Peb::Good => {
                v.skin(Skin::PebbleGood);
            }
            Peb::Faded => {
                v.skin(Skin::Pebble).faded();
            }
            Peb::Bare => {}
        }
    }
}

/// Every panel is a carved plate.
pub fn panel(v: &mut VNode) -> &mut VNode {
    v.skin(Skin::Plate);
    v.node.border = UiRect::ZERO;
    v.node.border_radius = BorderRadius::ZERO;
    if v.node.padding == UiRect::default() {
        v.node.padding = UiRect::all(Val::Px(10.0));
    }
    v.solid = true;
    v
}

#[derive(Component, Clone)]
pub struct TipComp {
    pub tip: Tip,
    pub depth: u8,
}

#[derive(Component)]
pub struct TagComp(pub Tag);

/// A pin over the map (see `VNode::pin`).
#[derive(Component)]
pub struct MapPin;

/// The beaver a node peeks at (see `VNode::peek`).
#[derive(Component, Clone, Copy, PartialEq)]
pub struct Peek(pub sim::colony::BeaverId);

#[derive(Component, Default)]
pub struct Mounted {
    tree: Vec<VNode>,
    ents: Vec<MountedNode>,
}

struct MountedNode {
    e: Entity,
    children: Vec<MountedNode>,
}

fn kind_components(icons: &Icons, k: &Kind, node: &Node) -> KindBundle {
    match k {
        Kind::Box => KindBundle::None,
        Kind::Text { s, size, color, bold } => KindBundle::Text(Text::new(s.clone()), font(*size, *bold), TextColor(*color)),
        Kind::Icon { icon, px, color } => {
            let (img, mut n) = icon_node(icons, *icon, *px, *color);
            n.margin = node.margin;
            KindBundle::Icon(img, n)
        }
        Kind::Image(h) => KindBundle::Image(ImageNode::new(h.clone())),
    }
}

enum KindBundle {
    None,
    Text(Text, TextFont, TextColor),
    Icon(ImageNode, Node),
    Image(ImageNode),
}

/// A skinned button brightens while the pointer is over it.
#[derive(Component, Clone, Copy)]
pub struct Lift(pub Color);

fn skin_image(icons: &Icons, s: Skin, tint: Color) -> ImageNode {
    let b = s.slice();
    let mode = if b > 0.0 {
        NodeImageMode::Sliced(TextureSlicer { border: BorderRect::all(b), center_scale_mode: SliceScaleMode::Stretch, sides_scale_mode: SliceScaleMode::Stretch, max_corner_scale: 1.0 })
    } else {
        NodeImageMode::Stretch
    };
    ImageNode::new(icons.skin(s)).with_mode(mode).with_color(tint).with_visual_box(bevy::ui::VisualBox::BorderBox)
}

pub fn lift_hover(mut q: Query<(&Interaction, &Lift, &mut ImageNode), Or<(Changed<Interaction>, Changed<Lift>)>>) {
    for (i, lift, mut img) in &mut q {
        let base = lift.0.to_srgba();
        img.color = match i {
            Interaction::None => lift.0,
            _ => Color::srgba((base.red * 1.3).min(1.0), (base.green * 1.3).min(1.0), (base.blue * 1.3).min(1.0), base.alpha),
        };
    }
}

fn outer_scroll_node(n: &Node) -> Node {
    let mut out = n.clone();
    out.overflow = Overflow::clip();
    out.flex_direction = FlexDirection::Column;
    out.row_gap = Val::Px(0.0);
    out.column_gap = Val::Px(0.0);
    out.padding = UiRect::ZERO;
    out
}

fn inner_scroll_node(n: &Node) -> Node {
    let pad_r = match n.padding.right {
        Val::Px(px) => px + 14.0,
        _ => 14.0,
    };
    Node {
        width: Val::Percent(100.0),
        max_height: Val::Percent(100.0),
        flex_grow: 1.0,
        min_height: Val::Px(0.0),
        flex_direction: n.flex_direction,
        align_items: n.align_items,
        justify_content: n.justify_content,
        row_gap: n.row_gap,
        column_gap: n.column_gap,
        padding: UiRect { right: Val::Px(pad_r), ..n.padding },
        overflow: Overflow::scroll_y(),
        ..default()
    }
}

fn spawn_tree(p: &mut ChildSpawnerCommands, icons: &Icons, v: &VNode) -> MountedNode {
    let node = if v.scroll_y { outer_scroll_node(&v.node) } else { v.node.clone() };
    let mut ec = p.spawn((node, BackgroundColor(v.bg), FocusPolicy::Pass));
    if let Some(b) = v.border {
        ec.insert(b);
    }
    if let (Some((s, tint)), Kind::Box) = (v.skin, &v.kind) {
        ec.insert(skin_image(icons, s, tint));
        if v.action.is_some() {
            ec.insert(Lift(tint));
        }
    }
    match kind_components(icons, &v.kind, &v.node) {
        KindBundle::None => {}
        KindBundle::Text(t, f, c) => {
            ec.insert((t, f, c));
        }
        KindBundle::Icon(img, n) => {
            ec.insert((img, n));
        }
        KindBundle::Image(img) => {
            ec.insert(img);
        }
    }
    if v.interactive() {
        if v.click_through {
            ec.insert((Interaction::default(), FocusPolicy::Pass));
        } else {
            ec.insert((Interaction::default(), FocusPolicy::Block, Blocks));
            if v.pin {
                ec.insert(MapPin);
            }
        }
    }
    if let Some(a) = &v.action {
        ec.insert(a.clone());
    }
    if let Some(t) = &v.tip {
        ec.insert(TipComp { tip: t.clone(), depth: v.depth });
    }
    if let Some(b) = v.peek {
        ec.insert(Peek(b));
    }
    if v.tag != Tag::None {
        ec.insert(TagComp(v.tag));
        if matches!(v.tag, Tag::Minimap | Tag::TipLayer(_) | Tag::Scroll) {
            ec.insert(RelativeCursorPosition::default());
        }
    }
    let e = ec.id();
    let mut children = Vec::new();
    if v.scroll_y {
        ec.with_children(|cp| {
            let vp = cp
                .spawn((
                    inner_scroll_node(&v.node),
                    super::scrollbar::ScrollViewport,
                    ScrollPosition::default(),
                    RelativeCursorPosition::default(),
                    FocusPolicy::Pass,
                ))
                .with_children(|vc| {
                    for c in &v.children {
                        children.push(spawn_tree(vc, icons, c));
                    }
                })
                .id();
            super::scrollbar::spawn_scrollbar(cp, vp, 2.0, 2.0, 2.0);
        });
    } else if !v.children.is_empty() {
        ec.with_children(|cp| {
            for c in &v.children {
                children.push(spawn_tree(cp, icons, c));
            }
        });
    }
    MountedNode { e, children }
}

fn update_tree(cmd: &mut Commands, icons: &Icons, old: &VNode, new: &VNode, m: &MountedNode) {
    let mut ec = cmd.entity(m.e);
    if old.node != new.node && !matches!(new.kind, Kind::Icon { .. }) {
        let node = if new.scroll_y { outer_scroll_node(&new.node) } else { new.node.clone() };
        ec.insert(node);
    }
    if old.bg != new.bg {
        ec.insert(BackgroundColor(new.bg));
    }
    if old.skin != new.skin && matches!(new.kind, Kind::Box) {
        match new.skin {
            Some((s, tint)) => {
                ec.insert((skin_image(icons, s, tint), Lift(tint)));
            }
            None => {
                ec.remove::<(ImageNode, Lift)>();
            }
        }
    }
    if old.border != new.border {
        match new.border {
            Some(b) => {
                ec.insert(b);
            }
            None => {
                ec.insert(BorderColor::all(Color::NONE));
            }
        }
    }
    if old.kind != new.kind || (matches!(new.kind, Kind::Icon { .. }) && old.node != new.node) {
        match kind_components(icons, &new.kind, &new.node) {
            KindBundle::None => {}
            KindBundle::Text(t, f, c) => {
                ec.insert((t, f, c));
            }
            KindBundle::Icon(img, n) => {
                ec.insert((img, n));
            }
            KindBundle::Image(img) => {
                ec.insert(img);
            }
        }
    }
    if old.action != new.action {
        match &new.action {
            Some(a) => {
                ec.insert(a.clone());
            }
            None => {
                ec.remove::<HudAction>();
            }
        }
    }
    if old.peek != new.peek {
        match new.peek {
            Some(b) => {
                ec.insert(Peek(b));
            }
            None => {
                ec.remove::<Peek>();
            }
        }
    }
    if old.tip != new.tip || old.depth != new.depth {
        match &new.tip {
            Some(t) => {
                ec.insert(TipComp { tip: t.clone(), depth: new.depth });
            }
            None => {
                ec.remove::<TipComp>();
            }
        }
    }
    for ((o, n), mc) in old.children.iter().zip(&new.children).zip(&m.children) {
        update_tree(cmd, icons, o, n, mc);
    }
}

pub fn reconcile(cmd: &mut Commands, icons: &Icons, host: Entity, mounted: &mut Mounted, mut tree: Vec<VNode>, depth: u8) {
    for v in &mut tree {
        v.set_depth(depth);
    }
    let same = mounted.tree.len() == tree.len() && mounted.tree.iter().zip(&tree).all(|(a, b)| a.same_shape(b));
    if same {
        for ((o, n), m) in mounted.tree.iter().zip(&tree).zip(&mounted.ents) {
            update_tree(cmd, icons, o, n, m);
        }
    } else {
        cmd.entity(host).despawn_related::<Children>();
        let mut ents = Vec::new();
        cmd.entity(host).with_children(|p| {
            for v in &tree {
                ents.push(spawn_tree(p, icons, v));
            }
        });
        mounted.ents = ents;
    }
    mounted.tree = tree;
}

pub fn clear(cmd: &mut Commands, host: Entity, mounted: &mut Mounted) {
    if !mounted.tree.is_empty() {
        cmd.entity(host).despawn_related::<Children>();
        mounted.tree.clear();
        mounted.ents.clear();
    }
}

pub const TIP_LAYERS: usize = 2;
const OPEN_SECS: f32 = 0.3;
const FREEZE_SECS: f32 = 1.5;
const GRACE_SECS: f32 = 0.25;
const FROZEN_GRACE_SECS: f32 = 0.7;

#[derive(Clone, Debug, PartialEq)]
pub enum TipSrc {
    Ui(Entity),
    World(u64),
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub src: TipSrc,
    pub tip: Tip,
    pub pos: Vec2,
    pub frozen: bool,
    pub opened: f32,
    pub last_held: f32,
}

#[derive(Resource, Default)]
pub struct Tips {
    pub layers: Vec<Layer>,
    pending: Option<(TipSrc, u8, Tip, f32)>,
    pub world: Option<(u64, Tip)>,
    pub sizes: [Vec2; TIP_LAYERS],
}

#[derive(Component)]
pub struct TipHost(pub u8);

pub fn spawn_tip_hosts(mut commands: Commands) {
    for i in 0..TIP_LAYERS {
        commands.spawn((
            Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), ..default() },
            GlobalZIndex(1000 + i as i32),
            FocusPolicy::Pass,
            TipHost(i as u8),
            Mounted::default(),
            Visibility::Hidden,
        ));
    }
}

fn cursor_logical(windows: &Query<&Window, With<PrimaryWindow>>, scale: f32) -> Option<Vec2> {
    windows.single().ok()?.cursor_position().map(|p| p / scale.max(0.01))
}

#[allow(clippy::too_many_arguments)]
pub fn tip_logic(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Res<UiScale>,
    hovered: Query<(Entity, &Interaction, &TipComp)>,
    layer_roots: Query<(&TagComp, &RelativeCursorPosition)>,
    over_ui: Res<super::PointerOverUi>,
    mut tips: ResMut<Tips>,
) {
    let _p = ::sim::prof::scope("game/tip logic");
    let now = time.elapsed_secs();
    let Some(cursor) = cursor_logical(&windows, ui_scale.0) else {
        tips.layers.retain(|l| l.frozen);
        return;
    };
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    let mut on: Option<(TipSrc, u8, Tip)> = None;
    for (e, i, t) in &hovered {
        if *i != Interaction::None && on.as_ref().is_none_or(|o| t.depth > o.1) {
            on = Some((TipSrc::Ui(e), t.depth, t.tip.clone()));
        }
    }
    if on.is_none() && !over_ui.0 {
        if let Some((k, t)) = &tips.world {
            on = Some((TipSrc::World(*k), 0, t.clone()));
        }
    }
    let mut over_layer = [false; TIP_LAYERS];
    for (tag, rel) in &layer_roots {
        if let Tag::TipLayer(n) = tag.0 {
            if let Some(p) = rel.normalized {
                if p.x.abs() <= 0.5 && p.y.abs() <= 0.5 {
                    over_layer[n as usize] = true;
                }
            }
        }
    }
    let mut deeper_held = false;
    for k in (0..tips.layers.len()).rev() {
        let src_on = on.as_ref().is_some_and(|o| o.0 == tips.layers[k].src);
        let held = src_on || over_layer.get(k).copied().unwrap_or(false) || deeper_held;
        if held {
            tips.layers[k].last_held = now;
        }
        deeper_held = held || deeper_held;
        let world = matches!(tips.layers[k].src, TipSrc::World(_));
        if src_on && !world && !tips.layers[k].frozen && now - tips.layers[k].opened > FREEZE_SECS {
            tips.layers[k].frozen = true;
        }
    }
    if let Some(first) = tips.layers.iter().position(|l| now - l.last_held > if l.frozen { FROZEN_GRACE_SECS } else { GRACE_SECS }) {
        tips.layers.truncate(first);
    }
    if mouse.just_pressed(MouseButton::Middle) {
        if let Some(l) = tips.layers.last_mut() {
            l.frozen = true;
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        tips.layers.clear();
    }
    match on {
        Some((src, depth, tip)) => {
            let d = depth as usize;
            let already = tips.layers.get(d).is_some_and(|l| l.src == src);
            // A map tip rides with the pointer and changes as soon as the pointer reaches
            // something else, so it always describes what is under the pointer now.
            let world = matches!(src, TipSrc::World(_));
            let swap = world && d == 0 && tips.layers.first().is_some_and(|l| matches!(l.src, TipSrc::World(_)) && !l.frozen);
            if already || swap {
                if swap && !already {
                    tips.layers.truncate(1);
                    tips.layers[0].src = src;
                    tips.layers[0].opened = now;
                }
                let l = &mut tips.layers[d];
                if world && !l.frozen {
                    l.pos = cursor + Vec2::new(16.0, 20.0);
                    l.last_held = now;
                }
                l.tip = tip;
                tips.pending = None;
            } else if d <= tips.layers.len() && d < TIP_LAYERS {
                let since = match &tips.pending {
                    Some((s, dd, _, t0)) if *s == src && *dd == depth => *t0,
                    _ => now,
                };
                tips.pending = Some((src.clone(), depth, tip.clone(), since));
                if alt || now - since >= OPEN_SECS {
                    tips.layers.truncate(d);
                    tips.layers.push(Layer { src, tip, pos: cursor + Vec2::new(16.0, 20.0), frozen: false, opened: now, last_held: now });
                    tips.pending = None;
                }
            }
        }
        None => tips.pending = None,
    }
}

pub fn tip_draw(
    mut commands: Commands,
    icons: Res<Icons>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Res<UiScale>,
    mut tips: ResMut<Tips>,
    mut hosts: Query<(Entity, &TipHost, &mut Mounted, &mut Visibility, &mut Node, Option<&Children>)>,
    computed: Query<&ComputedNode>,
) {
    let win = windows.single().map(|w| Vec2::new(w.width(), w.height()) / ui_scale.0.max(0.01)).unwrap_or(Vec2::new(1280.0, 720.0));
    for (_, h, _, _, _, children) in &hosts {
        if let Some(c) = children.and_then(|c| c.first()).and_then(|c| computed.get(*c).ok()) {
            tips.sizes[h.0 as usize] = c.size() * c.inverse_scale_factor();
        }
    }
    let sizes = tips.sizes;
    let layers = tips.layers.clone();
    for (e, h, mut m, mut vis, mut node, _) in &mut hosts {
        draw_layer(&mut commands, &icons, e, h.0, &layers, sizes, win, &mut m, &mut vis, &mut node);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_layer(commands: &mut Commands, icons: &Icons, e: Entity, k: u8, layers: &[Layer], sizes: [Vec2; TIP_LAYERS], win: Vec2, m: &mut Mounted, vis: &mut Visibility, node: &mut Node) {
    match layers.get(k as usize) {
        Some(l) => {
            let size = sizes[k as usize];
            let mut x = l.pos.x;
            let mut y = l.pos.y;
            if x + size.x > win.x - 8.0 {
                x = (l.pos.x - size.x - 24.0).max(8.0);
            }
            if y + size.y > win.y - 8.0 {
                y = (win.y - size.y - 8.0).max(8.0);
            }
            node.left = Val::Px(x);
            node.top = Val::Px(y);
            let tree = vec![tip_tree(&l.tip, l.frozen, k)];
            reconcile(commands, icons, e, m, tree, k + 1);
            if *vis != Visibility::Inherited {
                *vis = Visibility::Inherited;
            }
        }
        None => {
            clear(commands, e, m);
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
        }
    }
}

pub fn tip_tree(t: &Tip, frozen: bool, layer: u8) -> VNode {
    let mut ui = Ui::default();
    ui.col(3.0, |u| {
        if let Some((icon, title, value)) = &t.title {
            u.row(6.0, |r| {
                if let Some((i, c)) = icon {
                    r.icon(*i, 16.0, *c);
                }
                r.bold(title.clone(), 13.0, INK);
                r.spacer();
                if !value.is_empty() {
                    r.bold(value.clone(), 13.0, INK);
                }
                if frozen {
                    r.icon(Icon::Pin, 12.0, MUTED);
                }
            })
            .style(|n| n.min_width = Val::Px(120.0));
        }
        for l in &t.lines {
            let v = u.row(6.0, |r| {
                if l.indent {
                    r.add(VNode::new(Node { width: Val::Px(10.0), ..default() }));
                }
                if let Some((i, c)) = l.icon {
                    r.icon(i, 14.0, if l.action.is_some() { ON_ACCENT } else { c });
                }
                if !l.label.is_empty() {
                    let col = if l.action.is_some() { ON_ACCENT } else if l.indent { MUTED } else { INK };
                    if l.action.is_some() { r.bold(l.label.clone(), 12.0, col); } else { r.text(l.label.clone(), 12.0, col); }
                }
                r.spacer();
                if !l.value.is_empty() {
                    r.bold(l.value.clone(), 12.0, l.color.unwrap_or(INK));
                }
                if let Some(k) = &l.key {
                    r.keycap(k.clone());
                }
                if l.nested.is_some() {
                    r.icon(Icon::Expand, 10.0, MUTED);
                }
            });
            v.node.min_width = Val::Px(140.0);
            if let Some(n) = &l.nested {
                v.tip(*n.clone());
                v.bg = Color::srgba(1.0, 1.0, 1.0, 0.03);
            }
            if let Some(a) = &l.action {
                v.skin(Skin::PebblePrimary);
                v.node.padding = UiRect::axes(Val::Px(8.0), Val::Px(5.0));
                v.node.margin = UiRect::top(Val::Px(3.0));
                v.action(a.clone());
            }
        }
        if t.title.is_none() && t.lines.is_empty() {
            u.text("-", 12.0, MUTED);
        }
    });
    let mut root = ui.nodes.pop().unwrap();
    root.node.max_width = Val::Px(340.0);
    panel(&mut root);
    root.node.padding = UiRect::axes(Val::Px(12.0), Val::Px(10.0));
    root.tag = Tag::TipLayer(layer);
    root
}
