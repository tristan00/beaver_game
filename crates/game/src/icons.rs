use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::f32::consts::TAU;

pub const CELL: u32 = 64;
pub const COLUMNS: u32 = 16;
const MIPS: u32 = 4;

macro_rules! icons {
    ($($name:ident),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Icon { $($name),* }
        impl Icon {
            pub const ALL: &'static [Icon] = &[$(Icon::$name),*];
            pub fn index(self) -> usize { self as usize }
            /// The icon a content row names.
            pub fn named(name: &str) -> Option<Icon> {
                match name { $(stringify!($name) => Some(Icon::$name),)* _ => None }
            }
        }
    };
}

icons! {
    Food, Logs, Sticks, Mud,
    Dam, Lodge, Canal, Lock,
    StickPile, FellingCamp,
    RoleForager, RoleFeller, RoleMudDigger, RoleBuilder, RoleHauler,
    ViewWorld, ViewWater, ViewForest,
    ViewElevation, ViewDeposits,
    Idle, Unstaffed, Blocked, Full, Warn, Urgent, Up, Down,
    Check, Cross,
    Pause, Play, Fast2, Fast3, Undo, Pin, Plus, Minus, Focus,
    Expand, Colony, Jobs, Follow, Area, Haul, Back,
    Beaver, Adult, Yearling, Kit, MoodGood, MoodOk, MoodBad, Health,
    Spring, Summer, Autumn, Winter, DrySummer,
    Hourglass, Walk, Tree, Plant, MudBank, Hammer, Crest, Rise, Flood, Push, Drop, Site,
    Building, Home, Upstream, Gauge,
    Widen, Extend,
    Pebbles, Sapling, FoodCache, RolePlanter, PebbleHeap, Forester, WaterPlant, RoleMender, HaulMound,
    RolePebbler,
    Wolf, Ledger, Territory, Journal, Lookout, RoleSentry, RaftLanding, LockGates, Raft,
    Deer, Rabbit, Bird,
    Cloud, Rain, Storm, Blizzard,
    Remove, Next,
}

#[derive(Resource, Clone)]
pub struct Icons {
    pub image: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
    pub skins: Vec<Handle<Image>>,
}

impl Icons {
    pub fn skin(&self, s: crate::skins::Skin) -> Handle<Image> {
        self.skins.get(s as usize).cloned().unwrap_or_default()
    }
}

pub struct IconsPlugin;

impl Plugin for IconsPlugin {
    fn build(&self, _app: &mut App) {}

    fn finish(&self, app: &mut App) {
        let world = app.world_mut();
        let (image, skins) = match world.get_resource_mut::<Assets<Image>>() {
            Some(mut images) => {
                let atlas = images.add(atlas_image());
                let skins = crate::skins::Skin::ALL.iter().map(|s| images.add(crate::skins::image(*s))).collect();
                (atlas, skins)
            }
            None => (Handle::default(), Vec::new()),
        };
        let layout = match world.get_resource_mut::<Assets<TextureAtlasLayout>>() {
            Some(mut layouts) => layouts.add(atlas_layout()),
            None => Handle::default(),
        };
        world.insert_resource(Icons { image, layout, skins });
    }
}

pub fn icon_node(icons: &Icons, icon: Icon, px: f32, color: Color) -> (ImageNode, Node) {
    (
        ImageNode::from_atlas_image(icons.image.clone(), TextureAtlas { layout: icons.layout.clone(), index: icon.index() })
            .with_color(color),
        Node { width: Val::Px(px), height: Val::Px(px), flex_shrink: 0.0, ..default() },
    )
}

fn atlas_rows() -> u32 {
    (Icon::ALL.len() as u32).div_ceil(COLUMNS)
}

fn atlas_layout() -> TextureAtlasLayout {
    TextureAtlasLayout::from_grid(UVec2::splat(CELL), COLUMNS, atlas_rows(), None, None)
}

pub fn rasterize_atlas() -> (u32, u32, Vec<u8>) {
    let (w, h) = (COLUMNS * CELL, atlas_rows() * CELL);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for px in rgba.chunks_exact_mut(4) {
        px[..3].copy_from_slice(&[255, 255, 255]);
    }
    for &icon in Icon::ALL {
        let alpha = rasterize_icon(icon);
        let (cx, cy) = ((icon.index() as u32 % COLUMNS) * CELL, (icon.index() as u32 / COLUMNS) * CELL);
        for y in 0..CELL {
            for x in 0..CELL {
                rgba[(((cy + y) * w + cx + x) * 4 + 3) as usize] = alpha[(y * CELL + x) as usize];
            }
        }
    }
    (w, h, rgba)
}

fn atlas_image() -> Image {
    let (w, h, base) = rasterize_atlas();
    let mut data = base.clone();
    let (mut lw, mut lh, mut level) = (w, h, base);
    for _ in 1..MIPS {
        let (nw, nh) = (lw / 2, lh / 2);
        let mut next = vec![255u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                let a = |dx: u32, dy: u32| level[(((2 * y + dy) * lw + 2 * x + dx) * 4 + 3) as usize] as u32;
                next[((y * nw + x) * 4 + 3) as usize] = ((a(0, 0) + a(1, 0) + a(0, 1) + a(1, 1) + 2) / 4) as u8;
            }
        }
        data.extend_from_slice(&next);
        (lw, lh, level) = (nw, nh, next);
    }
    let mut img = Image::new(
        Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        TextureDimension::D2,
        vec![0; (w * h * 4) as usize],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.mip_level_count = MIPS;
    img.data = Some(data);
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
    img
}

fn rasterize_icon(icon: Icon) -> Vec<u8> {
    let mut c = Canvas::new(Xf::ID);
    draw(icon, &mut c);
    c.d.iter().map(|&d| ((0.5 - d).clamp(0.0, 1.0) * 255.0 + 0.5) as u8).collect()
}

struct S(Box<dyn Fn(Vec2) -> f32>);

impl S {
    fn new(f: impl Fn(Vec2) -> f32 + 'static) -> S {
        S(Box::new(f))
    }
    fn at(&self, p: Vec2) -> f32 {
        (self.0)(p)
    }
    fn or(self, o: S) -> S {
        S::new(move |p| self.at(p).min(o.at(p)))
    }
    fn minus(self, o: S) -> S {
        S::new(move |p| self.at(p).max(-o.at(p)))
    }
    fn and(self, o: S) -> S {
        S::new(move |p| self.at(p).max(o.at(p)))
    }
    fn grow(self, k: f32) -> S {
        S::new(move |p| self.at(p) - k)
    }
    fn outline(self, w: f32) -> S {
        S::new(move |p| self.at(p).abs() - w * 0.5)
    }
    fn rot(self, cx: f32, cy: f32, deg: f32) -> S {
        let c = v(cx, cy);
        let r = Vec2::from_angle(-deg.to_radians());
        S::new(move |p| self.at(c + r.rotate(p - c)))
    }
    fn above(self, y: f32) -> S {
        S::new(move |p| self.at(p).max(p.y - y))
    }
}

