#![allow(deprecated)]
use super::{Blocks, HudAction};
use crate::icons::{icon_node, Icon, Icons};
use bevy::prelude::*;
use bevy::text::{FontSize, FontWeight};
use bevy::ui::{FocusPolicy, RelativeCursorPosition};
use bevy::window::PrimaryWindow;

pub const PANEL: Color = Color::srgba(0.106, 0.09, 0.075, 0.94);
pub const TIMBER: Color = Color::srgb(0.59, 0.41, 0.24);
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

pub fn font(size: f32, bold: bool) -> TextFont {
    TextFont { font_size: FontSize::Px(size), weight: if bold { FontWeight::BOLD } else { FontWeight::NORMAL }, ..default() }
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
    pub fn row(mut self, l: TipLine) -> Tip {
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
}

#[derive(Clone, Debug)]
pub struct VNode {
    pub node: Node,
    pub bg: Color,
    pub border: Option<BorderColor>,
    pub kind: Kind,
    pub action: Option<HudAction>,
    pub tip: Option<Tip>,
    pub solid: bool,
    pub tag: Tag,
    pub depth: u8,
    pub children: Vec<VNode>,
}

impl VNode {
    pub fn new(node: Node) -> VNode {
        VNode { node, bg: Color::NONE, border: None, kind: Kind::Box, action: None, tip: None, solid: false, tag: Tag::None, depth: 0, children: Vec::new() }
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
    pub fn tag(&mut self, t: Tag) -> &mut Self {
        self.tag = t;
        self
    }
    pub fn style(&mut self, f: impl FnOnce(&mut Node)) -> &mut Self {
        f(&mut self.node);
        self
    }
    fn interactive(&self) -> bool {
        self.solid || self.action.is_some() || self.tip.is_some()
    }
    fn same_shape(&self, o: &VNode) -> bool {
        self.kind.same_shape(&o.kind)
            && self.interactive() == o.interactive()
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
    pub fn num(&mut self, s: impl Into<String>, size: f32, color: Color, w: f32) -> &mut VNode {
        if w <= 0.0 {
            return self.bold(s, size, color);
        }
        let s = s.into();
        self.node(Node { width: Val::Px(w), justify_content: JustifyContent::FlexEnd, ..default() }, |u| {
            u.bold(s, size, color);
        })
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
    pub fn swatch(&mut self, c: Color, w: f32, h: f32) -> &mut VNode {
        let mut v = VNode::new(Node { width: Val::Px(w), height: Val::Px(h), border_radius: BorderRadius::all(Val::Px(2.0)), flex_shrink: 0.0, ..default() });
        v.bg = c;
        self.add(v)
    }
    pub fn icon_button(&mut self, icon: Icon, px: f32, action: HudAction, fill: Color, on: bool) -> &mut VNode {
        let tint = if fill == ACCENT { ON_ACCENT } else { INK };
        let pad = (px * 0.3).round();
        let v = self.node(button_node(pad, on), |u| {
            u.icon(icon, px, tint);
        });
        v.bg = fill;
        v.border = Some(BorderColor::all(if on { SELECT } else { LINE }));
        v.action(action)
    }
    pub fn icon_label_button(&mut self, icon: Icon, label: impl Into<String>, px: f32, action: HudAction, fill: Color, on: bool) -> &mut VNode {
        let tint = if fill == ACCENT { ON_ACCENT } else { INK };
        let label = label.into();
        let v = self.node(Node { column_gap: Val::Px(4.0), ..button_node(4.0, on) }, |u| {
            u.icon(icon, px, tint);
            u.bold(label, 12.0, tint);
        });
        v.bg = fill;
        v.border = Some(BorderColor::all(if on { SELECT } else { LINE }));
        v.action(action)
    }
    pub fn keycap(&mut self, k: impl Into<String>) -> &mut VNode {
        let k = k.into();
        let v = self.node(
            Node { padding: UiRect::axes(Val::Px(4.0), Val::Px(1.0)), border: UiRect::all(Val::Px(1.0)), border_radius: BorderRadius::all(Val::Px(3.0)), min_width: Val::Px(16.0), justify_content: JustifyContent::Center, ..default() },
            |u| {
                u.bold(k, 10.5, INK);
            },
        );
        v.border = Some(BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.35)));
        v.bg = Color::srgba(1.0, 1.0, 1.0, 0.05);
        v
    }
    pub fn rule(&mut self) -> &mut VNode {
        let v = self.add(VNode::new(Node { height: Val::Px(1.0), width: Val::Percent(100.0), margin: UiRect::vertical(Val::Px(2.0)), ..default() }));
        v.bg = LINE;
        v
    }
    pub fn bar(&mut self, frac: f32, w: f32, h: f32, fill: Color) -> &mut VNode {
        let f = frac.clamp(0.0, 1.0);
        let v = self.node(Node { width: Val::Px(w), height: Val::Px(h), border_radius: BorderRadius::all(Val::Px(2.0)), overflow: Overflow::clip(), ..default() }, |u| {
            u.add(VNode::new(Node { width: Val::Percent(f * 100.0), height: Val::Percent(100.0), ..default() })).bg(fill);
        });
        v.bg = Color::srgba(1.0, 1.0, 1.0, 0.1);
        v
    }
}

pub fn button_node(pad: f32, on: bool) -> Node {
    Node {
        padding: UiRect::all(Val::Px(pad)),
        border: UiRect::all(Val::Px(if on { 2.0 } else { 1.0 })),
        border_radius: BorderRadius::all(Val::Px(5.0)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        flex_shrink: 0.0,
        ..default()
    }
}

pub fn panel(v: &mut VNode) -> &mut VNode {
    v.bg = PANEL;
    v.border = Some(BorderColor { top: TIMBER, ..BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.06)) });
    v.node.border = UiRect { top: Val::Px(2.0), ..UiRect::all(Val::Px(1.0)) };
    v.node.border_radius = BorderRadius::all(Val::Px(7.0));
    if v.node.padding == UiRect::default() {
        v.node.padding = UiRect::all(Val::Px(8.0));
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

fn spawn_tree(p: &mut ChildSpawnerCommands, icons: &Icons, v: &VNode) -> MountedNode {
    let mut ec = p.spawn((v.node.clone(), BackgroundColor(v.bg), FocusPolicy::Pass));
    if let Some(b) = v.border {
        ec.insert(b);
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
        ec.insert((Interaction::default(), FocusPolicy::Block, Blocks));
    }
    if let Some(a) = &v.action {
        ec.insert(a.clone());
    }
    if let Some(t) = &v.tip {
        ec.insert(TipComp { tip: t.clone(), depth: v.depth });
    }
    if v.tag != Tag::None {
        ec.insert(TagComp(v.tag));
        if matches!(v.tag, Tag::Minimap | Tag::TipLayer(_)) {
            ec.insert(RelativeCursorPosition::default());
        }
    }
    let e = ec.id();
    let mut children = Vec::new();
    if !v.children.is_empty() {
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
        ec.insert(new.node.clone());
    }
    if old.bg != new.bg {
        ec.insert(BackgroundColor(new.bg));
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

pub const TIP_LAYERS: usize = 3;
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
        if src_on && !tips.layers[k].frozen && now - tips.layers[k].opened > FREEZE_SECS {
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
            if already {
                tips.layers[d].tip = tip;
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
                    r.icon(i, 14.0, c);
                }
                if !l.label.is_empty() {
                    r.text(l.label.clone(), 12.0, if l.indent { MUTED } else { INK });
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
        }
        if t.title.is_none() && t.lines.is_empty() {
            u.text("-", 12.0, MUTED);
        }
    });
    let mut root = ui.nodes.pop().unwrap();
    root.node.max_width = Val::Px(340.0);
    panel(&mut root);
    root.node.padding = UiRect::axes(Val::Px(9.0), Val::Px(7.0));
    root.tag = Tag::TipLayer(layer);
    root
}