fn v(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

fn union(shapes: Vec<S>) -> S {
    S::new(move |p| shapes.iter().map(|s| s.at(p)).fold(f32::MAX, f32::min))
}

fn circle(cx: f32, cy: f32, r: f32) -> S {
    let c = v(cx, cy);
    S::new(move |p| (p - c).length() - r)
}

fn ring(cx: f32, cy: f32, r: f32, w: f32) -> S {
    circle(cx, cy, r).outline(w)
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> S {
    let (c, r) = (v(cx, cy), v(rx, ry));
    S::new(move |p| {
        let q = (p - c).abs();
        let k0 = (q / r).length();
        let k1 = (q / (r * r)).length();
        if k1 < 1e-6 { -rx.min(ry) } else { k0 * (k0 - 1.0) / k1 }
    })
}

fn rect(cx: f32, cy: f32, hw: f32, hh: f32, rad: f32) -> S {
    let (c, h) = (v(cx, cy), v(hw, hh));
    S::new(move |p| {
        let q = (p - c).abs() - h + Vec2::splat(rad);
        q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0) - rad
    })
}

fn rect_c(x0: f32, y0: f32, x1: f32, y1: f32, rad: f32) -> S {
    rect((x0 + x1) * 0.5, (y0 + y1) * 0.5, (x1 - x0) * 0.5, (y1 - y0) * 0.5, rad)
}

fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let (pa, ba) = (p - a, b - a);
    let t = (pa.dot(ba) / ba.dot(ba).max(1e-6)).clamp(0.0, 1.0);
    (pa - ba * t).length()
}

fn seg(x0: f32, y0: f32, x1: f32, y1: f32, w: f32) -> S {
    let (a, b) = (v(x0, y0), v(x1, y1));
    S::new(move |p| seg_dist(p, a, b) - w * 0.5)
}

fn polyline_v(pts: Vec<Vec2>, w: f32) -> S {
    S::new(move |p| pts.windows(2).map(|s| seg_dist(p, s[0], s[1])).fold(f32::MAX, f32::min) - w * 0.5)
}

fn polyline(pts: &[(f32, f32)], w: f32) -> S {
    polyline_v(pts.iter().map(|&(x, y)| v(x, y)).collect(), w)
}

fn poly_v(pts: Vec<Vec2>) -> S {
    S::new(move |p| {
        let n = pts.len();
        let mut d = (p - pts[0]).length_squared();
        let mut s = 1.0;
        let mut j = n - 1;
        for i in 0..n {
            let (vi, vj) = (pts[i], pts[j]);
            let e = vj - vi;
            let w = p - vi;
            let b = w - e * (w.dot(e) / e.dot(e).max(1e-9)).clamp(0.0, 1.0);
            d = d.min(b.dot(b));
            let c0 = p.y >= vi.y;
            let c1 = p.y < vj.y;
            let c2 = e.x * w.y > e.y * w.x;
            if (c0 && c1 && c2) || (!c0 && !c1 && !c2) {
                s = -s;
            }
            j = i;
        }
        s * d.sqrt()
    })
}

fn poly(pts: &[(f32, f32)]) -> S {
    poly_v(pts.iter().map(|&(x, y)| v(x, y)).collect())
}

fn arc(cx: f32, cy: f32, r: f32, a0: f32, a1: f32, w: f32) -> S {
    let c = v(cx, cy);
    let span = a1 - a0;
    let e0 = c + Vec2::from_angle(a0.to_radians()) * r;
    let e1 = c + Vec2::from_angle(a1.to_radians()) * r;
    S::new(move |p| {
        let q = p - c;
        let a = q.y.atan2(q.x).to_degrees();
        let t = (a - a0).rem_euclid(360.0);
        let d = if t <= span { (q.length() - r).abs() } else { (p - e0).length().min((p - e1).length()) };
        d - w * 0.5
    })
}

fn qbez_pts(a: Vec2, c: Vec2, b: Vec2, n: usize) -> Vec<Vec2> {
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            a * (1.0 - t) * (1.0 - t) + c * 2.0 * t * (1.0 - t) + b * t * t
        })
        .collect()
}

fn qbez(a: (f32, f32), c: (f32, f32), b: (f32, f32), w: f32) -> S {
    polyline_v(qbez_pts(v(a.0, a.1), v(c.0, c.1), v(b.0, b.1), 20), w)
}

fn leaf(a: (f32, f32), b: (f32, f32), width: f32) -> S {
    let (a, b) = (v(a.0, a.1), v(b.0, b.1));
    let m = (a + b) * 0.5;
    let n = (b - a).perp().normalize() * width;
    let mut pts = qbez_pts(a, m + n, b, 16);
    pts.extend(qbez_pts(b, m - n, a, 16).into_iter().skip(1));
    poly_v(pts)
}

fn wave(x0: f32, x1: f32, y: f32, amp: f32, periods: f32, w: f32) -> S {
    let pts = (0..=40)
        .map(|i| {
            let t = i as f32 / 40.0;
            v(x0 + (x1 - x0) * t, y - amp * (t * periods * TAU).sin())
        })
        .collect();
    polyline_v(pts, w)
}

fn wave_fill(x0: f32, x1: f32, ytop: f32, ybot: f32, amp: f32, periods: f32) -> S {
    let k = periods * TAU / (x1 - x0);
    S::new(move |p| {
        let t = (p.x - x0) * k;
        let top = ytop - amp * t.sin();
        let slope = amp * k * t.cos();
        let dtop = (top - p.y) / (1.0 + slope * slope).sqrt();
        let dx = (x0 - p.x).max(p.x - x1);
        dtop.max(p.y - ybot).max(dx)
    })
}

fn radial(n: usize, cx: f32, cy: f32, f: impl Fn() -> S) -> S {
    union((0..n).map(|i| f().rot(cx, cy, i as f32 * 360.0 / n as f32)).collect())
}

#[derive(Clone, Copy)]
struct Xf {
    o: Vec2,
    k: f32,
    r: Vec2,
}

impl Xf {
    const ID: Xf = Xf { o: Vec2::ZERO, k: 1.0, r: Vec2::X };
    fn to_local(&self, p: Vec2) -> Vec2 {
        v(self.r.x, -self.r.y).rotate(p - self.o) / self.k
    }
    fn then(&self, child: Xf) -> Xf {
        Xf { o: self.o + self.r.rotate(child.o) * self.k, k: self.k * child.k, r: self.r.rotate(child.r) }
    }
}

struct Canvas {
    d: Vec<f32>,
    xf: Xf,
}

const W: f32 = 5.5;
const GAP: f32 = 3.0;

impl Canvas {
    fn new(xf: Xf) -> Self {
        Canvas { d: vec![1e3; (CELL * CELL) as usize], xf }
    }
    fn each(&mut self, s: &S, op: impl Fn(f32, f32) -> f32) {
        for y in 0..CELL {
            for x in 0..CELL {
                let p = v(x as f32 + 0.5, y as f32 + 0.5);
                let i = (y * CELL + x) as usize;
                let d = s.at(self.xf.to_local(p)) * self.xf.k;
                self.d[i] = op(self.d[i], d);
            }
        }
    }
    fn add(&mut self, s: S) {
        self.each(&s, f32::min);
    }
    fn cut(&mut self, s: S) {
        self.each(&s, |a, b| a.max(-b));
    }
    fn knock(&mut self, s: S) {
        let g = GAP * self.xf.k;
        self.each(&s, move |a, b| a.max(g - b).min(b));
    }
    fn part(&mut self, cx: f32, cy: f32, k: f32, deg: f32, gap: f32, f: impl FnOnce(&mut Canvas)) {
        let r = Vec2::from_angle(deg.to_radians());
        let child = Xf { o: v(cx, cy) - r.rotate(v(32.0, 32.0)) * k, k, r };
        let mut sub = Canvas::new(self.xf.then(child));
        f(&mut sub);
        let g = gap * self.xf.k;
        for (a, &b) in self.d.iter_mut().zip(&sub.d) {
            *a = if gap > 0.0 { a.max(g - b).min(b) } else { a.min(b) };
        }
    }
    fn cut_part(&mut self, cx: f32, cy: f32, k: f32, deg: f32, grow: f32, f: impl FnOnce(&mut Canvas)) {
        let r = Vec2::from_angle(deg.to_radians());
        let child = Xf { o: v(cx, cy) - r.rotate(v(32.0, 32.0)) * k, k, r };
        let mut sub = Canvas::new(self.xf.then(child));
        f(&mut sub);
        let g = grow * self.xf.k;
        for (a, &b) in self.d.iter_mut().zip(&sub.d) {
            *a = a.max(g - b);
        }
    }
}

fn head(c: &mut Canvas) {
    c.add(circle(14.5, 16.0, 7.5).or(circle(49.5, 16.0, 7.5)));
    c.add(ellipse(32.0, 32.0, 22.5, 19.5));
    c.cut(circle(22.5, 27.5, 3.8).or(circle(41.5, 27.5, 3.8)));
    c.cut(ellipse(32.0, 37.5, 7.0, 4.5));
    c.knock(rect_c(26.0, 45.0, 38.0, 58.0, 2.0));
    c.cut(seg(32.0, 48.0, 32.0, 60.0, 2.2));
}

/// A beaver-chewed tree: a solid trunk with a deep wedge gnawed out of one side, bark roots
/// below, crown branches above, and flying wood chips.
fn gnawed_stump(c: &mut Canvas) {
    // Trunk with flared roots and a deep side chew-notch on the right.
    c.add(
        poly(&[
            (18.0, 18.0),
            (38.0, 18.0),
            (38.0, 29.0),
            (27.0, 36.0),
            (39.0, 44.0),
            (46.0, 56.0),
            (10.0, 56.0),
            (18.0, 44.0),
        ])
        .grow(1.2),
    );
    // Small leafy branch sprouting off the top so it never reads as an hourglass.
    c.add(circle(28.0, 15.0, 11.0).or(circle(17.0, 20.0, 7.5)).or(circle(39.0, 19.0, 7.5)));
    // Flying wood chips from the chew-notch.
    c.add(poly(&[(41.0, 34.0), (54.0, 29.0), (51.0, 39.0)]).grow(0.8));
    c.add(poly(&[(45.0, 43.0), (57.0, 42.0), (52.0, 50.0)]).grow(0.6));
}

/// Woven stick-and-stake beaver craft: two upright stakes bound by horizontal logs.
fn wattle_work(c: &mut Canvas) {
    c.add(seg(21.0, 9.0, 21.0, 55.0, 7.0));
    c.add(seg(43.0, 9.0, 43.0, 55.0, 7.0));
    c.knock(seg(8.0, 22.0, 56.0, 22.0, 8.5));
    c.knock(seg(8.0, 42.0, 56.0, 42.0, 8.5));
    c.cut(circle(10.0, 22.0, 2.0).or(circle(54.0, 42.0, 2.0)));
}

/// A beaver repair patch: two bold crossed sticks binding a woven repair.
fn mend_patch(c: &mut Canvas) {
    c.add(rect(32.0, 32.0, 19.0, 19.0, 7.0));
    c.cut(seg(19.0, 32.0, 45.0, 32.0, 7.0).or(seg(32.0, 19.0, 32.0, 45.0, 7.0)));
}

/// A lashed bundle of sticks/logs carried by a hauler.
fn stick_bundle(c: &mut Canvas) {
    c.add(seg(9.0, 21.0, 55.0, 21.0, 7.5));
    c.add(seg(7.0, 33.0, 57.0, 33.0, 7.5));
    c.add(seg(11.0, 45.0, 53.0, 45.0, 7.5));
    // Vertical lashing band holding the bundle together.
    c.knock(rect_c(26.0, 12.0, 38.0, 54.0, 3.0));
}

/// Smooth river pebbles.
fn pebbles_shape(c: &mut Canvas) {
    c.add(poly(&[(5.0, 57.0), (9.0, 40.0), (21.0, 31.0), (33.0, 36.0), (37.0, 57.0)]).grow(2.0));
    c.knock(poly(&[(27.0, 57.0), (31.0, 29.0), (44.0, 17.0), (57.0, 28.0), (59.0, 57.0)]).grow(2.0));
    c.cut(seg(44.0, 25.0, 50.0, 40.0, 2.6).or(seg(14.0, 44.0, 22.0, 39.0, 2.4)));
}

fn berries(c: &mut Canvas) {
    c.add(circle(21.0, 44.0, 12.0));
    c.knock(circle(43.0, 41.0, 12.0));
    c.cut(circle(16.5, 40.0, 3.0).or(circle(38.5, 37.0, 3.0)));
    c.add(qbez((21.0, 33.0), (24.0, 18.0), (35.0, 10.0), 4.5));
    c.add(qbez((43.0, 30.0), (41.0, 18.0), (35.0, 10.0), 4.5));
    c.add(leaf((35.0, 11.0), (57.0, 7.0), 11.0));
}

/// A solid beaver lodge dome (for generic Building / Home / Flood).
fn lodge_silhouette(c: &mut Canvas) {
    c.add(seg(21.0, 7.0, 39.0, 28.0, 5.5));
    c.add(seg(43.0, 7.0, 25.0, 28.0, 5.5));
    c.knock(ellipse(32.0, 56.0, 27.0, 33.0).above(56.0));
}

/// A watchful almond eye.
fn eye(c: &mut Canvas) {
    c.add(circle(32.0, 52.0, 32.0).and(circle(32.0, 12.0, 32.0)).grow(1.0));
    c.cut(circle(32.0, 32.0, 11.5));
    c.add(circle(32.0, 32.0, 6.2));
}

fn pine(c: &mut Canvas) {
    c.add(
        poly(&[
            (32.0, 6.0),
            (46.0, 21.0),
            (40.0, 21.0),
            (52.0, 35.0),
            (45.0, 35.0),
            (56.0, 48.0),
            (8.0, 48.0),
            (19.0, 35.0),
            (12.0, 35.0),
            (24.0, 21.0),
            (18.0, 21.0),
        ])
        .grow(1.2),
    );
    c.add(rect_c(27.0, 44.0, 37.0, 58.0, 1.5));
}

fn round_tree(c: &mut Canvas) {
    c.add(seg(32.0, 34.0, 32.0, 58.0, 8.0));
    c.knock(circle(32.0, 22.0, 15.0).or(circle(19.0, 31.0, 11.5)).or(circle(45.0, 31.0, 11.5)).or(rect_c(19.0, 28.0, 45.0, 42.5, 8.0)));
}

fn mud_pile(c: &mut Canvas) {
    c.add(union(vec![circle(20.0, 44.0, 12.0), circle(44.0, 44.0, 12.0), circle(32.0, 32.0, 14.0), rect_c(8.0, 42.0, 56.0, 56.0, 5.0)]));
    c.cut(circle(25.0, 37.0, 3.2).or(circle(40.0, 46.0, 3.0)).or(circle(18.0, 49.0, 2.3)));
}

fn lodge_dome(c: &mut Canvas) {
    c.add(seg(21.0, 7.0, 39.0, 30.0, 5.0));
    c.add(seg(43.0, 7.0, 25.0, 30.0, 5.0));
    c.knock(ellipse(32.0, 55.0, 28.0, 31.0).above(55.0));
    c.cut(seg(6.0, 45.0, 30.0, 40.0, 3.0));
    c.cut(seg(34.0, 40.0, 58.0, 45.0, 3.0));
    c.cut(seg(14.0, 33.0, 31.0, 30.0, 3.0));
    c.cut(seg(33.0, 30.0, 50.0, 33.0, 3.0));
}

fn drop_shape() -> S {
    circle(32.0, 40.0, 16.0).or(poly(&[(32.0, 5.0), (46.1, 32.5), (17.9, 32.5)]).grow(0.8))
}

fn heart() -> S {
    circle(21.5, 24.0, 12.0)
        .or(circle(42.5, 24.0, 12.0))
        .or(poly(&[(10.5, 28.5), (32.0, 52.0), (53.5, 28.5), (32.0, 24.0)]).grow(1.5))
}

/// A puffy cloud, flat along its bottom.
fn cloud() -> S {
    union(vec![circle(21.0, 36.0, 11.0), circle(35.0, 28.0, 15.0), circle(48.0, 37.0, 10.0), rect_c(10.0, 36.0, 58.0, 47.0, 5.5)])
}

fn sun(c: &mut Canvas, r: f32, ray0: f32, ray1: f32) {
    c.add(circle(32.0, 32.0, r));
    c.add(radial(8, 32.0, 32.0, || seg(32.0, 32.0 - ray0, 32.0, 32.0 - ray1, 5.5)));
}

fn face(c: &mut Canvas) {
    // Beaver-eared mood face.
    c.add(circle(14.5, 16.0, 7.5).or(circle(49.5, 16.0, 7.5)));
    c.add(circle(32.0, 34.0, 23.0));
    c.cut(circle(23.5, 28.0, 3.8).or(circle(40.5, 28.0, 3.8)));
}

fn paw(c: &mut Canvas) {
    c.add(ellipse(32.0, 41.0, 13.0, 11.0));
    c.add(ellipse(14.0, 27.0, 5.0, 6.5).rot(14.0, 27.0, -25.0));
    c.add(ellipse(25.0, 17.0, 5.0, 6.5).rot(25.0, 17.0, -8.0));
    c.add(ellipse(39.0, 17.0, 5.0, 6.5).rot(39.0, 17.0, 8.0));
    c.add(ellipse(50.0, 27.0, 5.0, 6.5).rot(50.0, 27.0, 25.0));
}

fn slash(c: &mut Canvas) {
    c.knock(seg(10.0, 10.0, 54.0, 54.0, W));
}

fn sapling(c: &mut Canvas) {
    c.add(seg(32.0, 57.0, 32.0, 24.0, 5.5));
    c.add(leaf((31.0, 38.0), (8.0, 20.0), 14.0));
    c.add(leaf((33.0, 27.0), (56.0, 8.0), 15.0));
    c.add(seg(14.0, 58.0, 50.0, 58.0, 5.0));
}

fn tri_rounded(pts: &[(f32, f32)]) -> S {
    poly(pts).grow(2.5)
}

fn role(c: &mut Canvas, deg: f32, tool: impl FnOnce(&mut Canvas)) {
    c.part(23.0, 24.0, 0.66, 0.0, 0.0, head);
    c.part(41.0, 41.0, 0.66, deg, 3.5, tool);
}

fn draw(icon: Icon, c: &mut Canvas) {
    use Icon::*;
    match icon {
        Food => berries(c),
        Logs => {
            for (x, y) in [(19.0, 45.0), (45.0, 45.0), (32.0, 22.0)] {
                c.add(circle(x, y, 12.5));
                c.cut(ring(x, y, 7.0, 3.0));
            }
        }
        Sticks => {
            c.add(seg(10.0, 44.0, 42.0, 12.0, 4.8));
            c.add(seg(14.0, 52.0, 52.0, 14.0, 4.8));
            c.add(seg(22.0, 54.0, 54.0, 22.0, 4.8));
            c.add(seg(44.0, 20.0, 50.0, 6.0, 4.0));
            c.knock(seg(24.0, 26.0, 40.0, 42.0, 6.5));
        }
        Mud => mud_pile(c),
        Dam => {
            for y in [17.0, 29.0, 41.0, 53.0] {
                c.add(wave(5.0, 23.0, y, 2.8, 1.0, 5.0));
            }
            for y in [17.0, 34.0, 51.0] {
                c.add(circle(35.0, y, 8.8));
                c.cut(ring(35.0, y, 4.3, 2.6));
            }
            c.add(wave(47.0, 60.0, 53.0, 2.8, 1.0, 5.0));
        }
        Lodge => {
            c.part(32.0, 26.0, 0.8, 0.0, 0.0, lodge_dome);
            c.cut(rect_c(25.0, 34.0, 39.0, 52.0, 7.0));
            c.cut(seg(0.0, 49.0, 64.0, 49.0, 4.0));
            c.add(wave(6.0, 58.0, 55.0, 3.0, 2.0, 5.0));
        }
        Canal => {
            c.add(seg(10.0, 10.0, 10.0, 34.0, W).or(seg(54.0, 10.0, 54.0, 34.0, W)));
            c.add(arc(32.0, 34.0, 22.0, 0.0, 180.0, W));
            c.add(wave(19.0, 45.0, 28.0, 3.0, 1.0, 4.5));
            c.add(wave(19.0, 45.0, 40.0, 3.0, 1.0, 4.5));
        }
        Lock => {
            c.add(rect_c(28.0, 12.0, 36.0, 58.0, 2.0));
            c.add(seg(22.0, 8.0, 42.0, 8.0, 5.0));
            c.add(wave(5.0, 24.0, 22.0, 3.0, 1.0, 5.0));
            c.add(wave(5.0, 24.0, 33.0, 3.0, 1.0, 5.0));
            c.add(wave(5.0, 24.0, 44.0, 3.0, 1.0, 5.0));
            c.add(wave(40.0, 59.0, 44.0, 3.0, 1.0, 5.0));
            c.add(seg(4.0, 56.0, 60.0, 56.0, 4.5));
        }
        StickPile => {
            // A stick-and-mud cache island heaped with crossed logs over shallow water.
            c.add(seg(10.0, 33.0, 38.0, 11.0, 6.0));
            c.add(seg(54.0, 33.0, 26.0, 11.0, 6.0));
            c.add(seg(18.0, 29.0, 48.0, 17.0, 5.5));
            c.knock(ellipse(32.0, 47.0, 25.0, 14.0).above(47.0));
            c.cut(circle(22.0, 39.0, 3.2).or(circle(32.0, 37.0, 3.2)).or(circle(42.0, 39.0, 3.2)));
            c.add(wave(6.0, 58.0, 55.0, 2.8, 2.0, 4.8));
        }
        FellingCamp => gnawed_stump(c),

        RoleForager => role(c, 0.0, berries),
        RoleFeller => role(c, 0.0, gnawed_stump),
        RoleMudDigger => role(c, 0.0, mud_pile),
        RoleBuilder => role(c, 0.0, wattle_work),
        RoleMender => role(c, 0.0, mend_patch),
        HaulMound => {
            // A stick-and-mud staging mound with opposing haul arrows above it.
            c.add(seg(14.0, 16.0, 46.0, 16.0, 5.0));
            c.add(tri_rounded(&[(55.0, 16.0), (43.0, 8.0), (43.0, 24.0)]));
            c.add(seg(18.0, 29.0, 50.0, 29.0, 5.0));
            c.add(tri_rounded(&[(9.0, 29.0), (21.0, 21.0), (21.0, 37.0)]));
            c.add(ellipse(32.0, 50.0, 24.0, 12.0).above(50.0));
            c.add(wave(6.0, 58.0, 56.0, 2.5, 2.0, 4.5));
        }
        RoleHauler => role(c, 0.0, stick_bundle),
        RolePebbler => role(c, 0.0, pebbles_shape),
        RoleSentry => role(c, 0.0, eye),
        RaftLanding => {
            // A timber landing on piles over waves with a lashed log raft moored beside it.
            c.add(rect_c(5.0, 24.0, 31.0, 31.0, 2.0));
            c.add(seg(11.0, 31.0, 11.0, 47.0, 4.5).or(seg(25.0, 31.0, 25.0, 47.0, 4.5)));
            // Mooring post on the landing edge.
            c.add(seg(29.0, 14.0, 29.0, 24.0, 4.5));
            // Lashed log raft floating on the right with a small load.
            for x in [38.0, 47.0, 56.0] {
                c.add(circle(x, 42.0, 4.8));
            }
            c.add(seg(36.0, 35.0, 58.0, 35.0, 5.0));
            c.add(wave(4.0, 60.0, 55.0, 2.5, 2.0, 4.5));
        }
        LockGates => {
            // Two pairs of mitre gates between walls, water between.
            c.add(seg(10.0, 6.0, 10.0, 58.0, 5.0).or(seg(54.0, 6.0, 54.0, 58.0, 5.0)));
            c.add(seg(12.0, 14.0, 32.0, 22.0, 4.0).or(seg(52.0, 14.0, 32.0, 22.0, 4.0)));
            c.add(seg(12.0, 42.0, 32.0, 50.0, 4.0).or(seg(52.0, 42.0, 32.0, 50.0, 4.0)));
            c.add(wave(14.0, 50.0, 33.0, 2.5, 1.0, 4.0));
        }
        Raft => {
            // Lashed log raft on waves carrying a tied load of sticks.
            for x in [12.0, 25.0, 39.0, 52.0] {
                c.add(circle(x, 42.0, 6.2));
                c.cut(circle(x, 42.0, 2.2));
            }
            c.add(seg(12.0, 31.0, 52.0, 31.0, 6.5));
            c.add(seg(18.0, 22.0, 46.0, 22.0, 6.0));
            c.add(wave(4.0, 60.0, 56.0, 2.5, 2.0, 4.5));
        }
        Lookout => {
            // A tall beaver watch-mound over water, crowned with a watchful almond eye.
            c.part(32.0, 17.0, 0.80, 0.0, 0.0, eye);
            c.add(ellipse(32.0, 52.0, 25.0, 18.0).above(52.0));
            c.cut(seg(13.0, 44.0, 31.0, 41.0, 2.8).or(seg(33.0, 41.0, 51.0, 44.0, 2.8)));
            c.add(wave(6.0, 58.0, 56.0, 2.5, 2.0, 4.5));
        }
        ViewWorld => {
            // Normal valley view: rolling hills, winding river and a sun.
            c.add(circle(46.0, 16.0, 8.0));
            c.add(poly(&[(4.0, 56.0), (4.0, 36.0), (22.0, 20.0), (40.0, 38.0), (50.0, 30.0), (60.0, 40.0), (60.0, 56.0)]).grow(1.5));
            c.cut(qbez((32.0, 35.0), (20.0, 45.0), (42.0, 58.0), 6.0));
        }
        ViewWater => {
            c.add(drop_shape().grow(1.5));
            c.cut(wave(12.0, 52.0, 40.0, 3.0, 1.0, 4.0));
        }
        ViewForest => {
            c.part(22.0, 36.0, 0.7, 0.0, 0.0, pine);
            c.part(42.0, 31.0, 0.8, 0.0, 3.0, pine);
        }
        ViewElevation => {
            c.add(ellipse(33.0, 37.0, 26.0, 19.0).outline(W - 1.0));
            c.add(ellipse(30.0, 32.0, 16.0, 11.5).outline(W - 1.0));
            c.add(circle(28.0, 29.0, 5.5));
        }
        ViewDeposits => {
            // Natural resources: a sprouting plant above river pebbles and a mud mound.
            c.part(32.0, 19.0, 0.56, 0.0, 0.0, sapling);
            c.part(18.0, 44.0, 0.48, 0.0, 0.0, pebbles_shape);
            c.part(45.0, 45.0, 0.48, 0.0, 2.5, mud_pile);
        }
        Idle => {
            c.add(circle(30.0, 33.0, 23.0).minus(circle(43.0, 24.0, 19.0)));
        }
        Unstaffed => {
            c.part(32.0, 32.0, 0.88, 0.0, 0.0, head);
            slash(c);
        }
        Blocked => {
            c.add(circle(32.0, 32.0, 26.0));
            c.cut(rect_c(15.0, 28.0, 49.0, 36.0, 2.0));
        }
        Full => {
            c.add(rect_c(13.0, 6.0, 51.0, 58.0, 6.0).outline(W));
            for y in [13.5, 27.0, 40.5] {
                c.add(rect_c(21.0, y, 43.0, y + 10.0, 2.0));
            }
        }
        Warn => {
            c.add(tri_rounded(&[(32.0, 8.0), (57.0, 53.0), (7.0, 53.0)]));
            c.cut(seg(32.0, 23.0, 32.0, 38.0, 5.5));
            c.cut(circle(32.0, 46.5, 3.2));
        }
        Urgent => {
            c.add(poly(&[(32.0, 7.0), (57.0, 32.0), (32.0, 57.0), (7.0, 32.0)]).grow(2.5));
            c.cut(seg(32.0, 19.0, 32.0, 36.0, 6.0));
            c.cut(circle(32.0, 45.5, 3.5));
        }
        Up => c.add(tri_rounded(&[(32.0, 12.0), (53.0, 49.0), (11.0, 49.0)])),
        Down => c.add(tri_rounded(&[(32.0, 52.0), (11.0, 15.0), (53.0, 15.0)])),
        Check => c.add(polyline(&[(11.0, 33.0), (25.0, 47.0), (53.0, 17.0)], 8.0)),
        Cross => c.add(seg(14.0, 14.0, 50.0, 50.0, 8.0).or(seg(50.0, 14.0, 14.0, 50.0, 8.0))),
        Pause => c.add(rect_c(14.0, 10.0, 27.0, 54.0, 3.0).or(rect_c(37.0, 10.0, 50.0, 54.0, 3.0))),
        Play => c.add(tri_rounded(&[(19.0, 12.0), (52.0, 32.0), (19.0, 52.0)])),
        Fast2 => {
            c.add(tri_rounded(&[(7.0, 15.0), (29.0, 32.0), (7.0, 49.0)]));
            c.knock(tri_rounded(&[(32.0, 15.0), (54.0, 32.0), (32.0, 49.0)]));
        }
        Fast3 => {
            c.add(tri_rounded(&[(5.0, 18.0), (21.0, 32.0), (5.0, 46.0)]));
            c.knock(tri_rounded(&[(23.0, 18.0), (39.0, 32.0), (23.0, 46.0)]));
            c.knock(tri_rounded(&[(41.0, 18.0), (57.0, 32.0), (41.0, 46.0)]));
        }
        Undo => {
            c.add(arc(35.0, 36.0, 18.0, 180.0, 400.0, W + 0.5));
            c.add(tri_rounded(&[(7.0, 31.0), (27.0, 31.0), (17.0, 44.0)]));
        }
        Pin => c.part(32.0, 32.0, 1.0, 35.0, 0.0, |c| {
            c.add(rect_c(20.0, 6.0, 44.0, 14.0, 3.0));
            c.add(poly(&[(25.0, 13.0), (39.0, 13.0), (42.0, 34.0), (22.0, 34.0)]));
            c.add(rect_c(14.0, 33.0, 50.0, 40.0, 3.0));
            c.add(seg(32.0, 40.0, 32.0, 59.0, 4.0));
        }),
        Plus => c.add(seg(32.0, 11.0, 32.0, 53.0, 7.5).or(seg(11.0, 32.0, 53.0, 32.0, 7.5))),
        Minus => c.add(seg(11.0, 32.0, 53.0, 32.0, 7.5)),
        Focus => {
            c.add(ring(32.0, 32.0, 17.0, W));
            c.add(seg(32.0, 5.0, 32.0, 18.0, W).or(seg(32.0, 46.0, 32.0, 59.0, W)));
            c.add(seg(5.0, 32.0, 18.0, 32.0, W).or(seg(46.0, 32.0, 59.0, 32.0, W)));
            c.add(circle(32.0, 32.0, 4.5));
        }
        Expand => c.add(tri_rounded(&[(24.0, 17.0), (45.0, 32.0), (24.0, 47.0)])),
        Colony => {
            // A beaver settlement: two overlapping timber-and-mud lodges over water.
            c.add(seg(37.0, 18.0, 48.0, 31.0, 4.2));
            c.add(seg(51.0, 18.0, 40.0, 31.0, 4.2));
            c.add(ellipse(44.0, 46.0, 16.0, 19.0).above(46.0));
            c.cut(rect_c(40.0, 36.0, 48.0, 48.0, 4.0));
            c.knock(seg(16.0, 12.0, 29.0, 27.0, 4.5).or(seg(32.0, 12.0, 19.0, 27.0, 4.5)).or(ellipse(24.0, 47.0, 20.0, 24.0).above(47.0)));
            c.cut(rect_c(18.5, 34.0, 29.5, 49.0, 5.5));
            c.cut(seg(0.0, 49.0, 64.0, 49.0, 3.8));
            c.add(wave(6.0, 58.0, 55.0, 2.8, 2.0, 4.8));
        }
        Jobs => {
            // A working beaver holding a stout stick in its teeth.
            c.part(32.0, 25.0, 0.78, 0.0, 0.0, head);
            c.knock(seg(8.0, 48.0, 56.0, 44.0, 7.5));
        }
        Follow => {
            c.add(circle(32.0, 52.0, 32.0).and(circle(32.0, 12.0, 32.0)).grow(1.0));
            c.cut(circle(32.0, 32.0, 12.5));
            c.add(circle(32.0, 32.0, 8.0));
        }
        Area => {
            for (x, y) in [(11.0, 13.0), (53.0, 13.0), (11.0, 51.0), (53.0, 51.0)] {
                c.add(rect(x, y, 5.5, 5.5, 1.5));
            }
            c.add(seg(23.0, 13.0, 41.0, 13.0, 4.5).or(seg(23.0, 51.0, 41.0, 51.0, 4.5)));
            c.add(seg(11.0, 25.0, 11.0, 39.0, 4.5).or(seg(53.0, 25.0, 53.0, 39.0, 4.5)));
        }
        Haul => {
            // A lashed stick bundle moving forward.
            c.part(24.0, 38.0, 0.72, 0.0, 0.0, stick_bundle);
            c.add(seg(22.0, 15.0, 49.0, 15.0, 5.5));
            c.add(tri_rounded(&[(58.0, 15.0), (45.0, 6.0), (45.0, 24.0)]));
        }
        Back => {
            c.add(seg(13.0, 32.0, 53.0, 32.0, 7.0));
            c.add(polyline(&[(30.0, 14.0), (12.0, 32.0), (30.0, 50.0)], 7.0));
        }
        Beaver => head(c),
        Adult => c.part(32.0, 33.0, 0.98, 0.0, 0.0, head),
        Yearling => c.part(32.0, 37.0, 0.8, 0.0, 0.0, head),
        Kit => c.part(32.0, 41.0, 0.62, 0.0, 0.0, head),
        MoodGood => {
            face(c);
            c.cut(arc(32.0, 34.0, 12.0, 30.0, 150.0, 4.8));
        }
        MoodOk => {
            face(c);
            c.cut(seg(21.0, 43.0, 43.0, 43.0, 4.8));
        }
        MoodBad => {
            face(c);
            c.cut(arc(32.0, 54.0, 12.0, 220.0, 320.0, 4.8));
        }
        Health => c.add(heart()),
        Spring => {
            c.add(qbez((32.0, 56.0), (31.0, 42.0), (33.0, 30.0), 5.0));
            c.add(leaf((32.0, 34.0), (9.0, 22.0), 13.0));
            c.add(leaf((33.0, 30.0), (55.0, 12.0), 15.0));
            c.add(ellipse(32.0, 59.0, 20.0, 6.0).above(58.0));
        }
        Summer => sun(c, 13.0, 19.5, 27.0),
        Autumn => {
            c.part(32.0, 31.0, 1.0, 35.0, 0.0, |c| {
                c.add(leaf((32.0, 50.0), (32.0, 5.0), 40.0));
                c.add(seg(32.0, 48.0, 32.0, 61.0, 5.0));
                c.cut(seg(32.0, 18.0, 32.0, 50.0, 3.2));
                c.cut(seg(32.0, 36.0, 21.0, 27.0, 3.0).or(seg(32.0, 36.0, 43.0, 27.0, 3.0)));
            });
        }
        Winter => {
            c.add(radial(3, 32.0, 32.0, || seg(32.0, 6.0, 32.0, 58.0, 5.0)));
            c.add(radial(6, 32.0, 32.0, || polyline(&[(24.0, 11.0), (32.0, 19.0), (40.0, 11.0)], 4.5)));
        }
        DrySummer => {
            c.part(32.0, 23.0, 0.7, 0.0, 0.0, |c| sun(c, 13.0, 19.5, 27.0));
            c.add(rect_c(5.0, 46.0, 59.0, 58.0, 3.0));
            c.cut(polyline(&[(20.0, 44.0), (24.0, 51.0), (19.0, 60.0)], 3.0));
            c.cut(polyline(&[(42.0, 44.0), (38.0, 51.0), (44.0, 60.0)], 3.0));
        }
        Hourglass => {
            c.add(rect_c(12.0, 5.0, 52.0, 12.0, 2.5).or(rect_c(12.0, 52.0, 52.0, 59.0, 2.5)));
            c.add(poly(&[(17.0, 14.0), (47.0, 14.0), (34.5, 32.0), (47.0, 50.0), (17.0, 50.0), (29.5, 32.0)]).grow(1.0));
        }
        Walk => {
            c.part(20.0, 41.0, 0.62, -15.0, 0.0, paw);
            c.part(44.0, 22.0, 0.62, 15.0, 3.0, paw);
        }
        Tree => round_tree(c),
        Pebbles => pebbles_shape(c),
        Sapling => sapling(c),
        FoodCache => {
            // A waterside feed pile: berries and leafy twigs cached in shallow water.
            c.part(32.0, 25.0, 0.72, 0.0, 0.0, berries);
            c.knock(ellipse(32.0, 49.0, 24.0, 9.0).above(49.0));
            c.add(wave(6.0, 58.0, 56.0, 2.8, 2.0, 4.8));
        }
        RolePlanter => role(c, 0.0, sapling),
        Wolf => {
            c.add(tri_rounded(&[(12.0, 3.0), (28.0, 17.0), (11.0, 27.0)]).or(tri_rounded(&[(52.0, 3.0), (36.0, 17.0), (53.0, 27.0)])));
            c.add(poly(&[(9.0, 22.0), (22.0, 14.0), (42.0, 14.0), (55.0, 22.0), (51.0, 38.0), (38.0, 58.0), (26.0, 58.0), (13.0, 38.0)]).grow(1.0));
            c.cut(seg(19.0, 30.0, 27.0, 34.0, 4.5).or(seg(45.0, 30.0, 37.0, 34.0, 4.5)));
            c.cut(ellipse(32.0, 51.0, 5.5, 3.8));
        }
        Deer => {
            // A stag's head: antlers over two ears and a long face.
            c.add(polyline(&[(24.0, 20.0), (16.0, 10.0), (12.0, 2.0)], 3.5).or(polyline(&[(16.0, 10.0), (6.0, 8.0)], 3.0)));
            c.add(polyline(&[(40.0, 20.0), (48.0, 10.0), (52.0, 2.0)], 3.5).or(polyline(&[(48.0, 10.0), (58.0, 8.0)], 3.0)));
            c.add(ellipse(14.0, 26.0, 9.0, 4.5).or(ellipse(50.0, 26.0, 9.0, 4.5)));
            c.add(poly(&[(22.0, 20.0), (42.0, 20.0), (40.0, 46.0), (36.0, 60.0), (28.0, 60.0), (24.0, 46.0)]).grow(1.5));
            c.cut(circle(26.0, 32.0, 2.8).or(circle(38.0, 32.0, 2.8)));
        }
        Rabbit => {
            // Two long ears over a round head.
            c.add(ellipse(23.0, 18.0, 6.0, 16.0).or(ellipse(41.0, 18.0, 6.0, 16.0)));
            c.add(ellipse(32.0, 44.0, 18.0, 15.0));
            c.cut(circle(25.0, 41.0, 2.8).or(circle(39.0, 41.0, 2.8)));
            c.cut(ellipse(32.0, 51.0, 3.5, 2.5));
        }
        Bird => {
            // Side on: body, head, beak, tail and a cut-out wing.
            c.add(ellipse(30.0, 38.0, 19.0, 13.0));
            c.add(circle(46.0, 24.0, 9.0));
            c.add(poly(&[(53.0, 20.0), (62.0, 25.0), (53.0, 28.0)]));
            c.add(poly(&[(14.0, 34.0), (2.0, 28.0), (6.0, 44.0)]));
            c.cut(qbez((16.0, 34.0), (30.0, 50.0), (42.0, 36.0), 3.0));
            c.cut(circle(47.0, 22.0, 2.2));
        }
        Forester => {
            // Willow shallows: sprouting saplings standing inside a shallow-water ring.
            c.add(ellipse(32.0, 48.0, 25.0, 10.0).outline(4.8));
            c.part(24.0, 26.0, 0.62, -6.0, 2.5, sapling);
            c.part(42.0, 29.0, 0.54, 8.0, 2.5, sapling);
        }
        PebbleHeap => {
            // A stacked heap of smooth river pebbles on a shallow-water mound.
            c.add(ellipse(20.0, 41.0, 12.0, 9.0));
            c.knock(ellipse(44.0, 41.0, 12.0, 9.0));
            c.knock(ellipse(32.0, 25.0, 12.0, 9.5));
            c.add(wave(6.0, 58.0, 56.0, 2.6, 2.0, 4.6));
        }
        WaterPlant => {
            // A cleft lily pad with a flower standing on it.
            c.add(ellipse(31.0, 45.0, 28.0, 14.0));
            c.cut(poly(&[(31.0, 45.0), (60.0, 36.0), (60.0, 52.0)]));
            c.knock(poly(&[(14.0, 30.0), (20.0, 12.0), (27.0, 24.0), (32.0, 6.0), (37.0, 24.0), (44.0, 12.0), (50.0, 30.0), (32.0, 38.0)]).grow(1.0));
        }
        Plant => {
            c.add(leaf((32.0, 57.0), (32.0, 6.0), 17.0));
            c.knock(leaf((30.0, 57.0), (5.0, 26.0), 13.0));
            c.knock(leaf((34.0, 57.0), (59.0, 26.0), 13.0));
        }
        MudBank => {
            let mut pts = vec![v(60.0, 58.0), v(18.0, 58.0)];
            pts.extend(qbez_pts(v(18.0, 58.0), v(30.0, 18.0), v(60.0, 16.0), 16));
            c.add(poly_v(pts));
            c.cut(circle(46.0, 34.0, 3.0).or(circle(52.0, 46.0, 2.5)).or(circle(38.0, 48.0, 2.5)));
            c.knock(wave(4.0, 30.0, 42.0, 2.5, 1.0, 4.5));
            c.knock(wave(4.0, 34.0, 53.0, 2.5, 1.0, 4.5));
        }
        Hammer => wattle_work(c),
        Crest => {
            c.add(seg(10.0, 8.0, 54.0, 8.0, W));
            c.add(seg(32.0, 22.0, 32.0, 48.0, W));
            c.add(tri_rounded(&[(32.0, 16.0), (43.0, 28.0), (21.0, 28.0)]));
            c.add(tri_rounded(&[(32.0, 56.0), (21.0, 44.0), (43.0, 44.0)]));
        }
        Rise => {
            c.add(seg(32.0, 22.0, 32.0, 42.0, 6.5));
            c.add(tri_rounded(&[(32.0, 7.0), (46.0, 24.0), (18.0, 24.0)]));
            c.add(wave(6.0, 58.0, 54.0, 3.5, 1.5, W));
        }
        Flood => {
            c.part(32.0, 22.0, 0.82, 0.0, 0.0, lodge_silhouette);
            c.cut(rect_c(0.0, 37.0, 64.0, 64.0, 0.0));
            c.add(wave(4.0, 60.0, 42.0, 3.5, 2.0, W));
            c.add(wave(4.0, 60.0, 54.0, 3.5, 2.0, W));
        }
        Push => {
            c.add(rect_c(48.0, 6.0, 57.0, 58.0, 2.0));
            c.add(wave(6.0, 28.0, 32.0, 4.0, 1.0, 6.0));
            c.add(tri_rounded(&[(27.0, 20.0), (40.0, 32.0), (27.0, 44.0)]));
        }
        Drop => c.add(drop_shape().grow(1.5)),
        Cloud => c.add(cloud()),
        Rain => {
            c.part(32.0, 22.0, 0.85, 0.0, 0.0, |c| c.add(cloud()));
            for x in [20.0, 32.0, 44.0] {
                c.add(seg(x, 44.0, x - 4.0, 58.0, 4.5));
            }
        }
        Storm => {
            c.part(32.0, 20.0, 0.85, 0.0, 0.0, |c| c.add(cloud()));
            c.knock(poly(&[(35.0, 36.0), (24.0, 50.0), (31.0, 50.0), (26.0, 62.0), (41.0, 46.0), (34.0, 46.0), (39.0, 36.0)]));
        }
        Blizzard => {
            c.part(22.0, 24.0, 0.55, 0.0, 0.0, |c| {
                c.add(radial(3, 32.0, 32.0, || seg(32.0, 6.0, 32.0, 58.0, 6.0)));
                c.add(radial(6, 32.0, 32.0, || polyline(&[(24.0, 11.0), (32.0, 19.0), (40.0, 11.0)], 5.5)));
            });
            c.add(wave(30.0, 60.0, 20.0, 2.5, 1.0, W));
            c.add(wave(8.0, 60.0, 46.0, 2.5, 1.5, W));
            c.add(wave(16.0, 54.0, 57.0, 2.5, 1.0, W));
        }
        Remove => {
            // A bin: lid with a handle, and a tapered body with two grooves.
            c.add(rect_c(8.0, 13.0, 56.0, 20.0, 2.0));
            c.add(rect_c(24.0, 6.0, 40.0, 13.0, 2.0).outline(4.0));
            c.add(poly(&[(13.0, 24.0), (51.0, 24.0), (47.0, 58.0), (17.0, 58.0)]).grow(1.0));
            c.cut(seg(26.0, 31.0, 27.0, 51.0, 4.0).or(seg(38.0, 31.0, 37.0, 51.0, 4.0)));
        }
        Next => {
            c.add(seg(8.0, 32.0, 44.0, 32.0, 8.0));
            c.add(polyline(&[(32.0, 14.0), (52.0, 32.0), (32.0, 50.0)], 8.0));
        }
        Site => {
            // Two wooden stakes with a slack surveyor cord and a starter pile of sticks between them.
            c.add(seg(13.0, 13.0, 13.0, 55.0, 6.5));
            c.add(seg(51.0, 13.0, 51.0, 55.0, 6.5));
            c.add(qbez((13.0, 20.0), (32.0, 31.0), (51.0, 20.0), 4.0));
            c.add(poly(&[(19.0, 55.0), (32.0, 39.0), (45.0, 55.0)]).grow(1.5));
            c.add(seg(6.0, 55.0, 58.0, 55.0, 4.5));
        }
        Building => lodge_dome(c),
        Home => {
            lodge_silhouette(c);
            c.cut_part(32.0, 41.0, 0.42, 0.0, 0.0, |c| c.add(heart()));
        }
        Upstream => {
            c.add(wave(17.0, 57.0, 32.0, 4.5, 1.0, W + 0.5));
            c.add(polyline(&[(27.0, 15.0), (10.0, 32.0), (27.0, 49.0)], W + 0.5));
        }
        Gauge => {
            c.add(rect_c(12.0, 7.0, 40.0, 57.0, 8.0).outline(W));
            c.add(wave_fill(18.5, 33.5, 30.0, 51.0, 2.0, 1.0).and(rect_c(18.5, 13.0, 33.5, 50.5, 3.0)));
            c.add(seg(48.0, 16.0, 56.0, 16.0, 4.5).or(seg(48.0, 32.0, 54.0, 32.0, 4.5)).or(seg(48.0, 48.0, 56.0, 48.0, 4.5)));
        }
        Widen => {
            c.add(rect_c(10.0, 14.0, 54.0, 27.0, 3.0));
            c.add(rect_c(10.0, 37.0, 54.0, 50.0, 3.0));
            c.add(tri_rounded(&[(32.0, 64.0), (24.0, 54.0), (40.0, 54.0)]));
        }
        Ledger => {
            c.add(rect_c(6.0, 53.0, 58.0, 58.0, 2.0));
            c.add(rect_c(10.0, 34.0, 22.0, 50.0, 2.0));
            c.add(rect_c(26.0, 20.0, 38.0, 50.0, 2.0));
            c.add(rect_c(42.0, 8.0, 54.0, 50.0, 2.0));
        }
        Territory => {
            // A compass rose: outer ring with cardinal ticks and a tilted diamond needle.
            c.add(ring(32.0, 32.0, 24.5, 5.5));
            c.add(
                seg(32.0, 4.0, 32.0, 12.0, 4.5)
                    .or(seg(32.0, 52.0, 32.0, 60.0, 4.5))
                    .or(seg(4.0, 32.0, 12.0, 32.0, 4.5))
                    .or(seg(52.0, 32.0, 60.0, 32.0, 4.5)),
            );
            c.add(poly(&[(47.0, 17.0), (37.5, 37.5), (17.0, 47.0), (26.5, 26.5)]).grow(0.8));
            c.cut(circle(32.0, 32.0, 4.2));
        }
        Journal => {
            c.add(poly(&[(5.0, 12.0), (29.0, 18.0), (29.0, 56.0), (5.0, 50.0)]).grow(1.0));
            c.add(poly(&[(35.0, 18.0), (59.0, 12.0), (59.0, 50.0), (35.0, 56.0)]).grow(1.0));
            for y in [26.0, 35.0, 44.0] {
                c.cut(seg(10.0, y - 3.0, 24.0, y, 3.0).or(seg(40.0, y, 54.0, y - 3.0, 3.0)));
            }
        }
        Extend => {
            c.add(seg(14.0, 32.0, 50.0, 32.0, 6.0));
            c.add(tri_rounded(&[(4.0, 32.0), (18.0, 20.0), (18.0, 44.0)]));
            c.add(tri_rounded(&[(60.0, 32.0), (46.0, 20.0), (46.0, 44.0)]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_icons_rasterize_non_empty() {
        for &icon in Icon::ALL {
            let alpha = rasterize_icon(icon);
            let lit = alpha.iter().filter(|&&a| a > 128).count();
            assert!(lit > 40, "icon {icon:?} has too few lit pixels ({lit})");
        }
        if let Ok(path) = std::env::var("BC_DUMP_ICONS_RAW") {
            let (w, h, rgba) = rasterize_atlas();
            let mut out = Vec::with_capacity(8 + rgba.len());
            out.extend_from_slice(&w.to_le_bytes());
            out.extend_from_slice(&h.to_le_bytes());
            out.extend_from_slice(&rgba);
            std::fs::write(path, out).expect("write raw atlas");
        }
    }
}
